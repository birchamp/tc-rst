//! Open and Understand (unit B3): the open job, the close at Home, the open book with its rail
//! and comments, the source panes of a chapter, and the helps.
//!
//! A book's resources are read once per open book (`helps_load_one_book`): the session keeps
//! the current and the previous book's ([`BookCache`]), each with its source panes cut into
//! chapters ([`crate::sources`]), and every chapter's panes and helps are answered from them.
//! Nothing here writes the project.

use std::sync::Arc;

use tc_api::editor::{
    Book, BookView, CommentView, FrameRow, PaneChapter, PaneText, RailBook, RowSource, SourceFrame,
    SourcePane, Sources, SourcesView,
};
use tc_api::helps::{
    Article, ArticleLink, ArticleView, LitVerse, NoteCard, PaneFocus, Passage, PassageView,
    QuestionCard, ReadySlot, SimplifiedVerse, Slot, WordCard,
};
use tc_api::ids::{JobId, RefPart, WallMs};
use tc_api::jobs::{JobKind, JobOutcome, OpenPhase, Phase};
use tc_api::model::{ArticleBlock, ArticleSpan, BlockKind};
use tc_api::prefs::DraftUnit;
use tc_api::session::{Close, KeptPlace, Open, OpenUnit, OpenedProject};
use tc_api::shown::{Direction, Known};
use tc_api::{ApiError, NotSaved};
use tc_core::BookId;
use tc_helps::{
    AcademyArticleId, HelpItemId, HelpRef, HelpsPart, Manual, Occurrence, Quote, Slug,
    WordArticleId, WordCategory,
};
use tc_journal::record::{CommentTarget, VerseValue, VersificationValue};
use tc_journal::{ChangeId, PinGroup, Record, RecordKey};
use tc_resources::{
    BookResources, HelpsChoice, Missing, PartStatus, ProjectPins, Rung, TextRole, TextStatus,
};

use crate::app::App;
use crate::counters::{AppCounter, CountingSnapshot};
use crate::dispatch::Handle;
use crate::jobs::{JobCtx, JobEnd, JobSpec};
use crate::numbering::{self, Numbering};
use crate::session::ProjectSession;
use crate::sources::{ChapterModel, PaneBook, VerseModel, pane_book, verse_range};

/// How many books a session keeps loaded: the current and the previous (§4.4).
const BOOKS_KEPT: usize = 2;

/// One source text of a loaded book.
pub(crate) enum LoadedText {
    Ready(PaneBook),
    NotInstalled,
    Missing,
    Error(String),
}

struct LoadedPane {
    id: String,
    /// The pinned version (`v91`), when the pin names one.
    version: Option<String>,
    text: LoadedText,
}

/// What a session read for one book: its helps and its source texts, cut into chapters.
pub(crate) struct LoadedBook {
    book: BookId,
    /// The versions of the pins and the helps setting it was read under: a change of either
    /// reads again.
    stamp: Vec<Vec<ChangeId>>,
    pins: Option<ProjectPins>,
    pub(crate) resources: BookResources,
    panes: Vec<LoadedPane>,
    /// The original-language text of the book's testament (Translate's chip); none when the
    /// project pins none.
    original: Option<LoadedPane>,
    simplified: Slot<PaneBook>,
    simplified_pin: Option<String>,
    /// How the project numbers the book's verses against its sources.
    pub(crate) numbering: Numbering,
    /// Read with the Door43 helps, whatever the project's helps setting: the check lists'
    /// reading (Q33).
    door43: bool,
}

impl LoadedBook {
    /// The source panes' ids, in order.
    pub(crate) fn pane_ids(&self) -> Vec<String> {
        self.panes.iter().map(|p| p.id.clone()).collect()
    }

    pub(crate) fn pane(&self, id: &str) -> Option<&LoadedText> {
        self.panes.iter().find(|p| p.id == id).map(|p| &p.text)
    }

    /// The book's original-language text (the UGNT or the UHB of its testament).
    pub(crate) fn original(&self) -> &LoadedText {
        self.original
            .as_ref()
            .map_or(&LoadedText::Missing, |pane| &pane.text)
    }

    pub(crate) fn pins(&self) -> Option<&ProjectPins> {
        self.pins.as_ref()
    }
}

/// The pane whose words title a help or a check: the literal Bible (`ult`, else `glt`), else
/// the first pane (tC4's `literalPaneId`).
pub(crate) fn literal_pane(ids: &[String]) -> Option<String> {
    ["ult", "glt"]
        .into_iter()
        .find(|id| ids.iter().any(|i| i == id))
        .map(str::to_owned)
        .or_else(|| ids.first().cloned())
}

/// The books a session has read, newest last. The lock is held across a load, so two reads of
/// one book at once load it once.
#[derive(Default)]
pub(crate) struct BookCache {
    books: tokio::sync::Mutex<Vec<Arc<LoadedBook>>>,
    /// A story project's resources (B7).
    obs: crate::story::ObsCache,
}

impl BookCache {
    /// Every book is read again at its next need: a download may have installed what one
    /// lacked.
    pub(crate) async fn forget(&self) {
        self.books.lock().await.clear();
        self.obs.forget().await;
    }

