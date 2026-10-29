//! The Check screen's reads (unit B5a): the tool picker, a tool's check session and its rail,
//! and the guided fix's offer. Nothing here writes the project (tC4 #501).
//!
//! - **Preflight** is tc-check's (`tc_check::fix::preflight`, tC4's `preflightToolBook`) over
//!   the project's pins, which are the journal's D30 ladder, and the resources installed on
//!   this computer. tC4 asks it with `online = true` (D88): a missing pin is always a fetch,
//!   whose download asks before it uses the internet.
//! - **The list** of a ready tool derives from the book's Door43 helps, the pins' resolution,
//!   whatever helps the reading panes show (Q33: Aquifer is for reading only). It is kept in the
//!   session's [`CheckCache`] under its key (the resolved resource, the project's numbering and
//!   the book's scope) and derived again only when the key changes, which starts a new check
//!   session (§4.4).
//! - **The rows** carry each check's state from the fold's decisions placed on the list
//!   (`tc_check::session::open`). Each row keeps the view of the snapshot that last changed it,
//!   so a UI that holds the rail at a view gets only the rows changed since (AD17, A-12). A
//!   snapshot that moved re-assesses only the checks its changes reach (C4, PERF:79-80): the
//!   session's event pump logs the keys of each change, and while every decision of the list
//!   sits on its own check, a decision re-assesses its check and a verse the decided checks of
//!   its chapter (a bridge can move the text of the verses after it). Anything else (a
//!   decision that would be placed on another check, a pin, a gap in the log) recomputes
//!   every row, as the open does, and derives nothing.
//! - **Titles** are the gateway words that render each quote in the literal source pane, read at
//!   the place the resource gives the check (its own numbering), else the original quote (tC4
//!   `gatewayTitleFor`).
//! - **The item detail and the marks** (unit B5b) are in [`item`]: `check/item`, `check/article`
//!   and `check/decide`.

mod item;

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tc_api::ApiError;
use tc_api::check::{
    AlignProgress, CardCount, Category, CheckEmpty, CheckListRef, CheckPlace, CheckProgress,
    CheckSessionView, CheckUnit, Dot, DroppedNote, NextCheck, NoteCategory, Open, Picker,
    PickerView, PinCitation, RailProgress, RailRow, RailRowPatch, RailRows, RailSince,
    ResourceCitation, RowFlags, Rows, RowsView, Rung, Tool, ToolCard, ToolState, WordCategory,
};
use tc_api::ids::{CheckSessionId, StoryNo, ViewSeq};
use tc_check::derive::{
    BookSpec, CheckList, notes_for_book, notes_for_stories, word_links_for_book,
    word_links_for_stories,
};
use tc_check::fix::{self, Machine, Preflight, PreflightState};
use tc_check::{CheckItem, CheckKey, CheckState, CheckTarget, Collection, Ladder, Pin, ToolId};
use tc_core::BookId;
use tc_helps::{HelpsPart, Occurrence};
use tc_journal::fold::{Delta, ProjectedDecision};
use tc_journal::record::{BookPresence, ResourceRef, Scope as BookScope};
use tc_journal::{Record, RecordKey};
use tc_resources::{Missing, PartStatus, ProjectPins};

use crate::app::App;
use crate::counters::{AppCounter, CountingSnapshot};
use crate::dispatch::Handle;
use crate::numbering::Numbering;
use crate::plans::core_unit;
use crate::session::ProjectSession;
use crate::story::LoadedObs;
use crate::understand::{LoadedBook, LoadedText, project_pins, versification};

/// The tools of the picker, in tC4's order (`TOOL_SLOT`).
const TOOLS: [Tool; 2] = [Tool::TranslationWords, Tool::TranslationNotes];

fn tool_id(tool: Tool) -> ToolId {
    match tool {
        Tool::TranslationWords => ToolId::TranslationWords,
        Tool::TranslationNotes => ToolId::TranslationNotes,
    }
}

fn rung(rung: tc_check::Rung) -> Rung {
    match rung {
        tc_check::Rung::Primary => Rung::Primary,
        tc_check::Rung::Fallback => Rung::Fallback,
    }
}

fn citation(pin: &Pin) -> PinCitation {
    PinCitation {
        repo_path: pin.resource.repo_path.to_string(),
        version: pin.version.clone(),
        sha: pin.resource.sha.to_hex(),
    }
}

fn state(state: PreflightState) -> ToolState {
    match state {
        PreflightState::Ready => ToolState::Ready,
        // D88: tC4 asks with the internet on, so a missing pin is a fetch.
        PreflightState::Fetch | PreflightState::Unavailable => ToolState::Fetch,
        PreflightState::Unpinned => ToolState::Unpinned,
        PreflightState::NotCovered => ToolState::NotCovered,
        PreflightState::OldWordLinks => ToolState::OldTwl,
        PreflightState::UnusablePin => ToolState::UnusablePin,
    }
}

fn category(category: tc_check::Category) -> Category {
    use tc_check::NoteCategory as N;
    use tc_helps::WordCategory as W;
    match category {
        tc_check::Category::Note(n) => Category::Note(match n {
            N::Discourse => NoteCategory::Discourse,
            N::Numbers => NoteCategory::Numbers,
            N::Figures => NoteCategory::Figures,
            N::Culture => NoteCategory::Culture,
            N::Grammar => NoteCategory::Grammar,
            N::Other => NoteCategory::Other,
        }),
        tc_check::Category::Word(w) => Category::Word(match w {
            W::KeyTerm => WordCategory::Kt,
            W::Name => WordCategory::Names,
            W::Other => WordCategory::Other,
        }),
    }
}

/// A check's place on the wire: the text tc-check prints for its target.
pub(crate) fn place(target: &CheckTarget) -> Result<CheckPlace, String> {
    target
        .to_string()
        .parse()
        .map_err(|e| format!("{target}: {e}"))
}

