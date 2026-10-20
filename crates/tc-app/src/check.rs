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
//!   snapshot that moved recomputes the states and derives nothing.
//! - **Titles** are the gateway words that render each quote in the literal source pane, read at
//!   the place the resource gives the check (its own numbering), else the original quote (tC4
//!   `gatewayTitleFor`).
//! - **The item detail and the marks** (unit B5b) are in [`item`]: `check/item`, `check/article`
//!   and `check/decide`.

mod item;

use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_api::ApiError;
use tc_api::check::{
    AlignProgress, CardCount, Category, CheckEmpty, CheckListRef, CheckPlace, CheckProgress,
    CheckSessionView, CheckUnit, Dot, DroppedNote, FixState, FixView, NextCheck, NoteCategory,
    Open, Picker, PickerView, PinCitation, RailProgress, RailRow, RailRowPatch, RailRows,
    RailSince, ResourceCitation, RowFlags, Rows, RowsView, Rung, Tool, ToolCard, ToolState,
    WordCategory,
};
use tc_api::ids::{CheckSessionId, ViewSeq};
use tc_check::derive::{BookSpec, CheckList, notes_for_book, word_links_for_book};
use tc_check::fix::{self, Machine, Preflight, PreflightState};
use tc_check::{CheckItem, CheckState, CheckTarget, Collection, Ladder, Pin, ToolId};
use tc_core::BookId;
use tc_helps::{HelpsPart, Occurrence};
use tc_journal::fold::ProjectedDecision;
use tc_journal::record::{BookPresence, ResourceRef, Scope as BookScope, VerseValue};
use tc_journal::{Record, RecordKey};
use tc_resources::{Missing, PartStatus, ProjectPins};