    pub(crate) fn obs(&self) -> &crate::story::ObsCache {
        &self.obs
    }
}

fn not_found(what: &str) -> ApiError {
    ApiError::BadRequest {
        field: "book".to_owned(),
        reason: format!("{what} is not in the project"),
    }
}

/// The project's pins, from its `Pins` records; none when no group has a record (tC4's "no
/// pins recorded", which shows every help as not included).
pub(crate) fn project_pins(read: &CountingSnapshot) -> Result<Option<ProjectPins>, String> {
    let mut pins = ProjectPins::default();
    let mut any = false;
    for group in PinGroup::ALL {
        if let Some(Record::Pins { value, .. }) = read.projected(&RecordKey::Pins(group)) {
            pins.set_group_value(group, &value)
                .map_err(|e| format!("the {group:?} pins: {e}"))?;
            any = true;
        }
    }
    Ok(any.then_some(pins))
}

fn helps_setting() -> Option<RecordKey> {
    tc_journal::keys::SettingKey::parse(tc_resources::HELPS_SOURCE_SETTING)
        .ok()
        .map(RecordKey::Setting)
}

/// The versions of what a book's resources are read under: each pin group's, the helps
/// setting's and the versification's heads. They are in memory, so a book read again costs no
/// value read.
pub(crate) fn resources_stamp(read: &CountingSnapshot) -> Vec<Vec<ChangeId>> {
    PinGroup::ALL
        .into_iter()
        .map(RecordKey::Pins)
        .chain(helps_setting())
        .chain([RecordKey::Versification])
        .map(|key| read.heads(&key))
        .collect()
}

/// The project's versification register, when it has one.
pub(crate) fn versification(read: &CountingSnapshot) -> Option<VersificationValue> {
    match read.projected(&RecordKey::Versification) {
        Some(Record::Versification { value }) => Some(value),
        _ => None,
    }
}

/// A chapter's project verses in order, each a verse or the first and last of a span (tC4's
/// `indexBook` entries): a verse inside a span, or not in the translation, is none.
fn project_verses(read: &CountingSnapshot, book: BookId, chapter: u16) -> Vec<(u16, u16)> {
    let mut verses: Vec<(u16, u16)> = read
        .chapter_keys(book, chapter)
        .verses
        .iter()
        .filter_map(|key| match read.projected(key) {
            Some(Record::Verse {
                verse,
                value: VerseValue::Text { last, .. },
                ..
            }) => Some((verse.get(), last.get().max(verse.get()))),
            _ => None,
        })
        .collect();
    verses.sort_unstable();
    verses
}

/// The source verse a row of another numbering shows in a pane: the one that carries its first
/// verse.
fn row_verse<'a>(pane: &'a PaneBook, row: &FrameRow) -> Option<&'a VerseModel> {
    let RowSource::Verse { chapter, verse } = &row.source else {
        return None;
    };
    let (from, _) = verse_range(verse.as_str())?;
    pane.chapters
        .get(chapter)?
        .verses
        .iter()
        .find(|m| m.from <= from && from <= m.to)
}

/// A pane's chapter in another numbering: each row's source verse under the project verse's
/// key, with no section.
fn mapped_chapter(pane: &PaneBook, rows: &[FrameRow]) -> PaneChapter {
    PaneChapter {
        sections: Vec::new(),
        verses: rows
            .iter()
            .filter_map(|row| {
                let mut view = row_verse(pane, row)?.view.clone();
                view.key = row.verse.clone();
                Some(view)
            })
            .collect(),
    }
}

/// Where a help stands: the source chapter and verse its quote is found at, and the chapter and
/// verse it is shown at.
struct Placed {
    source_chapter: u16,
    source_verse: RefPart,
    chapter: RefPart,
    verse: RefPart,
}

/// The helps source the project's setting names; Door43 without one.
fn helps_choice(read: &CountingSnapshot) -> HelpsChoice {
    let Some(key) = helps_setting() else {
        return HelpsChoice::Dcs;
    };
    match read.projected(&key) {
        Some(Record::Setting {
            value: Some(json), ..
        }) => HelpsChoice::from_setting(&serde_json::Value::from(&json)).unwrap_or_default(),
        _ => HelpsChoice::Dcs,
    }
}

fn text_state(status: &TextStatus) -> LoadedText {
    match status {
        TextStatus::Loaded { usfm, .. } => LoadedText::Ready(pane_book(usfm)),
        TextStatus::Missing(missing) => match missing {
            Missing::NotInstalled(_) | Missing::AquiferNotInstalled(_) => LoadedText::NotInstalled,
            Missing::NotPinned
            | Missing::NotCovered { .. }
            | Missing::BookNotInResource { .. }
            | Missing::NotInAquifer => LoadedText::Missing,
            Missing::Unreadable { problem, .. } => LoadedText::Error(problem.clone()),
            Missing::OldTwl(_) | Missing::UnusablePin { .. } => {
                LoadedText::Error(format!("{missing:?}"))
            }
        },
    }
}