fn view_of(read: &CountingSnapshot) -> ViewSeq {
    crate::wire::scope::view(read.seq())
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// What a list was derived under: another resolution, numbering or scope derives it again.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListKey {
    resolution: ResourceRef,
    numbering: Option<&'static str>,
    scope: Option<BookScope>,
}

/// A tool's derived list: everything that does not move with the decisions.
struct Derived {
    items: Vec<CheckItem>,
    /// Item indices in the order By verse lists them (tC4's stable reference sort).
    order: Vec<u32>,
    terms: Vec<String>,
    term_of: Vec<Option<u32>>,
    /// Each term's heading By figure: a tN figure's Translation Academy title (tC4 #769).
    group_titles: Vec<String>,
    titles: Vec<String>,
    places: Vec<CheckPlace>,
    dropped: Option<DroppedNote>,
    citation: ResourceCitation,
    /// The resource the list derives from, which assesses a decision shown on another key.
    resolution: ResourceRef,
    empty: Option<CheckEmpty>,
    /// Each check by the key its decision is stored under; `None` when two checks share one,
    /// and a change of a decision cannot name its check.
    by_key: Option<ByKey>,
    /// The checks of each chapter of a book, or of each frame of a story: a change of the
    /// text there moves their assessments.
    by_part: ByPart,
}

type ByKey = HashMap<CheckKey, usize>;
type ByPart = BTreeMap<u16, Vec<usize>>;

/// The chapter of a verse's check, or the frame of a story's: where its text is.
fn part_of(target: &CheckTarget) -> Option<u16> {
    match target {
        CheckTarget::Verse { chapter, .. } => chapter.number(),
        CheckTarget::Frame { frame, .. } => Some(frame.get()),
    }
}

/// The two indexes a re-assessment looks a change up in.
fn indexes(items: &[CheckItem]) -> (Option<ByKey>, ByPart) {
    let mut by_key = HashMap::with_capacity(items.len());
    let mut unique = true;
    let mut by_part = ByPart::new();
    for (i, item) in items.iter().enumerate() {
        unique &= by_key.insert(item.key.clone(), i).is_none();
        if let Some(part) = part_of(&item.key.target) {
            by_part.entry(part).or_default().push(i);
        }
    }
    (unique.then_some(by_key), by_part)
}

/// One list in a session's cache, with its rows as of `seq`.
struct ListEntry {
    key: ListKey,
    session: CheckSessionId,
    derived: Arc<Derived>,
    /// The snapshot the rows were computed from.
    seq: u64,
    /// By item index.
    rows: Vec<RailRow>,
    /// The view of the snapshot that last changed each row.
    stamps: Vec<ViewSeq>,
    /// The view the list was derived at: a UI's view older than it cannot be patched.
    built: ViewSeq,
    /// The fold's decisions of the list's tool and book as of `seq`, and for each check the one
    /// shown on it (an index into `decisions`): what `check/item` shows of a check.
    states: Vec<CheckState>,
    decisions: Vec<ProjectedDecision>,
    shown: Vec<Option<usize>>,
    /// Where a change can be re-assessed alone; `None` when the list cannot tell (two checks
    /// share a key), and every change recomputes the rows.
    placing: Option<Placing>,
}

/// What the placement of a list's decisions depends on, for a re-assessment (`reassess`).
/// `tc_check::carry` places a decision by its key, its quote and its group id, never by its
/// text or its judgement; so a change of a decision that keeps those three, or of a verse,
/// moves no decision, and only the states of the checks it reaches change.
struct Placing {
    /// Each decision's index in `ListEntry::decisions`, by its key.
    at: HashMap<CheckKey, usize>,
    /// The checks each decision is shown on.
    shown_on: HashMap<usize, Vec<usize>>,
    /// The decisions not placed on the check of their own key and quote: off the list, or
    /// moved by the carry's second pass. A first decision on a check one of them could reach
    /// (by its id or its place) can move it, so it recomputes the rows.
    waiting: Vec<usize>,
}

impl ListEntry {
    fn progress(&self) -> RailProgress {
        RailProgress {
            decided: count(self.rows.iter().filter(|r| r.flags.decided).count()),
            total: count(self.rows.len()),
        }
    }

    /// The first check not decided, in the list's own order.
    fn first_undecided(&self) -> Option<usize> {
        self.rows.iter().position(|r| !r.flags.decided)
    }
}

/// The check lists a session has derived (`CheckListCache`, §4.4). The lock is held across a
/// derivation, so two reads of one list at once derive it once.
#[derive(Default)]
pub(crate) struct CheckCache {
    lists: tokio::sync::Mutex<HashMap<CheckListRef, ListEntry>>,
    log: std::sync::Mutex<ChangeLog>,
    logged: tokio::sync::Notify,
}

/// The keys of the changes after a snapshot, as the session's event pump receives them: what a
/// list held at that snapshot re-assesses.
#[derive(Default)]
struct ChangeLog {
    /// The log holds every change after this snapshot; `None` before the pump starts.
    from: Option<u64>,
    /// The newest snapshot whose change the log holds.
    upto: u64,
    /// Each change's snapshot and what of it a check list reads; `None` for a change that can
    /// move any check (a rebuilt fold, a pin).
    steps: VecDeque<(u64, Option<Changes>)>,
    keys: usize,
}

/// What a run of changes touched that a check list reads: the keys, and the chapters whose
/// text the fold projected again. A chapter can move without a verse key of its own (a
/// restore race lists the verses typed after it), so the chapters are kept beside the keys.
#[derive(Default)]
struct Changes {
    keys: Vec<RecordKey>,
    chapters: Vec<(BookId, u16)>,
}

impl Changes {
    fn len(&self) -> usize {
        self.keys.len() + self.chapters.len()
    }
}

/// How many keys the log keeps: a list older than that is recomputed whole, as at its open.
const LOG_KEYS: usize = 100_000;
/// How long a re-assessment waits for the pump to log a snapshot the request has already
/// read. The pump logs before it tells the UI, so only a write's own reply waits, briefly.
const LOG_WAIT: Duration = Duration::from_secs(2);

