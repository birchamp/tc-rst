//! Open and Understand (unit B3): the open job, the close at Home, the open book with its rail
//! and comments, the source panes of a chapter, and the helps.
//!
//! A book's resources are read once per open book (`helps_load_one_book`): the session keeps
//! the current and the previous book's ([`BookCache`]), each with its source panes cut into
//! chapters ([`crate::sources`]), and every chapter's panes and helps are answered from them.
//! Nothing here writes the project.

use std::sync::Arc;

use tc_api::editor::{
    Book, BookView, CommentView, PaneText, RailBook, SourcePane, Sources, SourcesView,
};
use tc_api::helps::{
    Article, ArticleLink, ArticleView, NoteCard, PaneFocus, Passage, PassageView, QuestionCard,
    ReadySlot, SimplifiedVerse, Slot, WordCard,
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
use tc_journal::record::CommentTarget;
use tc_journal::{ChangeId, PinGroup, Record, RecordKey};
use tc_resources::{
    BookResources, HelpsChoice, Missing, PartStatus, ProjectPins, Rung, TextRole, TextStatus,
};

use crate::app::App;
use crate::counters::{AppCounter, CountingSnapshot};
use crate::dispatch::Handle;
use crate::jobs::{JobCtx, JobEnd, JobSpec};
use crate::session::ProjectSession;
use crate::sources::{ChapterModel, PaneBook, pane_book};

/// How many books a session keeps loaded: the current and the previous (§4.4).
const BOOKS_KEPT: usize = 2;

/// One source text of a loaded book.
enum LoadedText {
    Ready(PaneBook),
    NotInstalled,
    Missing,
    Error(String),
}

struct LoadedPane {
    id: String,
    text: LoadedText,
}

/// What a session read for one book: its helps and its source texts, cut into chapters.
pub(crate) struct LoadedBook {
    book: BookId,
    /// The versions of the pins and the helps setting it was read under: a change of either
    /// reads again.
    stamp: Vec<Vec<ChangeId>>,
    pins: Option<ProjectPins>,
    resources: BookResources,
    panes: Vec<LoadedPane>,
    simplified: Slot<PaneBook>,
    simplified_pin: Option<String>,
}

/// The books a session has read, newest last. The lock is held across a load, so two reads of
/// one book at once load it once.
#[derive(Default)]
pub(crate) struct BookCache {
    books: tokio::sync::Mutex<Vec<Arc<LoadedBook>>>,
}

fn not_found(what: &str) -> ApiError {
    ApiError::BadRequest {
        field: "book".to_owned(),
        reason: format!("{what} is not in the project"),
    }
}

/// The project's pins, from its `Pins` records; none when no group has a record (tC4's "no
/// pins recorded", which shows every help as not included).
fn project_pins(read: &CountingSnapshot) -> Result<Option<ProjectPins>, String> {
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

/// The versions of what a book's resources are read under: each pin group's and the helps
/// setting's heads. They are in memory, so a book read again costs no value read.
fn resources_stamp(read: &CountingSnapshot) -> Vec<Vec<ChangeId>> {
    PinGroup::ALL
        .into_iter()
        .map(RecordKey::Pins)
        .chain(helps_setting())
        .map(|key| read.heads(&key))
        .collect()
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
fn missing_slot<T>(missing: &Missing) -> Slot<T> {
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
fn rung_dir(pins: Option<&ProjectPins>, rung: Option<Rung>) -> Option<Direction> {
    let set = pins?.set(rung?)?;
    let gateway = tc_resources::gateways::of_set(set)?;
    Some(match gateway.direction {
        tc_resources::gateways::Direction::Ltr => Direction::Ltr,
        tc_resources::gateways::Direction::Rtl => Direction::Rtl,
    })
}

/// A part's slot, with `items` when it loaded.
fn slot<T>(
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
            text: resources
                .text(&TextRole::Pane(pane.id.clone()))
                .map_or(LoadedText::Missing, text_state),
        })
        .collect();
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
        simplified,
        simplified_pin,
    }
}

impl App {
    /// The open session's resources of `book`, read at its first need and kept with the
    /// previous book's.
    async fn loaded_book(
        &self,
        session: &Arc<ProjectSession>,
        book: BookId,
    ) -> Result<Arc<LoadedBook>, ApiError> {
        let read = session.read();
        let stamp = resources_stamp(&read);
        let mut books = session.books().books.lock().await;
        if let Some(found) = books.iter().find(|b| b.book == book && b.stamp == stamp) {
            return Ok(found.clone());
        }
        let pins = project_pins(&read).map_err(|reason| ApiError::BadRequest {
            field: "pins".to_owned(),
            reason,
        })?;
        let choice = helps_choice(&read);
        let app = self.clone();
        let loaded = tokio::task::spawn_blocking(move || load(book, stamp, pins, &choice, &app))
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
        books.retain(|b| b.book != book);
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
        let metadata = peek
            .map(|p| p.metadata)
            .unwrap_or_else(|| Err("the project's metadata could not be read".to_owned()));
        let facts = crate::home::card_facts(&metadata, &req.project, kind);
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
        let panes = loaded
            .panes
            .iter()
            .map(|pane| SourcePane {
                id: pane.id.clone(),
                dir: Direction::Ltr,
                text: match &pane.text {
                    LoadedText::Ready(book) => PaneText::Ready(
                        book.chapters
                            .get(&req.chapter)
                            .map(ChapterModel::view)
                            .unwrap_or_default(),
                    ),
                    LoadedText::NotInstalled => PaneText::NotInstalled,
                    LoadedText::Missing => PaneText::Missing,
                    LoadedText::Error(reason) => PaneText::Error {
                        reason: reason.clone(),
                    },
                },
            })
            .collect();
        Ok(SourcesView { panes })
    }
}

// ---------------------------------------------------------------------------------------
// helps/passage, helps/article
// ---------------------------------------------------------------------------------------

/// A help's reference as tC4's `contextId.reference` holds it: the chapter and verse parts
/// as written (`1:1`, `1:intro`, `front:intro`, `1:1-3`).
fn parts(reference: &HelpRef) -> Option<(RefPart, RefPart)> {
    let text = reference.tsv_text();
    let (chapter, verse) = text.split_once(':')?;
    Some((RefPart::parse(chapter).ok()?, RefPart::parse(verse).ok()?))
}

fn in_chapter(chapter: &RefPart, wanted: u16) -> bool {
    chapter.as_str().parse::<u16>().ok() == Some(wanted)
}

fn blocks(markdown: &str) -> Vec<ArticleBlock> {
    tc_helps::blocks::render_article_blocks(markdown)
        .into_iter()
        .map(|b| ArticleBlock {
            kind: match b.kind {
                tc_helps::blocks::BlockKind::Heading => BlockKind::H,
                tc_helps::blocks::BlockKind::Item => BlockKind::Li,
                tc_helps::blocks::BlockKind::Paragraph => BlockKind::P,
            },
            quote: b.quote,
            spans: b
                .spans
                .into_iter()
                .map(|s| ArticleSpan {
                    text: s.text,
                    bold: s.bold,
                    italic: s.italic,
                })
                .collect(),
        })
        .collect()
}

/// tC4's number for an occurrence: `-1` for every one, `0` for none, and 1 where the
/// resource records none.
fn occurrence(o: Occurrence) -> i32 {
    match o {
        Occurrence::NotTied => 0,
        Occurrence::Nth(n) => i32::try_from(n.get()).unwrap_or(i32::MAX),
        Occurrence::All => -1,
        Occurrence::Unrecorded => 1,
    }
}

fn item_id(id: &HelpItemId) -> String {
    match id {
        HelpItemId::Row(row) => row.as_str().to_owned(),
        HelpItemId::Aquifer(content) => content.as_str().to_owned(),
    }
}

/// A words article's name: its slug, or an Aquifer article's content id.
fn word_slug(id: &WordArticleId) -> String {
    match id {
        WordArticleId::Dcs { slug, .. } => slug.as_str().to_owned(),
        WordArticleId::Aquifer(content) => content.as_str().to_owned(),
    }
}

/// The manuals tC4 probes for a note's article, in its order (`TA_SECTIONS`,
/// `tC4:src/data/articles.ts:18`).
const MANUALS: [Manual; 4] = [
    Manual::Translate,
    Manual::Checking,
    Manual::Process,
    Manual::Intro,
];

impl LoadedBook {
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

    fn passage(&self, chapter: u16) -> PassageView {
        let helps = &self.resources.helps;
        let parts_of = |part| self.resources.parts.get(&part);
        let pins = self.pins.as_ref();
        let notes = slot(parts_of(HelpsPart::Notes), pins, || {
            helps
                .notes()
                .iter()
                .filter_map(|note| {
                    let (c, v) = parts(&note.reference)?;
                    in_chapter(&c, chapter).then(|| NoteCard {
                        id: item_id(&note.id),
                        panes: self.focus(chapter, &v, &note.quote, note.occurrence),
                        chapter: c,
                        verse: v,
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
                    let (c, v) = parts(&link.reference)?;
                    in_chapter(&c, chapter).then(|| WordCard {
                        id: item_id(&link.id),
                        panes: self.focus(chapter, &v, &link.orig_words, link.occurrence),
                        chapter: c,
                        verse: v,
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
                    let (c, v) = parts(&q.reference)?;
                    in_chapter(&c, chapter).then(|| QuestionCard {
                        id: item_id(&q.id),
                        chapter: c,
                        verse: v,
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
                    .and_then(|book| book.chapters.get(&chapter))
                    .map(|c| {
                        c.verses
                            .iter()
                            .map(|v| SimplifiedVerse {
                                key: v.key.clone(),
                                text: v.view.text.clone(),
                            })
                            .collect()
                    })
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
        PassageView {
            notes,
            words,
            questions,
            simplified,
            simplified_pin: self.simplified_pin.clone(),
        }
    }
}

impl Handle<Passage> for App {
    async fn handle(&self, req: Passage) -> Result<PassageView, ApiError> {
        let session = self.read(&req.at)?;
        let loaded = self.loaded_book(&session, req.book).await?;
        let view = tokio::task::spawn_blocking(move || loaded.passage(req.chapter))
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
fn word_body(title: &str, slug: &str, body: &str) -> String {
    if title == slug {
        body.to_owned()
    } else {
        format!("# {title}\n\n{body}")
    }
}

impl Handle<Article> for App {
    async fn handle(&self, req: Article) -> Result<ArticleView, ApiError> {
        let session = self.read(&req.at)?;
        let loaded = self.loaded_book(&session, req.book).await?;
        let helps = &loaded.resources.helps;
        let (found, part) = match &req.link {
            ArticleLink::Academy { slug } => (
                Slug::parse(slug).ok().and_then(|slug| {
                    MANUALS.iter().find_map(|manual| {
                        let id = AcademyArticleId::Dcs {
                            manual: *manual,
                            slug: slug.clone(),
                        };
                        helps
                            .academy_article(&id)
                            .map(|f| (f.item.title.clone(), f.item.body.clone()))
                    })
                }),
                HelpsPart::Notes,
            ),
            ArticleLink::Words { category, slug } => (
                category
                    .as_deref()
                    .and_then(WordCategory::parse)
                    .zip(Slug::parse(slug).ok())
                    .and_then(|(category, slug)| {
                        helps.word_article(&WordArticleId::Dcs { category, slug })
                    })
                    .map(|f| {
                        (
                            f.item.title.clone(),
                            word_body(&f.item.title, slug, &f.item.body),
                        )
                    }),
                HelpsPart::WordLinks,
            ),
        };
        let rung = match loaded.resources.parts.get(&part) {
            Some(PartStatus::Loaded { rung, .. }) => *rung,
            _ => None,
        };
        Ok(match found {
            Some((title, body)) => ArticleView::Found {
                title,
                blocks: blocks(&body),
                dir: rung_dir(loaded.pins.as_ref(), rung).unwrap_or(Direction::Ltr),
            },
            None => ArticleView::Missing,
        })
    }
}