/// A slot that is not ready, in tC4's states (`SlotState`, `tC4:src/views/HelpsPanel.jsx:109`).
pub(crate) fn missing_slot<T>(missing: &Missing) -> Slot<T> {
    match missing {
        Missing::NotPinned | Missing::NotCovered { .. } | Missing::NotInAquifer => Slot::None,
        Missing::NotInstalled(_) | Missing::AquiferNotInstalled(_) => Slot::Fetch,
        Missing::BookNotInResource { .. } => Slot::Missing,
        Missing::OldTwl(_) => Slot::OldTwl,
        Missing::Unreadable { source, problem } => Slot::Error {
            reason: format!("{source}: {problem}"),
        },
        Missing::UnusablePin { rung, pin } => Slot::Error {
            reason: format!("the {rung:?} pin cannot be read: {pin:?}"),
        },
    }
}

/// The direction of the helps a rung answered with (#650): its gateway's, when the app knows
/// the gateway.
pub(crate) fn rung_dir(pins: Option<&ProjectPins>, rung: Option<Rung>) -> Option<Direction> {
    let set = pins?.set(rung?)?;
    let gateway = tc_resources::gateways::of_set(set)?;
    Some(match gateway.direction {
        tc_resources::gateways::Direction::Ltr => Direction::Ltr,
        tc_resources::gateways::Direction::Rtl => Direction::Rtl,
    })
}

/// A part's slot, with `items` when it loaded.
pub(crate) fn slot<T>(
    status: Option<&PartStatus>,
    pins: Option<&ProjectPins>,
    items: impl FnOnce() -> Vec<T>,
) -> Slot<T> {
    match status {
        Some(PartStatus::Loaded {
            rung,
            substitute_for,
            ..
        }) => Slot::Ready(ReadySlot {
            items: items(),
            dir: rung_dir(pins, *rung),
            fallback: substitute_for.is_some(),
        }),
        Some(PartStatus::Missing(missing)) => missing_slot(missing),
        None => Slot::None,
    }
}

fn load(
    book: BookId,
    stamp: Vec<Vec<ChangeId>>,
    pins: Option<ProjectPins>,
    choice: &HelpsChoice,
    register: Option<&VersificationValue>,
    app: &App,
) -> LoadedBook {
    let store = &app.inner.resources;
    let read_pins = pins.clone().unwrap_or_default();
    let resources = tc_resources::load_book(store, &read_pins, choice, book);
    app.inner.counters.add(AppCounter::BookResourceLoads, 1);
    let panes = read_pins
        .extra_scripture
        .iter()
        .map(|pane| LoadedPane {
            id: pane.id.as_str().to_owned(),
            version: pane.pin.version.as_ref().map(|v| v.as_str().to_owned()),
            text: resources
                .text(&TextRole::Pane(pane.id.clone()))
                .map_or(LoadedText::Missing, text_state),
        })
        .collect();
    let original = resources
        .text(&TextRole::OriginalLanguage)
        .map(|status| LoadedPane {
            id: "orig".to_owned(),
            version: match status {
                TextStatus::Loaded { pin, .. } => {
                    pin.version.as_ref().map(|v| v.as_str().to_owned())
                }
                TextStatus::Missing(_) => None,
            },
            text: text_state(status),
        });
    let simplified_pin = read_pins
        .set(Rung::Primary)
        .and_then(|set| set.pin(tc_resources::SetSlot::SimplifiedText))
        .or_else(|| {
            read_pins
                .set(Rung::Fallback)
                .and_then(|set| set.pin(tc_resources::SetSlot::SimplifiedText))
        })
        .map(|pin| pin.repo.name().as_str().to_owned());
    let simplified = match resources.text(&TextRole::Simplified) {
        Some(TextStatus::Loaded { usfm, rung, .. }) => Slot::Ready(ReadySlot {
            items: vec![pane_book(usfm)],
            dir: rung_dir(pins.as_ref(), *rung),
            fallback: false,
        }),
        Some(TextStatus::Missing(missing)) => missing_slot(missing),
        None => Slot::None,
    };
    LoadedBook {
        book,
        stamp,
        pins,
        resources,
        panes,
        original,
        simplified,
        simplified_pin,
        numbering: Numbering::of(register),
        door43: matches!(choice, HelpsChoice::Dcs),
    }
}

impl App {
    /// The open session's resources of `book`, read at its first need and kept with the
    /// previous book's.
    pub(crate) async fn loaded_book(
        &self,
        session: &Arc<ProjectSession>,
        book: BookId,
    ) -> Result<Arc<LoadedBook>, ApiError> {
        let choice = helps_choice(&session.read());
        self.loaded_book_with(session, book, choice).await
    }

    /// The book's resources read with the Door43 helps, as the check lists derive from them
    /// (Q33): the screen's own when its helps setting is Door43.
    pub(crate) async fn loaded_book_for_checks(
        &self,
        session: &Arc<ProjectSession>,
        book: BookId,
    ) -> Result<Arc<LoadedBook>, ApiError> {
        self.loaded_book_with(session, book, HelpsChoice::Dcs).await
    }