/// A key whose change can move a check's state or the list's placement.
fn read_by_checks(key: &RecordKey) -> bool {
    matches!(
        key,
        RecordKey::Check(_)
            | RecordKey::Verse(..)
            | RecordKey::ChapterFront(..)
            | RecordKey::BookPresence(_)
            | RecordKey::Frame(..)
            | RecordKey::StoryBase(_)
            | RecordKey::Pins(_)
            | RecordKey::ResourcesExtra(_)
    )
}

impl CheckCache {
    /// The pump starts at snapshot `seq`: every change after it reaches [`Self::note`].
    pub(crate) fn begin(&self, seq: u64) {
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        *log = ChangeLog {
            from: Some(seq),
            upto: seq,
            ..ChangeLog::default()
        };
        drop(log);
        self.logged.notify_waiters();
    }

    /// The change folded into snapshot `seq`.
    pub(crate) fn note(&self, seq: u64, delta: &Delta) {
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        let Some(from) = log.from else {
            return;
        };
        if seq <= from {
            return;
        }
        let step = (!delta.everything && !delta.pins).then(|| Changes {
            keys: delta
                .keys
                .iter()
                .filter(|key| read_by_checks(key))
                .cloned()
                .collect(),
            chapters: delta.chapters.clone(),
        });
        log.keys += step.as_ref().map_or(1, Changes::len);
        log.steps.push_back((seq, step));
        log.upto = log.upto.max(seq);
        while log.keys > LOG_KEYS {
            let Some((dropped, step)) = log.steps.pop_front() else {
                break;
            };
            log.keys = log
                .keys
                .saturating_sub(step.as_ref().map_or(1, Changes::len));
            log.from = Some(dropped);
        }
        drop(log);
        self.logged.notify_waiters();
    }

    /// What the changes after snapshot `after` up to snapshot `upto` touched; `None` when the
    /// log cannot say (it starts later, the pump has not logged `upto` in time, or a change can
    /// move any check).
    async fn changes(&self, after: u64, upto: u64) -> Option<Changes> {
        let deadline = tokio::time::Instant::now() + LOG_WAIT;
        loop {
            let notified = self.logged.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let log = self.log.lock().unwrap_or_else(|e| e.into_inner());
                if log.from? > after {
                    return None;
                }
                if log.upto >= upto {
                    let mut all = Changes::default();
                    for (seq, step) in &log.steps {
                        if *seq > after && *seq <= upto {
                            let step = step.as_ref()?;
                            all.keys.extend(step.keys.iter().cloned());
                            all.chapters.extend(step.chapters.iter().copied());
                        }
                    }
                    return Some(all);
                }
            }
            tokio::time::timeout_at(deadline, notified).await.ok()?;
        }
    }
}

/// Numbers the check sessions of the whole server run, so an id is never reused.
static SESSIONS: AtomicU64 = AtomicU64::new(0);

fn next_session() -> CheckSessionId {
    CheckSessionId::new(SESSIONS.fetch_add(1, Ordering::Relaxed) + 1)
}

/// The resources installed here as tc-check's preflight reads them. A store that cannot be
/// listed is stated (tC4's `preflightError`), never read as "nothing installed".
fn installed(app: &App) -> Result<Vec<fix::Installed>, ApiError> {
    let pins = app.inner.resources.installed_dcs_pins().map_err(|e| {
        app.internal(
            "the installed resources could not be listed",
            crate::reports::facts(serde_json::json!({ "error": e.to_string() })),
        )
    })?;
    Ok(pins
        .iter()
        .map(|pin| {
            let journal = pin.journal_pin();
            fix::Installed {
                resource: journal.resource,
                version: journal.version,
                coverage: journal.coverage,
            }
        })
        .collect())
}

fn pins_of(read: &CountingSnapshot) -> Result<Option<ProjectPins>, ApiError> {
    project_pins(read).map_err(|reason| ApiError::BadRequest {
        field: "pins".to_owned(),
        reason,
    })
}

/// What a unit is called in a report.
fn unit_name(unit: CheckUnit) -> String {
    match unit {
        CheckUnit::Book(book) => book.code().to_owned(),
        CheckUnit::Story(story) => format!("story {}", story.get()),
    }
}

fn preflight(
    pins: Option<&ProjectPins>,
    tool: Tool,
    unit: CheckUnit,
    installed: &[fix::Installed],
) -> Preflight {
    let machine = Machine {
        installed,
        online: true,
    };
    fix::preflight(
        pins.map(|p| p as &dyn Ladder),
        tool_id(tool),
        core_unit(unit),
        &machine,
    )
}

/// The book's scope from its presence record; `None` when the book is not present.
fn book_scope(read: &CountingSnapshot, book: BookId) -> Option<BookScope> {
    match read.projected(&RecordKey::BookPresence(book)) {
        Some(Record::BookPresence {
            value: BookPresence::Present { scope, .. },
            ..
        }) => Some(scope),
        _ => None,
    }
}

/// The gateway words that render `item`'s quote in the literal pane, at the place the
/// resource gives the check, else the original quote.
fn title(
    loaded: &LoadedBook,
    literal: Option<&crate::sources::PaneBook>,
    item: &CheckItem,
) -> String {
    let occurrence = Occurrence::nth(item.key.occurrence).unwrap_or(Occurrence::NotTied);
    literal
        .zip(crate::understand::parts(&item.source))
        .and_then(|(pane, (chapter, verse))| {
            let chapter = chapter.as_str().parse::<u16>().ok()?;
            pane.chapters
                .get(&chapter)?
                .focus(verse.as_str(), &item.quote, occurrence)
                .0
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            let _ = loaded;
            item.original_title()
        })
}