use crate::app::App;
use crate::counters::{AppCounter, CountingSnapshot};
use crate::dispatch::Handle;
use crate::numbering::Numbering;
use crate::session::ProjectSession;
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
fn place(target: &CheckTarget) -> Result<CheckPlace, String> {
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
    titles: Vec<String>,
    places: Vec<CheckPlace>,
    dropped: Option<DroppedNote>,
    citation: ResourceCitation,
    /// The resource the list derives from, which assesses a decision shown on another key.
    resolution: ResourceRef,
    empty: Option<CheckEmpty>,
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

fn preflight(
    pins: Option<&ProjectPins>,
    tool: Tool,
    book: BookId,
    installed: &[fix::Installed],
) -> Preflight {
    let machine = Machine {
        installed,
        online: true,
    };
    fix::preflight(
        pins.map(|p| p as &dyn Ladder),
        tool_id(tool),
        Collection::Book(book),
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
    let empty_with = |empty, dropped| Derived {
        items: Vec::new(),
        order: Vec::new(),
        terms: Vec::new(),
        term_of: Vec::new(),
        titles: Vec::new(),
        places: Vec::new(),
        dropped,
        citation: citation.clone(),
        resolution: resolution.0.resource.clone(),
        empty: Some(empty),
    };
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
    let items = list.items;
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
    let ids = loaded.pane_ids();
    let literal = crate::understand::literal_pane(&ids)
        .and_then(|id| loaded.pane(&id))
        .and_then(|text| match text {
            LoadedText::Ready(book) => Some(book),
            _ => None,
        });
    let titles = items
        .iter()
        .map(|item| title(loaded, literal, item))
        .collect();
    Ok(Derived {
        items,
        order,
        terms,
        term_of,
        titles,
        places,
        dropped,
        citation,
        resolution: resolution.0.resource.clone(),
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
}

/// Each check's row as the snapshot `read` shows its decisions.
fn rows(read: &CountingSnapshot, derived: &Derived, tool: Tool, book: BookId) -> Assessed {
    if derived.items.is_empty() {
        return Assessed {
            rows: Vec::new(),
            states: Vec::new(),
            decisions: Vec::new(),
            shown: Vec::new(),
        };
    }
    let collection = Collection::Book(book);
    let decisions = read.check_decisions(tool_id(tool), collection);
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
        .map(|(i, state)| RailRow {
            index: count(i),
            check_id: derived.items[i].key.check_id.as_str().to_owned(),
            place: derived.places[i].clone(),
            title: derived.titles[i].clone(),
            flags: flags(state),
            term: derived.term_of[i],
            category: Some(category(derived.items[i].category)),
            marker: None,
        })
        .collect();
    let shown = (0..session.states.len())
        .map(|i| session.shown(i))
        .collect();
    Assessed {
        rows,
        states: session.states,
        decisions,
        shown,
    }
}

/// A tool that is ready, with what its list is derived under.
struct Ready {
    key: ListKey,
    pin: Pin,
    rung: tc_check::Rung,
}

impl App {
    /// The list of `tool` in `book` as the session's newest snapshot shows it: derived when
    /// its key is new, its rows recomputed when the snapshot moved. `None` when the tool is
    /// not ready.
    async fn with_list<T>(
        &self,
        session: &Arc<ProjectSession>,
        tool: Tool,
        book: BookId,
        installed: &[fix::Installed],
        answer: impl FnOnce(&ListEntry, &CountingSnapshot) -> T,
    ) -> Result<Result<T, Preflight>, ApiError> {
        let read = session.read();
        let pins = pins_of(&read)?;
        let pre = preflight(pins.as_ref(), tool, book, installed);
        let ready = match (&pre.state, &pre.resolution) {
            (PreflightState::Ready, Some(resolution)) => Ready {
                key: ListKey {
                    resolution: resolution.pin.resource.clone(),
                    numbering: Numbering::of(versification(&read).as_ref())
                        .project_scheme()
                        .map(|s| s.name()),
                    scope: book_scope(&read, book),
                },
                pin: resolution.pin.clone(),
                rung: resolution.rung,
            },
            _ => return Ok(Err(pre)),
        };
        let list = CheckListRef {
            tool,
            unit: CheckUnit::Book(book),
        };
        let mut lists = session.checks().lists.lock().await;
        let fresh = lists.get(&list).is_none_or(|e| e.key != ready.key);
        if fresh {
            let loaded = self.loaded_book_for_checks(session, book).await?;
            let numbering = loaded.numbering;
            let key = ready.key.clone();
            let derived = tokio::task::spawn_blocking(move || {
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
            .map_err(|stopped| {
                self.internal(
                    "a check list could not be derived",
                    crate::reports::facts(serde_json::json!({
                        "book": book.code(),
                        "stopped": crate::jobs::stop_reason(stopped),
                    })),
                )
            })?
            .map_err(|reason| {
                self.internal(
                    "a check list could not be derived",
                    crate::reports::facts(serde_json::json!({
                        "book": book.code(),
                        "reason": reason,
                    })),
                )
            })?;
            self.inner
                .counters
                .add(AppCounter::ItemsDerived, derived.items.len() as u64);
            let assessed = rows(&read, &derived, tool, book);
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
                },
            );
        }
        let Some(entry) = lists.get_mut(&list) else {
            return Err(self.internal(
                "a check list was lost from its cache",
                crate::reports::facts(serde_json::json!({ "book": book.code() })),
            ));
        };
        if entry.seq != read.seq() {
            let now = rows(&read, &entry.derived, tool, book);
            let view = view_of(&read);
            for ((old, new), stamp) in entry.rows.iter_mut().zip(now.rows).zip(&mut entry.stamps) {
                if *old != new {
                    *old = new;
                    *stamp = view;
                }
            }
            entry.states = now.states;
            entry.decisions = now.decisions;
            entry.shown = now.shown;
            entry.seq = read.seq();
        }
        Ok(Ok(answer(entry, &read)))
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
        progress: entry.progress(),
        citation: Some(entry.derived.citation.clone()),
        dropped: entry.derived.dropped.clone(),
        empty: entry.derived.empty,
        start: count(entry.first_undecided().unwrap_or(0)),
    }
}

fn book_of(list: &CheckListRef) -> Result<BookId, ApiError> {
    match list.unit {
        CheckUnit::Book(book) => Ok(book),
        // The stories' checks come with the OBS screens (B7).
        CheckUnit::Stories => Err(ApiError::BadRequest {
            field: "list".to_owned(),
            reason: "the stories' check lists are not served yet".to_owned(),
        }),
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
                .with_list(&session, tool, req.book, &installed, |entry, _| {
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
            let pre = preflight(pins.as_ref(), tool, req.book, &installed);
            let progress = match counted {
                Ok(Ok(progress)) => Some(CardCount::Counted(progress)),
                Ok(Err(_)) => None,
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
            align: align_card(&read, req.book),
            old_twl,
        })
    }
}

/// The Align card's count (tC4 `alignPickerEntry` over `alignIndexItems`): every drafted
/// verse counts; one is done when its alignment is marked done and still vouches for the verse
/// (`tc_align::verse_status`); the next is the first drafted verse not done.
fn align_card(read: &CountingSnapshot, book: BookId) -> CardCount<AlignProgress> {
    let mut done = 0;
    let mut total = 0;
    let mut next = None;
    for chapter in read.chapters_of(book) {
        let keys = read.chapter_keys(book, chapter);
        for key in &keys.verses {
            let Some(Record::Verse {
                verse,
                value: VerseValue::Text { words, last, .. },
                ..
            }) = read.projected(key)
            else {
                continue;
            };
            // tC4's verse text of a stub (`___`) is empty: not drafted (#572's rule).
            if !crate::drafted::words_drafted(&words) {
                continue;
            }
            total += 1;
            let target = tc_align::words::TargetVerse::new(&words);
            let run = tc_journal::alignment::AlignedRun {
                anchor: verse.get(),
                run_start: verse.get(),
                run_end: last.get().max(verse.get()),
                text_md5: target.md5(),
                source_sha: None,
            };
            let alignment = keys
                .alignments
                .iter()
                .find_map(|k| match read.projected(k) {
                    Some(Record::Alignment {
                        verse: anchor,
                        value,
                        ..
                    }) if anchor == verse => Some(value),
                    _ => None,
                });
            let status = tc_align::verse_status(alignment.as_ref(), Some((&run, &target)), None);
            if status.state == tc_align::VerseState::Valid {
                done += 1;
            } else if next.is_none() {
                let label = if last.get() > verse.get() {
                    format!("{chapter}:{}-{}", verse.get(), last.get())
                } else {
                    format!("{chapter}:{}", verse.get())
                };
                next = Some(label);
            }
        }
    }
    CardCount::Counted(AlignProgress { done, total, next })
}

// ---------------------------------------------------------------------------------------
// check/open, check/rows
// ---------------------------------------------------------------------------------------

impl Handle<Open> for App {
    async fn handle(&self, req: Open) -> Result<CheckSessionView, ApiError> {
        let session = self.read(&req.at)?;
        let book = book_of(&req.list)?;
        let installed = installed(self)?;
        let list = req.list;
        let app = self.clone();
        self.with_list(&session, list.tool, book, &installed, |entry, read| {
            session_view(&app, list, entry, read, req.since)
        })
        .await?
        .map_err(|_| ApiError::ToolNotReady)
    }
}

impl Handle<Rows> for App {
    async fn handle(&self, req: Rows) -> Result<RowsView, ApiError> {
        let session = self.read(&req.at)?;
        let book = book_of(&req.list)?;
        let installed = installed(self)?;
        let app = self.clone();
        let since = req.since;
        let answer = self
            .with_list(&session, req.list.tool, book, &installed, |entry, read| {
                if entry.session != since.session || since.view < entry.built {
                    return RowsView::Reopen;
                }
                RowsView::Changed {
                    view: view_of(read),
                    rows: changed_since(&app, entry, since.view),
                    progress: entry.progress(),
                }
            })
            .await?;
        // A tool that stopped being ready is opened again, which shows why.
        Ok(answer.unwrap_or(RowsView::Reopen))
    }
}

// ---------------------------------------------------------------------------------------
// resources/fixState
// ---------------------------------------------------------------------------------------

impl Handle<FixState> for App {
    async fn handle(&self, req: FixState) -> Result<Option<FixView>, ApiError> {
        let session = self.read(&req.at)?;
        let book = book_of(&req.list)?;
        let installed = installed(self)?;
        let read = session.read();
        let pins = pins_of(&read)?;
        let Some(pins) = pins else {
            return Ok(None);
        };
        let pre = preflight(Some(&pins), req.list.tool, book, &installed);
        // Only a card whose pinned resource is missing offers the fix (tC4 `ToolNeeds`).
        if !matches!(
            pre.state,
            PreflightState::Fetch | PreflightState::Unavailable
        ) {
            return Ok(None);
        }
        let machine = Machine {
            installed: &installed,
            online: true,
        };
        Ok(fix::guided_fix(&pins, &pre, &machine).map(|offer| FixView {
            tool: req.list.tool,
            pin: citation(&offer.pin),
            rung: rung(offer.rung),
            candidates: offer
                .candidates
                .iter()
                .map(|c| PinCitation {
                    repo_path: c.resource.repo_path.to_string(),
                    version: c.version.clone(),
                    sha: c.resource.sha.to_hex(),
                })
                .collect(),
        }))
    }
}