    async fn loaded_book_with(
        &self,
        session: &Arc<ProjectSession>,
        book: BookId,
        choice: HelpsChoice,
    ) -> Result<Arc<LoadedBook>, ApiError> {
        let read = session.read();
        let stamp = resources_stamp(&read);
        let door43 = matches!(choice, HelpsChoice::Dcs);
        let mut books = session.books().books.lock().await;
        if let Some(found) = books
            .iter()
            .find(|b| b.book == book && b.stamp == stamp && b.door43 == door43)
        {
            return Ok(found.clone());
        }
        let pins = project_pins(&read).map_err(|reason| ApiError::BadRequest {
            field: "pins".to_owned(),
            reason,
        })?;
        let register = versification(&read);
        let app = self.clone();
        let loaded = tokio::task::spawn_blocking(move || {
            load(book, stamp, pins, &choice, register.as_ref(), &app)
        })
        .await
        .map_err(|stopped| {
            self.internal(
                "a book's resources could not be read",
                crate::reports::facts(serde_json::json!({
                    "book": book.code(),
                    "stopped": crate::jobs::stop_reason(stopped),
                })),
            )
        })?;
        let loaded = Arc::new(loaded);
        books.retain(|b| b.book != book || b.door43 != door43);
        books.push(loaded.clone());
        let over = books.len().saturating_sub(BOOKS_KEPT);
        books.drain(..over);
        Ok(loaded)
    }
}

// ---------------------------------------------------------------------------------------
// session/open, session/close
// ---------------------------------------------------------------------------------------

impl Handle<Open> for App {
    async fn handle(&self, req: Open) -> Result<JobId, ApiError> {
        // Opening any project stops the Home summary that runs (§4.5).
        self.cancel_home_summary();
        let app = self.clone();
        let spec = JobSpec::new(JobKind::Open, Phase::Open(OpenPhase::FinishingWork))
            .project(req.project.clone());
        Ok(self
            .inner
            .jobs
            .start_async(spec, move |ctx| async move { app.open_job(req, ctx).await }))
    }
}

impl App {
    async fn open_job(&self, req: Open, ctx: JobCtx) -> Result<JobOutcome, JobEnd> {
        #[cfg(feature = "test-hooks")]
        self.hold_at(tc_api::test::HoldPoint::Open).await;
        let session = self
            .inner
            .sessions
            .open_screen(&req.project, ctx.progress.clone())
            .await
            .map_err(|failure| JobEnd::Failed(crate::wire::error::open_failure(failure)))?;
        let now = self.inner.clock.now();
        self.inner.prefs.record_last_used(req.project.clone(), now);
        let store = self.inner.sessions.store().clone();
        let folder = req.project.clone();
        let peek = tokio::task::spawn_blocking(move || store.peek(&folder))
            .await
            .ok()
            .and_then(Result::ok);
        let kind = self
            .read(&session.at)
            .map(|s| crate::home::api_kind(s.handle().facts().kind))
            .map_err(JobEnd::Failed)?;
        let (metadata, text_direction) = match peek {
            Some(p) => (p.metadata, p.text_direction),
            None => (
                Err("the project's metadata could not be read".to_owned()),
                None,
            ),
        };
        let facts =
            crate::home::card_facts(&metadata, text_direction.as_deref(), &req.project, kind);
        let place = req.unit.and_then(|unit| {
            let key = match unit {
                OpenUnit::Book(book) => book.code().to_owned(),
                OpenUnit::Story(story) => format!("story:{}", story.get()),
            };
            self.inner
                .prefs
                .place(&session.id, &key)
                .map(|place| KeptPlace {
                    mode: place.mode,
                    chapter: place.chapter,
                    tool: place.tool,
                })
        });
        Ok(JobOutcome::Opened(Box::new(OpenedProject {
            draft_unit: self
                .inner
                .prefs
                .draft_unit(&session.id)
                .unwrap_or(DraftUnit::Section),
            name: facts.name,
            tag: facts.tag.unwrap_or_default(),
            dir: facts.dir,
            kind,
            books: facts.books.unwrap_or_default(),
            place,
            session,
        })))
    }
}

impl Handle<Close> for App {
    async fn handle(&self, req: Close) -> Result<(), ApiError> {
        self.inner
            .sessions
            .leave_screen(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))
    }
}

// ---------------------------------------------------------------------------------------
// editor/book, editor/sources
// ---------------------------------------------------------------------------------------