/// Derives `tool`'s list of `book` from the loaded resources (tC4 `deriveCheckItems`).
fn derive(
    loaded: &LoadedBook,
    tool: Tool,
    book: BookId,
    numbering: Numbering,
    scope: Option<&BookScope>,
    resolution: (&Pin, tc_check::Rung),
) -> Result<Derived, String> {
    let citation = ResourceCitation {
        pin: citation(resolution.0),
        rung: rung(resolution.1),
    };
    let empty_with = |empty, dropped| empty_derived(&citation, resolution.0, empty, dropped);
    let part = match tool {
        Tool::TranslationNotes => HelpsPart::Notes,
        Tool::TranslationWords => HelpsPart::WordLinks,
    };
    match loaded.resources.parts.get(&part) {
        Some(PartStatus::Loaded { .. }) => {}
        Some(PartStatus::Missing(Missing::BookNotInResource { .. })) => {
            return Ok(empty_with(CheckEmpty::Missing, None));
        }
        Some(PartStatus::Missing(missing)) => return Err(format!("{missing:?}")),
        None => return Err(format!("no {part:?} were read")),
    }
    let Some(project_frame) = numbering.project_scheme() else {
        return Ok(empty_with(CheckEmpty::VersificationUnknown, None));
    };
    let ranges = match scope {
        Some(BookScope::Ranges(ranges)) => ranges.as_slice(),
        Some(BookScope::Whole) | None => &[],
    };
    let spec = BookSpec {
        book,
        resource_frame: crate::numbering::SOURCES.scheme(),
        project_frame: Some(project_frame),
        scope: ranges,
    };
    let helps = &loaded.resources.helps;
    let list: CheckList = match tool {
        Tool::TranslationNotes => notes_for_book(helps.notes(), &spec),
        Tool::TranslationWords => {
            // The links whose article the resolving set's Translation Words carry (#643); with
            // no article read, none is held back.
            let articles = loaded
                .resources
                .parts
                .get(&HelpsPart::WordArticles)
                .filter(|p| p.is_loaded())
                .map(|_| {
                    helps
                        .word_links()
                        .iter()
                        .filter(|l| helps.word_article(&l.article).is_some())
                        .map(|l| l.article.clone())
                        .collect()
                });
            word_links_for_book(helps.word_links(), articles.as_ref(), &spec)
        }
    };
    let dropped = (!list.dropped.is_empty()).then(|| DroppedNote {
        count: count(list.dropped.len()),
        scheme: Some(project_frame.name().to_owned()),
    });
    if list.items.is_empty() {
        let empty = if dropped.is_some() {
            CheckEmpty::AllDropped
        } else {
            CheckEmpty::None
        };
        return Ok(empty_with(empty, dropped));
    }
    let ids = loaded.pane_ids();
    let literal = crate::understand::literal_pane(&ids)
        .and_then(|id| loaded.pane(&id))
        .and_then(|text| match text {
            LoadedText::Ready(book) => Some(book),
            _ => None,
        });
    assemble(
        list.items,
        dropped,
        citation,
        resolution.0,
        |item| title(loaded, literal, item),
        |slug| academy_title(helps, slug),
    )
}

/// A figure's heading: its Translation Academy article's title, else its slug (tC4
/// `readTaTitle`).
fn academy_title(helps: &tc_helps::Helps, slug: &str) -> String {
    tc_helps::Slug::parse(slug)
        .ok()
        .and_then(|slug| {
            crate::understand::MANUALS.iter().find_map(|manual| {
                helps
                    .academy_article(&tc_helps::AcademyArticleId::Dcs {
                        manual: *manual,
                        slug: slug.clone(),
                    })
                    .map(|found| found.item.title.clone())
            })
        })
        .unwrap_or_else(|| slug.to_owned())
}

/// Derives `tool`'s list of one story from the loaded story resources: the open story's
/// notes or word links (tC4 `deriveObsItems`). A story has no numbering to map and no scope.
fn derive_story(
    loaded: &LoadedObs,
    tool: Tool,
    story: StoryNo,
    resolution: (&Pin, tc_check::Rung),
) -> Result<Derived, String> {
    let citation = ResourceCitation {
        pin: citation(resolution.0),
        rung: rung(resolution.1),
    };
    let part = match tool {
        Tool::TranslationNotes => HelpsPart::ObsNotes,
        Tool::TranslationWords => HelpsPart::ObsWordLinks,
    };
    match loaded.resources.parts.get(&part) {
        Some(PartStatus::Loaded { .. }) => {}
        Some(PartStatus::Missing(missing)) => return Err(format!("{missing:?}")),
        None => return Err(format!("no {part:?} were read")),
    }
    let helps = &loaded.resources.helps;
    let list = match tool {
        Tool::TranslationNotes => notes_for_stories(helps.notes(), Some(story.get())),
        Tool::TranslationWords => word_links_for_stories(helps.word_links(), Some(story.get())),
    };
    if list.items.is_empty() {
        return Ok(empty_derived(
            &citation,
            resolution.0,
            CheckEmpty::None,
            None,
        ));
    }
    // A story has one source text and no original language: a check is titled by its quote,
    // which is the gateway story's own words.
    assemble(
        list.items,
        None,
        citation,
        resolution.0,
        CheckItem::original_title,
        |slug| academy_title(helps, slug),
    )
}

/// A list with no check to show.
fn empty_derived(
    citation: &ResourceCitation,
    resolution: &Pin,
    empty: CheckEmpty,
    dropped: Option<DroppedNote>,
) -> Derived {
    Derived {
        items: Vec::new(),
        order: Vec::new(),
        terms: Vec::new(),
        term_of: Vec::new(),
        group_titles: Vec::new(),
        titles: Vec::new(),
        places: Vec::new(),
        dropped,
        citation: citation.clone(),
        resolution: resolution.resource.clone(),
        empty: Some(empty),
        by_key: Some(HashMap::new()),
        by_part: BTreeMap::new(),
    }
}