impl Handle<Book> for App {
    async fn handle(&self, req: Book) -> Result<BookView, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let chapters = read.chapters_of(req.book);
        if chapters.is_empty() {
            return Err(not_found(req.book.code()));
        }
        let mut draft = crate::drafted::BookDraft {
            drafted: 0,
            total: 0,
        };
        for &chapter in &chapters {
            for key in read.chapter_keys(req.book, chapter).verses {
                if let Some(Record::Verse { verse, value, .. }) = read.projected(&key) {
                    crate::drafted::count_verse(&mut draft, verse.get(), &value);
                }
            }
        }
        let mut comments = Vec::new();
        for key in read.comments_of(req.book) {
            let Some(Record::Comment { value, .. }) = read.projected(&key) else {
                continue;
            };
            let CommentTarget::Verse { chapter, verse, .. } = value.target else {
                continue;
            };
            comments.push(CommentView {
                token: self.shown_token(&read, &key)?,
                chapter: chapter.get(),
                verse: verse.get(),
                text: value.text,
                at: WallMs(read.written_at(&key).map_or(0, |at| at.0)),
            });
        }
        comments.sort_by_key(|c| c.at);
        let summary = self.inner.home.last_known(&session.scope().project);
        let rail = read
            .projected_books()
            .into_iter()
            .map(|book| RailBook {
                book,
                drafted: if book == req.book {
                    Known::Known(draft.percent())
                } else {
                    summary
                        .as_ref()
                        .and_then(|s| s.books.iter().find(|(b, _)| *b == book))
                        .and_then(|(_, d)| d.map(|d| Known::Known(d.percent())))
                        .unwrap_or(Known::Unknown)
                },
            })
            .collect();
        Ok(BookView {
            view: crate::wire::scope::view(read.seq()),
            book: req.book,
            chapters,
            rail,
            comments,
        })
    }
}

impl Handle<Sources> for App {
    async fn handle(&self, req: Sources) -> Result<SourcesView, ApiError> {
        let session = self.read(&req.at)?;
        let loaded = self.loaded_book(&session, req.book).await?;
        let frame = loaded.frame(&session.read(), req.chapter);
        let pane_view = |pane: &LoadedPane, dir: Direction| SourcePane {
            id: pane.id.clone(),
            version: pane.version.clone(),
            dir,
            text: match &pane.text {
                LoadedText::Ready(book) => PaneText::Ready(match &frame {
                    SourceFrame::Same => book
                        .chapters
                        .get(&req.chapter)
                        .map(ChapterModel::view)
                        .unwrap_or_default(),
                    SourceFrame::Mapped { rows } => mapped_chapter(book, rows),
                    SourceFrame::Unknown => PaneChapter::default(),
                }),
                LoadedText::NotInstalled => PaneText::NotInstalled,
                LoadedText::Missing => PaneText::Missing,
                LoadedText::Error(reason) => PaneText::Error {
                    reason: reason.clone(),
                },
            },
        };
        let panes = loaded
            .panes
            .iter()
            .map(|pane| pane_view(pane, Direction::Ltr))
            .collect();
        // The Hebrew Bible reads right to left (tC4's `SourceCell`).
        let original_dir = match req.book.testament() {
            tc_core::Testament::Old => Direction::Rtl,
            tc_core::Testament::New => Direction::Ltr,
        };
        let original = loaded
            .original
            .as_ref()
            .map(|pane| pane_view(pane, original_dir));
        Ok(SourcesView {
            panes,
            original,
            frame,
        })
    }
}

// ---------------------------------------------------------------------------------------
// helps/passage, helps/article
// ---------------------------------------------------------------------------------------

/// A help's reference as tC4's `contextId.reference` holds it: the chapter and verse parts
/// as written (`1:1`, `1:intro`, `front:intro`, `1:1-3`).
pub(crate) fn parts(reference: &HelpRef) -> Option<(RefPart, RefPart)> {
    let text = reference.tsv_text();
    let (chapter, verse) = text.split_once(':')?;
    Some((RefPart::parse(chapter).ok()?, RefPart::parse(verse).ok()?))
}

fn in_chapter(chapter: &RefPart, wanted: u16) -> bool {
    chapter.as_str().parse::<u16>().ok() == Some(wanted)
}

pub(crate) fn api_spans(spans: Vec<tc_helps::blocks::Span>) -> Vec<ArticleSpan> {
    spans
        .into_iter()
        .map(|s| ArticleSpan {
            text: s.text,
            bold: s.bold,
            italic: s.italic,
        })
        .collect()
}

pub(crate) fn blocks(markdown: &str) -> Vec<ArticleBlock> {
    use tc_helps::blocks::BlockKind as Kind;
    tc_helps::blocks::render_article_blocks(markdown)
        .into_iter()
        .map(|b| ArticleBlock {
            kind: match b.kind {
                Kind::Heading => BlockKind::H,
                Kind::Item => BlockKind::Li,
                Kind::Numbered => BlockKind::Ol,
                Kind::Paragraph => BlockKind::P,
                Kind::Row => BlockKind::Tr,
            },
            quote: b.quote,
            spans: api_spans(b.spans),
            depth: b.depth,
            number: b.number,
            cells: b.cells.into_iter().map(api_spans).collect(),
            header: b.header,
        })
        .collect()
}

/// tC4's number for an occurrence: `-1` for every one, `0` for none, and 1 where the
/// resource records none.
pub(crate) fn occurrence(o: Occurrence) -> i32 {
    match o {
        Occurrence::NotTied => 0,
        Occurrence::Nth(n) => i32::try_from(n.get()).unwrap_or(i32::MAX),
        Occurrence::All => -1,
        Occurrence::Unrecorded => 1,
    }
}

pub(crate) fn item_id(id: &HelpItemId) -> String {
    match id {
        HelpItemId::Row(row) => row.as_str().to_owned(),
        HelpItemId::Aquifer(content) => content.as_str().to_owned(),
    }
}

/// A words article's name: its slug, or an Aquifer article's content id.
pub(crate) fn word_slug(id: &WordArticleId) -> String {
    match id {
        WordArticleId::Dcs { slug, .. } => slug.as_str().to_owned(),
        WordArticleId::Aquifer(content) => content.as_str().to_owned(),
    }
}

/// The manuals tC4 probes for a note's article, in its order (`TA_SECTIONS`,
/// `tC4:src/data/articles.ts:18`).
pub(crate) const MANUALS: [Manual; 4] = [
    Manual::Translate,
    Manual::Checking,
    Manual::Process,
    Manual::Intro,
];

impl LoadedBook {
    /// How the project numbers `chapter` against the sources.
    fn frame(&self, read: &CountingSnapshot, chapter: u16) -> SourceFrame {
        match self.numbering {
            Numbering::Same => SourceFrame::Same,
            Numbering::Unknown => SourceFrame::Unknown,
            Numbering::Other(scheme) => SourceFrame::Mapped {
                rows: numbering::rows(
                    scheme,
                    self.book,
                    chapter,
                    &project_verses(read, self.book, chapter),
                ),
            },
        }
    }

    /// Where a help of the sources stands in the project's `chapter`, if it is there.
    fn place(&self, frame: &SourceFrame, chapter: u16, reference: &HelpRef) -> Option<Placed> {
        let (c, v) = parts(reference)?;
        match (frame, self.numbering) {
            (SourceFrame::Same, _) => in_chapter(&c, chapter).then(|| Placed {
                source_chapter: chapter,
                source_verse: v.clone(),
                chapter: c,
                verse: v,
            }),
            (SourceFrame::Mapped { .. }, Numbering::Other(scheme)) => {
                let (at, verse) = numbering::place(scheme, self.book, &c, &v)?;
                (at == chapter).then_some(Placed {
                    source_chapter: c.as_str().parse().ok()?,
                    source_verse: v,
                    chapter: RefPart::parse(&chapter.to_string()).ok()?,
                    verse,
                })
            }
            _ => None,
        }
    }

    /// Each pane's rendering of a placed help's quote, its tokens under the verse keys the
    /// pane shows them at.
    fn placed_focus(
        &self,
        frame: &SourceFrame,
        placed: &Placed,
        quote: &Quote,
        occurrence: Occurrence,
    ) -> Vec<PaneFocus> {
        let mut focus = self.focus(
            placed.source_chapter,
            &placed.source_verse,
            quote,
            occurrence,
        );
        let SourceFrame::Mapped { rows } = frame else {
            return focus;
        };
        for pane in &mut focus {
            let Some(LoadedPane {
                text: LoadedText::Ready(book),
                ..
            }) = self.panes.iter().find(|p| p.id == pane.pane)
            else {
                continue;
            };
            pane.lit = std::mem::take(&mut pane.lit)
                .into_iter()
                .filter_map(|lit| {
                    let row = rows.iter().find(|row| {
                        matches!(row.source, RowSource::Verse { chapter, .. } if chapter == placed.source_chapter)
                            && row_verse(book, row).is_some_and(|m| m.key == lit.verse)
                    })?;
                    Some(LitVerse {
                        verse: row.verse.clone(),
                        tokens: lit.tokens,
                    })
                })
                .collect();
        }
        focus
    }

    /// Each ready pane's rendering of a quote and the tokens that render it.
    fn focus(
        &self,
        chapter: u16,
        verse: &RefPart,
        quote: &Quote,
        occurrence: Occurrence,
    ) -> Vec<PaneFocus> {
        self.panes
            .iter()
            .filter_map(|pane| {
                let LoadedText::Ready(book) = &pane.text else {
                    return None;
                };
                let (title, lit) = book
                    .chapters
                    .get(&chapter)
                    .map(|c| c.focus(verse.as_str(), quote, occurrence))
                    .unwrap_or_default();
                Some(PaneFocus {
                    pane: pane.id.clone(),
                    title,
                    lit,
                })
            })
            .collect()
    }