/// The list's rows' fixed parts, from its items.
fn assemble(
    items: Vec<CheckItem>,
    dropped: Option<DroppedNote>,
    citation: ResourceCitation,
    resolution: &Pin,
    title_of: impl Fn(&CheckItem) -> String,
    group_title: impl Fn(&str) -> String,
) -> Result<Derived, String> {
    let places = items
        .iter()
        .map(|item| place(&item.key.target))
        .collect::<Result<Vec<_>, _>>()?;
    let mut order: Vec<u32> = (0..count(items.len())).collect();
    // The rail's reference sort (tc-check's, tC4's `railGroupsOf`): stable, so the checks of
    // one verse keep the resource's order.
    tc_check::rail::sort_by_reference(&items, &mut order);
    // The group ids of both tools: By word groups a tW rail by them, and every row's tooltip
    // names its own.
    let mut slugs: Vec<&str> = items.iter().map(|i| i.group_id.as_str()).collect();
    slugs.sort_by(|a, b| tc_check::rail::locale_cmp(a, b));
    slugs.dedup();
    let at: HashMap<&str, u32> = slugs
        .iter()
        .enumerate()
        .map(|(i, s)| (*s, count(i)))
        .collect();
    let term_of: Vec<Option<u32>> = items
        .iter()
        .map(|item| at.get(item.group_id.as_str()).copied())
        .collect();
    let terms: Vec<String> = slugs.into_iter().map(str::to_owned).collect();
    let group_titles = terms.iter().map(|slug| group_title(slug)).collect();
    let titles = items.iter().map(title_of).collect();
    let (by_key, by_part) = indexes(&items);
    Ok(Derived {
        by_key,
        by_part,
        items,
        order,
        terms,
        term_of,
        group_titles,
        titles,
        places,
        dropped,
        citation,
        resolution: resolution.resource.clone(),
        empty: None,
    })
}

fn flags(state: &CheckState) -> RowFlags {
    RowFlags {
        dot: match state.dot() {
            tc_check::Dot::Valid => Dot::Valid,
            tc_check::Dot::Invalid => Dot::Invalid,
            tc_check::Dot::Todo => Dot::Todo,
        },
        decided: state.is_decided(),
        invalidated: state.invalidation().is_some(),
        invalid: state.status() == Some(tc_check::UserStatus::Invalid),
        bookmarked: state.bookmarked,
        commented: state.commented,
    }
}

/// The checks of a list as a snapshot shows their decisions.
struct Assessed {
    rows: Vec<RailRow>,
    states: Vec<CheckState>,
    decisions: Vec<ProjectedDecision>,
    shown: Vec<Option<usize>>,
    placing: Option<Placing>,
}

/// What `tc_check::carry` places a decision by, besides its key: its quote and its group id.
fn placed_by(decision: &ProjectedDecision) -> (String, String) {
    let stored = tc_check::carry::StoredDecision::of(decision.key.clone(), decision.record.clone());
    (stored.quote, stored.group_id)
}

/// The place a carry's second pass matches a decision or a check by (`carry::place_of`).
fn place_key(target: &CheckTarget, quote: &str, occurrence: u32) -> (CheckTarget, String, u32) {
    (target.clone(), quote.to_owned(), occurrence)
}

/// The row of check `i` as `state` shows it.
fn row_of(derived: &Derived, i: usize, state: &CheckState) -> RailRow {
    RailRow {
        index: count(i),
        check_id: derived.items[i].key.check_id.as_str().to_owned(),
        place: derived.places[i].clone(),
        title: derived.titles[i].clone(),
        flags: flags(state),
        term: derived.term_of[i],
        category: Some(category(derived.items[i].category)),
        marker: None,
    }
}

/// Each check's row as the snapshot `read` shows its decisions.
fn rows(read: &CountingSnapshot, derived: &Derived, tool: Tool, unit: CheckUnit) -> Assessed {
    if derived.items.is_empty() {
        return Assessed {
            rows: Vec::new(),
            states: Vec::new(),
            decisions: Vec::new(),
            shown: Vec::new(),
            placing: Some(Placing {
                at: HashMap::new(),
                shown_on: HashMap::new(),
                waiting: Vec::new(),
            }),
        };
    }
    let collection = core_unit(unit);
    // A story's session reads that story's decisions only.
    let decisions = match unit {
        CheckUnit::Book(_) => read.check_decisions(tool_id(tool), collection),
        CheckUnit::Story(story) => match tc_journal::StoryNo::new(story.get()) {
            Some(story) => read.story_decisions(tool_id(tool), story),
            None => Vec::new(),
        },
    };
    // A decision shown under another key than its own is assessed against that check's text,
    // read once per target the placement asks for.
    let texts = read.check_texts(collection);
    let targets: HashMap<&CheckTarget, usize> = derived
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| (&item.key.target, i))
        .collect();
    let cells: Vec<OnceCell<Option<Arc<tc_check::TargetText>>>> =
        (0..derived.items.len()).map(|_| OnceCell::new()).collect();
    let text = |target: &CheckTarget| {
        let i = *targets.get(target)?;
        cells.get(i)?.get_or_init(|| texts.text(target)).as_deref()
    };
    let session = tc_check::session::open(
        derived.items.clone(),
        &decisions,
        Some(&derived.resolution),
        text,
    );
    let rows = session
        .states
        .iter()
        .enumerate()
        .map(|(i, state)| row_of(derived, i, state))
        .collect();
    let shown = (0..session.states.len())
        .map(|i| session.shown(i))
        .collect();
    let placing = derived.by_key.as_ref().map(|_| {
        let mut shown_on: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut identity = vec![false; decisions.len()];
        for (i, on) in session.placed.iter().enumerate() {
            if let Some(&d) = on.last() {
                shown_on.entry(d).or_default().push(i);
            }
            for &d in on {
                identity[d] |= decisions[d].key == derived.items[i].key
                    && placed_by(&decisions[d]).0 == derived.items[i].quote.as_str();
            }
        }
        Placing {
            at: decisions
                .iter()
                .enumerate()
                .map(|(d, decision)| (decision.key.clone(), d))
                .collect(),
            shown_on,
            waiting: (0..decisions.len()).filter(|&d| !identity[d]).collect(),
        }
    });
    Assessed {
        rows,
        states: session.states,
        decisions,
        shown,
        placing,
    }
}

/// Re-assesses the checks the changed `keys` reach (C4, PERF:79-80), when no change can move a
/// decision (`Placing`): the checks a changed decision is shown on, a first decision's own
/// check, and the decided checks of a chapter whose verse or front changed (of a frame, or of
/// every frame for a story's base text). Each state is computed as the open computes it: a
/// decision on its own key with the fold's assessment over those chapters' text
/// (`decisions_at`), one shown under another key against the check's text there. `false` when a
/// change could move a decision, and the entry is as it was.
fn reassess(
    app: &App,
    read: &CountingSnapshot,
    entry: &mut ListEntry,
    list: CheckListRef,
    changes: &Changes,
) -> bool {
    let derived = entry.derived.clone();
    let (Some(by_key), Some(placing)) = (derived.by_key.as_ref(), entry.placing.as_ref()) else {
        return false;
    };
    let tool = tool_id(list.tool);
    let collection = core_unit(list.unit);
    let mut decisions: BTreeSet<CheckKey> = BTreeSet::new();
    let mut parts: BTreeSet<u16> = BTreeSet::new();
    let mut every = false;
    if let CheckUnit::Book(open) = list.unit {
        parts.extend(
            changes
                .chapters
                .iter()
                .filter(|(book, _)| *book == open)
                .map(|&(_, chapter)| chapter),
        );
    }
    for key in &changes.keys {
        match (key, list.unit) {
            (RecordKey::Check(check), _) => {
                if check.tool == tool && check.target.collection() == collection {
                    decisions.insert(check.clone());
                }
            }
            (
                RecordKey::Verse(book, chapter, _) | RecordKey::ChapterFront(book, chapter),
                CheckUnit::Book(open),
            ) if *book == open => {
                parts.insert(chapter.get());
            }
            (RecordKey::BookPresence(book), CheckUnit::Book(open)) if *book == open => {
                return false;
            }
            (RecordKey::Frame(story, frame), CheckUnit::Story(open))
                if story.get() == open.get() =>
            {
                parts.insert(frame.get());
            }
            (RecordKey::StoryBase(story), CheckUnit::Story(open)) if story.get() == open.get() => {
                every = true;
            }
            (RecordKey::Pins(_) | RecordKey::ResourcesExtra(_), _) => return false,
            _ => {}
        }
    }
    // The checks to assess again: each with the decision it shows, by key.
    let decided = |i: usize| entry.shown.get(i).copied().flatten();
    let mut checks: BTreeMap<usize, CheckKey> = BTreeMap::new();
    let show = |i: usize, d: usize, checks: &mut BTreeMap<usize, CheckKey>| {
        if let Some(decision) = entry.decisions.get(d) {
            checks.insert(i, decision.key.clone());
        }
    };
    if every {
        for i in 0..derived.items.len() {
            if let Some(d) = decided(i) {
                show(i, d, &mut checks);
            }
        }
    }
    for part in &parts {
        for &i in derived.by_part.get(part).into_iter().flatten() {
            if let Some(d) = decided(i) {
                show(i, d, &mut checks);
            }
        }
    }
    // A first decision lands on its own check, which must have shown none.
    let mut first: Vec<(usize, CheckKey)> = Vec::new();
    for key in &decisions {
        match placing.at.get(key) {
            Some(&d) => {
                for &i in placing.shown_on.get(&d).into_iter().flatten() {
                    checks.insert(i, key.clone());
                }
            }
            None => match by_key.get(key) {
                Some(&i) if decided(i).is_none() => first.push((i, key.clone())),
                // Off the list, or onto a check another decision shows: the carry places it.
                _ => return false,
            },
        }
    }
    if checks.is_empty() && first.is_empty() {
        return true;
    }
    let chapters: Vec<u16> = match list.unit {
        CheckUnit::Book(_) => checks
            .keys()
            .chain(first.iter().map(|(i, _)| i))
            .filter_map(|&i| part_of(&derived.items[i].key.target))
            .collect::<BTreeSet<u16>>()
            .into_iter()
            .collect(),
        CheckUnit::Story(_) => Vec::new(),
    };
    let wanted: Vec<RecordKey> = checks
        .values()
        .chain(decisions.iter())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|key| RecordKey::Check(key.clone()))
        .collect();
    let now = read.decisions_at(tool, collection, &wanted, &chapters);
    let found: HashMap<&CheckKey, &ProjectedDecision> = now.iter().map(|d| (&d.key, d)).collect();
    // Every decision the change names keeps what it is placed by, and each first decision is
    // on its own check and quote, out of reach of the decisions the carry moved.
    for key in &decisions {
        let Some(new) = found.get(key) else {
            return false;
        };
        match placing.at.get(key) {
            Some(&d) => {
                if entry.decisions.get(d).map(placed_by) != Some(placed_by(new)) {
                    return false;
                }
            }
            None => {
                let Some(&i) = by_key.get(key) else {
                    return false;
                };
                let item = &derived.items[i];
                if placed_by(new).0 != item.quote.as_str() {
                    return false;
                }
                let reaches = placing.waiting.iter().any(|&w| {
                    entry.decisions.get(w).is_none_or(|d| {
                        let (quote, _) = placed_by(d);
                        d.key.check_id == item.key.check_id
                            || place_key(&d.key.target, &quote, d.key.occurrence)
                                == place_key(
                                    &item.key.target,
                                    item.quote.as_str(),
                                    item.key.occurrence,
                                )
                    })
                });
                if reaches {
                    return false;
                }
            }
        }
    }
    // Read only for a decision shown under another key than its own.
    let texts = OnceCell::new();
    let text_of = |target: &CheckTarget| {
        texts
            .get_or_init(|| match list.unit {
                CheckUnit::Book(book) => read.check_texts_at(book, &chapters),
                CheckUnit::Story(_) => read.check_texts(Collection::Stories),
            })
            .text(target)
    };
    app.inner.counters.add(
        AppCounter::ChecksAssessed,
        (checks.len() + first.len()) as u64,
    );
    let Some(placing) = entry.placing.as_mut() else {
        return false;
    };
    // The decisions the change names, and those of the checks re-assessed, as they are now.
    for new in &now {
        if let Some(&d) = placing.at.get(&new.key)
            && let Some(slot) = entry.decisions.get_mut(d)
        {
            *slot = new.clone();
        }
    }
    for (i, key) in first {
        if let Some(new) = found.get(&key) {
            entry.decisions.push((*new).clone());
            let d = entry.decisions.len() - 1;
            placing.at.insert(key, d);
            placing.shown_on.insert(d, vec![i]);
            entry.shown[i] = Some(d);
            checks.insert(i, derived.items[i].key.clone());
        }
    }
    let view = view_of(read);
    for &i in checks.keys() {
        let Some(decision) = entry.shown[i].and_then(|d| entry.decisions.get(d)) else {
            continue;
        };
        let item = &derived.items[i];
        let state = if decision.key == item.key {
            CheckState::assessed(&decision.record, decision.assessment)
        } else {
            let text = text_of(&item.key.target);
            CheckState::of(
                Some(&decision.record),
                Some(&derived.resolution),
                text.as_deref(),
            )
        };
        let row = row_of(&derived, i, &state);
        entry.states[i] = state;
        if entry.rows[i] != row {
            entry.rows[i] = row;
            entry.stamps[i] = view;
        }
    }
    true
}