    fn passage(&self, chapter: u16, frame: &SourceFrame) -> PassageView {
        let helps = &self.resources.helps;
        let parts_of = |part| self.resources.parts.get(&part);
        let pins = self.pins.as_ref();
        let notes = slot(parts_of(HelpsPart::Notes), pins, || {
            helps
                .notes()
                .iter()
                .filter_map(|note| {
                    let at = self.place(frame, chapter, &note.reference)?;
                    Some(NoteCard {
                        id: item_id(&note.id),
                        panes: self.placed_focus(frame, &at, &note.quote, note.occurrence),
                        chapter: at.chapter,
                        verse: at.verse,
                        quote: note.quote.as_str().to_owned(),
                        occurrence: occurrence(note.occurrence),
                        note: blocks(&note.text),
                        academy: note
                            .support_reference
                            .as_ref()
                            .and_then(|a| a.slug())
                            .map(str::to_owned),
                    })
                })
                .collect()
        });
        let words = slot(parts_of(HelpsPart::WordLinks), pins, || {
            helps
                .word_links()
                .iter()
                .filter_map(|link| {
                    let at = self.place(frame, chapter, &link.reference)?;
                    Some(WordCard {
                        id: item_id(&link.id),
                        panes: self.placed_focus(frame, &at, &link.orig_words, link.occurrence),
                        chapter: at.chapter,
                        verse: at.verse,
                        quote: link.orig_words.as_str().to_owned(),
                        occurrence: occurrence(link.occurrence),
                        category: link.article.category().map(|c| c.folder().to_owned()),
                        slug: word_slug(&link.article),
                    })
                })
                .collect()
        });
        let questions = slot(parts_of(HelpsPart::Questions), pins, || {
            helps
                .questions()
                .iter()
                .filter_map(|q| {
                    let at = self.place(frame, chapter, &q.reference)?;
                    Some(QuestionCard {
                        id: item_id(&q.id),
                        chapter: at.chapter,
                        verse: at.verse,
                        question: q.question.clone(),
                        response: q.response.clone(),
                    })
                })
                .collect()
        });
        let simplified = match &self.simplified {
            Slot::Ready(ready) => Slot::Ready(ReadySlot {
                items: ready
                    .items
                    .first()
                    .map(|book| simplified_verses(book, chapter, frame))
                    .unwrap_or_default(),
                dir: ready.dir,
                fallback: ready.fallback,
            }),
            Slot::None => Slot::None,
            Slot::Fetch => Slot::Fetch,
            Slot::Missing => Slot::Missing,
            Slot::OldTwl => Slot::OldTwl,
            Slot::Error { reason } => Slot::Error {
                reason: reason.clone(),
            },
        };
        let other_book = match frame {
            SourceFrame::Mapped { rows } => match rows.first().map(|row| &row.source) {
                Some(RowSource::OtherBook { to })
                    if rows
                        .iter()
                        .all(|row| matches!(row.source, RowSource::OtherBook { .. })) =>
                {
                    Some(to.clone())
                }
                _ => None,
            },
            _ => None,
        };
        PassageView {
            notes,
            words,
            questions,
            simplified,
            simplified_pin: self.simplified_pin.clone(),
            other_book,
        }
    }
}