/// A tool that is ready, with what its list is derived under.
struct Ready {
    key: ListKey,
    pin: Pin,
    rung: tc_check::Rung,
}

impl App {
    /// The list of `tool` in `unit` as the session's newest snapshot shows it: derived when
    /// its key is new, its rows recomputed when the snapshot moved. `None` when the tool is
    /// not ready.
    async fn with_list<T>(
        &self,
        session: &Arc<ProjectSession>,
        tool: Tool,
        unit: CheckUnit,
        installed: &[fix::Installed],
        answer: impl FnOnce(&ListEntry, &CountingSnapshot) -> T,
    ) -> Result<Option<T>, ApiError> {
        let read = session.read();
        let pins = pins_of(&read)?;
        let pre = preflight(pins.as_ref(), tool, unit, installed);
        let ready = match (&pre.state, &pre.resolution) {
            (PreflightState::Ready, Some(resolution)) => Ready {
                key: ListKey {
                    resolution: resolution.pin.resource.clone(),
                    // A story has no verse numbering and no scope to map or cut by.
                    numbering: match unit {
                        CheckUnit::Book(_) => Numbering::of(versification(&read).as_ref())
                            .project_scheme()
                            .map(|s| s.name()),
                        CheckUnit::Story(_) => None,
                    },
                    scope: match unit {
                        CheckUnit::Book(book) => book_scope(&read, book),
                        CheckUnit::Story(_) => None,
                    },
                },
                pin: resolution.pin.clone(),
                rung: resolution.rung,
            },
            _ => return Ok(None),
        };
        let list = CheckListRef { tool, unit };
        let mut lists = session.checks().lists.lock().await;
        let fresh = lists.get(&list).is_none_or(|e| e.key != ready.key);
        if fresh {
            let key = ready.key.clone();
            let derived = match unit {
                CheckUnit::Book(book) => {
                    let loaded = self.loaded_book_for_checks(session, book).await?;
                    let numbering = loaded.numbering;
                    tokio::task::spawn_blocking(move || {
                        derive(
                            &loaded,
                            tool,
                            book,
                            numbering,
                            key.scope.as_ref(),
                            (&ready.pin, ready.rung),
                        )
                    })
                    .await
                }
                CheckUnit::Story(story) => {
                    let loaded = self.loaded_obs(session).await?;
                    tokio::task::spawn_blocking(move || {
                        derive_story(&loaded, tool, story, (&ready.pin, ready.rung))
                    })
                    .await
                }
            }
            .map_err(|stopped| {
                self.internal(
                    "a check list could not be derived",
                    crate::reports::facts(serde_json::json!({
                        "unit": unit_name(unit),
                        "stopped": crate::jobs::stop_reason(stopped),
                    })),
                )
            })?
            .map_err(|reason| {
                self.internal(
                    "a check list could not be derived",
                    crate::reports::facts(serde_json::json!({
                        "unit": unit_name(unit),
                        "reason": reason,
                    })),
                )
            })?;
            self.inner
                .counters
                .add(AppCounter::ItemsDerived, derived.items.len() as u64);
            let assessed = rows(&read, &derived, tool, unit);
            self.inner
                .counters
                .add(AppCounter::ChecksAssessed, assessed.rows.len() as u64);
            let view = view_of(&read);
            lists.insert(
                list,
                ListEntry {
                    key: ready.key,
                    session: next_session(),
                    derived: Arc::new(derived),
                    seq: read.seq(),
                    stamps: vec![view; assessed.rows.len()],
                    rows: assessed.rows,
                    built: view,
                    states: assessed.states,
                    decisions: assessed.decisions,
                    shown: assessed.shown,
                    placing: assessed.placing,
                },
            );
        }
        let Some(entry) = lists.get_mut(&list) else {
            return Err(self.internal(
                "a check list was lost from its cache",
                crate::reports::facts(serde_json::json!({ "unit": unit_name(unit) })),
            ));
        };
        if entry.seq != read.seq() {
            let changed = session.checks().changes(entry.seq, read.seq()).await;
            let done = changed.is_some_and(|changes| reassess(self, &read, entry, list, &changes));
            if !done {
                let now = rows(&read, &entry.derived, tool, unit);
                self.inner
                    .counters
                    .add(AppCounter::ChecksAssessed, now.rows.len() as u64);
                let view = view_of(&read);
                for ((old, new), stamp) in
                    entry.rows.iter_mut().zip(now.rows).zip(&mut entry.stamps)
                {
                    if *old != new {
                        *old = new;
                        *stamp = view;
                    }
                }
                entry.states = now.states;
                entry.decisions = now.decisions;
                entry.shown = now.shown;
                entry.placing = now.placing;
            }
            entry.seq = read.seq();
        }
        Ok(Some(answer(entry, &read)))
    }
}

/// The rows of `entry` changed after `since`, counted as serialized.
fn changed_since(app: &App, entry: &ListEntry, since: ViewSeq) -> Vec<RailRowPatch> {
    let rows: Vec<RailRowPatch> = entry
        .rows
        .iter()
        .zip(&entry.stamps)
        .filter(|(_, stamp)| **stamp > since)
        .map(|(row, _)| RailRowPatch { row: row.clone() })
        .collect();
    app.inner
        .counters
        .add(AppCounter::RowsSerialized, rows.len() as u64);
    rows
}

fn session_view(
    app: &App,
    list: CheckListRef,
    entry: &ListEntry,
    read: &CountingSnapshot,
    since: Option<RailSince>,
) -> CheckSessionView {
    let patchable = since.filter(|s| s.session == entry.session && s.view >= entry.built);
    let rows = match patchable {
        Some(since) => RailRows::Since(changed_since(app, entry, since.view)),
        None => {
            let rows: Vec<RailRow> = entry
                .derived
                .order
                .iter()
                .filter_map(|&i| entry.rows.get(i as usize).cloned())
                .collect();
            app.inner
                .counters
                .add(AppCounter::RowsSerialized, rows.len() as u64);
            RailRows::Full(rows)
        }
    };
    CheckSessionView {
        session: entry.session,
        list,
        view: view_of(read),
        rows,
        terms: entry.derived.terms.clone(),
        group_titles: entry.derived.group_titles.clone(),
        folded: app.inner.prefs.rail_folded(list.tool),
        progress: entry.progress(),
        citation: Some(entry.derived.citation.clone()),
        dropped: entry.derived.dropped.clone(),
        empty: entry.derived.empty,
        start: count(entry.first_undecided().unwrap_or(0)),
    }
}

// ---------------------------------------------------------------------------------------
// check/picker
// ---------------------------------------------------------------------------------------

impl Handle<Picker> for App {
    async fn handle(&self, req: Picker) -> Result<PickerView, ApiError> {
        let session = self.read(&req.at)?;
        let installed = installed(self)?;
        let mut tools = Vec::new();
        for tool in TOOLS {
            let counted = self
                .with_list(&session, tool, req.unit, &installed, |entry, _| {
                    let progress = entry.progress();
                    CheckProgress {
                        decided: progress.decided,
                        total: progress.total,
                        dropped: entry.derived.dropped.clone(),
                        next: entry.first_undecided().map(|i| NextCheck {
                            title: entry.derived.titles[i].clone(),
                            place: entry.derived.places[i].clone(),
                        }),
                    }
                })
                .await;
            let read = session.read();
            let pins = pins_of(&read)?;
            let pre = preflight(pins.as_ref(), tool, req.unit, &installed);
            let progress = match counted {
                Ok(Some(progress)) => Some(CardCount::Counted(progress)),
                Ok(None) => None,
                // A count that failed is stated on its card and never blocks the tool (D30).
                Err(error) => Some(CardCount::Failed {
                    reason: error.to_string(),
                }),
            };
            tools.push(ToolCard {
                tool,
                state: state(pre.state),
                pin: pre
                    .needs
                    .as_ref()
                    .or(pre.resolution.as_ref().map(|r| &r.pin))
                    .map(citation),
                rung: pre.resolution.as_ref().map(|r| rung(r.rung)),
                fallback_for: pre
                    .unavailable_primary
                    .as_ref()
                    .and_then(|held| match held {
                        tc_check::SlotPin::Usable(pin) => Some(citation(pin)),
                        tc_check::SlotPin::Unusable(_) => None,
                    }),
                progress,
            });
        }
        let read = session.read();
        let old_twl =
            pins_of(&read)?.and_then(|pins| pins.old_twl_pin().map(|pin| pin.repo.to_string()));
        Ok(PickerView {
            tools,
            // A story has no Align tool: the entry is absent, not disabled (tC4 D74 §8).
            align: match req.unit {
                CheckUnit::Book(book) => Some(align_card(&read, book)),
                CheckUnit::Story(_) => None,
            },
            old_twl,
        })
    }
}

/// The Align card's count, from the rail's own rule (`crate::align::index`).
fn align_card(read: &CountingSnapshot, book: BookId) -> CardCount<AlignProgress> {
    CardCount::Counted(crate::align::index::progress(read, book))
}

// ---------------------------------------------------------------------------------------
// check/open, check/rows
// ---------------------------------------------------------------------------------------

impl Handle<Open> for App {
    async fn handle(&self, req: Open) -> Result<CheckSessionView, ApiError> {
        let session = self.read(&req.at)?;
        let installed = installed(self)?;
        let list = req.list;
        let app = self.clone();
        self.with_list(&session, list.tool, list.unit, &installed, |entry, read| {
            session_view(&app, list, entry, read, req.since)
        })
        .await?
        .ok_or(ApiError::ToolNotReady)
    }
}

impl Handle<Rows> for App {
    async fn handle(&self, req: Rows) -> Result<RowsView, ApiError> {
        let session = self.read(&req.at)?;
        let installed = installed(self)?;
        let app = self.clone();
        let since = req.since;
        let answer = self
            .with_list(
                &session,
                req.list.tool,
                req.list.unit,
                &installed,
                |entry, read| {
                    if entry.session != since.session || since.view < entry.built {
                        return RowsView::Reopen;
                    }
                    RowsView::Changed {
                        view: view_of(read),
                        rows: changed_since(&app, entry, since.view),
                        progress: entry.progress(),
                    }
                },
            )
            .await?;
        // A tool that stopped being ready is opened again, which shows why.
        Ok(answer.unwrap_or(RowsView::Reopen))
    }
}