/// The simplified text's verses of a project chapter: under another numbering, each row's
/// source verse, labelled with its place in the sources (tC4's `simplifiedVerses`).
fn simplified_verses(book: &PaneBook, chapter: u16, frame: &SourceFrame) -> Vec<SimplifiedVerse> {
    match frame {
        SourceFrame::Same => book
            .chapters
            .get(&chapter)
            .map(|c| {
                c.verses
                    .iter()
                    .map(|v| SimplifiedVerse {
                        key: v.key.clone(),
                        text: v.view.text.clone(),
                        source: None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        SourceFrame::Mapped { rows } => rows
            .iter()
            .filter_map(|row| {
                let RowSource::Verse { chapter, verse } = &row.source else {
                    return None;
                };
                Some(SimplifiedVerse {
                    key: row.verse.clone(),
                    text: row_verse(book, row)?.view.text.clone(),
                    source: Some(format!("{chapter}:{verse}")),
                })
            })
            .collect(),
        SourceFrame::Unknown => Vec::new(),
    }
}

impl Handle<Passage> for App {
    async fn handle(&self, req: Passage) -> Result<PassageView, ApiError> {
        let session = self.read(&req.at)?;
        let loaded = self.loaded_book(&session, req.book).await?;
        let frame = loaded.frame(&session.read(), req.chapter);
        let view = tokio::task::spawn_blocking(move || loaded.passage(req.chapter, &frame))
            .await
            .map_err(|stopped| {
                self.internal(
                    "a chapter's helps could not be made",
                    crate::reports::facts(serde_json::json!({
                        "stopped": crate::jobs::stop_reason(stopped),
                    })),
                )
            })?;
        Ok(view)
    }
}

/// A words article as tC4's pane draws it: the whole file, its `# Title` line included
/// (`readTwArticle`, `tC4:src/data/articles.ts:57-66`). tc-helps keeps the title apart; an
/// article whose file had no title line has the slug for its title, and gets no heading back.
pub(crate) fn word_body(title: &str, slug: &str, body: &str) -> String {
    if title == slug {
        body.to_owned()
    } else {
        format!("# {title}\n\n{body}")
    }
}

/// An article found: its title, its markdown and where it is in its repository, as tC4 names it
/// (`ArticleFile::ipath`).
struct Found<'a> {
    title: &'a str,
    body: &'a str,
    path: String,
}

/// The article `link` names, and the part its language comes from. Translation Academy's is
/// found by slug, a word's by folder and slug.
fn find_article<'a>(
    helps: &'a tc_helps::Helps,
    link: &ArticleLink,
    (academy, words): (HelpsPart, HelpsPart),
) -> (Option<Found<'a>>, HelpsPart) {
    match link {
        ArticleLink::Academy { slug } => (
            Slug::parse(slug).ok().and_then(|slug| {
                MANUALS.iter().find_map(|manual| {
                    let id = AcademyArticleId::Dcs {
                        manual: *manual,
                        slug: slug.clone(),
                    };
                    helps.academy_article(&id).map(|f| Found {
                        title: f.item.title.as_str(),
                        body: f.item.body.as_str(),
                        path: format!("{}/{}/01.md", manual.folder(), slug.as_str()),
                    })
                })
            }),
            academy,
        ),
        ArticleLink::Words { category, slug } => (
            category
                .as_deref()
                .and_then(WordCategory::parse)
                .zip(Slug::parse(slug).ok())
                .and_then(|(category, slug)| {
                    let path = format!("payload/{}/{}.md", category.folder(), slug.as_str());
                    helps
                        .word_article(&WordArticleId::Dcs { category, slug })
                        .map(|f| Found {
                            title: f.item.title.as_str(),
                            body: f.item.body.as_str(),
                            path,
                        })
                }),
            words,
        ),
    }
}

/// The article behind a check or a help, as the screens draw it.
pub(crate) fn article_view(loaded: &LoadedBook, link: &ArticleLink) -> ArticleView {
    resources_article_view(
        &BookResourcesRef {
            helps: &loaded.resources.helps,
            parts: &loaded.resources.parts,
            pins: loaded.pins.as_ref(),
        },
        link,
        (HelpsPart::Notes, HelpsPart::WordLinks),
    )
}

/// What an article is read from: a book's or a story project's loaded helps.
pub(crate) struct BookResourcesRef<'a> {
    pub(crate) helps: &'a tc_helps::Helps,
    pub(crate) parts: &'a std::collections::BTreeMap<HelpsPart, PartStatus>,
    pub(crate) pins: Option<&'a ProjectPins>,
}

/// The same article rendering for both resource kinds, retaining its repository path.
pub(crate) fn resources_article_view(
    resources: &BookResourcesRef<'_>,
    link: &ArticleLink,
    parts: (HelpsPart, HelpsPart),
) -> ArticleView {
    let (found, part) = find_article(resources.helps, link, parts);
    let rung = match resources.parts.get(&part) {
        Some(PartStatus::Loaded { rung, .. }) => *rung,
        _ => None,
    };
    match found {
        Some(Found { title, body, path }) => {
            let body = match link {
                ArticleLink::Academy { .. } => body.to_owned(),
                ArticleLink::Words { slug, .. } => word_body(title, slug, body),
            };
            ArticleView::Found {
                title: title.to_owned(),
                path,
                blocks: blocks(&body),
                dir: rung_dir(resources.pins, rung).unwrap_or(Direction::Ltr),
            }
        }
        None => ArticleView::Missing,
    }
}

/// The title of the article `link` names, when the resources carry it.
pub(crate) fn article_title(loaded: &LoadedBook, link: &ArticleLink) -> Option<String> {
    find_article(
        &loaded.resources.helps,
        link,
        (HelpsPart::Notes, HelpsPart::WordLinks),
    )
    .0
    .map(|found| found.title.to_owned())
}

/// [`article_title`] in the helps of either resource kind.
pub(crate) fn resources_article_title(
    resources: &BookResourcesRef<'_>,
    link: &ArticleLink,
    parts: (HelpsPart, HelpsPart),
) -> Option<String> {
    find_article(resources.helps, link, parts)
        .0
        .map(|found| found.title.to_owned())
}

impl Handle<Article> for App {
    async fn handle(&self, req: Article) -> Result<ArticleView, ApiError> {
        let session = self.read(&req.at)?;
        let loaded = self.loaded_book(&session, req.book).await?;
        Ok(article_view(&loaded, &req.link))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// tC4 #118: in `rsc`, Jonah 2:1 is the sources' 1:17. The pane shows that verse under the
    /// project's key, and the next under the next.
    #[test]
    fn a_pane_in_another_numbering_shows_the_source_verse_under_the_projects_key() {
        let usfm = "\\id JON\n\\c 1\n\\p\n\\v 16 The men feared.\n\\v 17 A great fish swallowed Jonah.\n\\c 2\n\\p\n\\v 1 Jonah prayed.\n";
        let pane = pane_book(usfm);
        let rsc = tc_versification::StandardScheme::Rsc.scheme();
        let rows = numbering::rows(rsc, BookId::Jonah, 2, &[(1, 1), (2, 2)]);
        let chapter = mapped_chapter(&pane, &rows);
        assert!(chapter.sections.is_empty(), "one unit per verse");
        let shown: Vec<(&str, &str)> = chapter
            .verses
            .iter()
            .map(|v| (v.key.as_str(), v.text.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("1", "A great fish swallowed Jonah."),
                ("2", "Jonah prayed.")
            ]
        );
        let simplified = simplified_verses(&pane, 2, &SourceFrame::Mapped { rows });
        assert_eq!(simplified[0].source.as_deref(), Some("1:17"));
        assert_eq!(simplified[0].key.as_str(), "1");
    }
}
