//! The open and the Understand screen's state and actions (unit B3): tC4's `s.opening`,
//! `book`, `chapter`, `rail`, `helps`, `helpsTab`, `helpsWidth`, `sourceTab`, `helpsHover`,
//! `helpsActive` and `understand.article`, and the actions `openProjectAt`, `openBook`,
//! `setChapter`, `setSourceTab`, `toggleRail`, `toggleHelps`, `setHelpsTab`, `setHelpsWidth`,
//! `hoverHelp`, `focusHelp`, `loadHelpArticle` and `closeHelpArticle`. The decisions are in
//! [`crate::app::understand`].

use std::cell::Cell;

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::helps::{Article, ArticleLink, ArticleView};
use tc_api::home::{LastEditView, ProjectKind, ResumePlace};
use tc_api::ids::ProjectRef;
use tc_api::jobs::Progress;
use tc_api::prefs::{DraftUnit, PlaceChange, PlaceMode, PlaceUpdate, RecordPlace, SetDraftUnit};
use tc_api::refusal::Message;
use tc_api::resources::GatewayPackage;
use tc_api::session::{Close, Open, OpenUnit, OpenedProject};
use tc_core::BookId;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::{Actions, HomeBanner, OpenTarget};
use super::js;
use crate::app::client::CallError;
use crate::app::feedback::HelpRequest;
use crate::app::jobs::JobEnd;
use crate::app::shell::{CurrentProject, Editing, ProjectFlavor, View};
use crate::app::understand::{HelpFocus, HelpsTab, Opening, UnitMode};
use crate::i18n::t_dyn;

/// An article the pane shows: loading, read, or failed (tC4's `understand.article`).
#[derive(Debug, Clone, PartialEq)]
pub enum ArticleState {
    Loading(ArticleLink),
    Read(ArticleLink, ArticleView),
    Failed(ArticleLink, String),
}

impl ArticleState {
    pub fn link(&self) -> &ArticleLink {
        match self {
            Self::Loading(link) | Self::Read(link, _) | Self::Failed(link, _) => link,
        }
    }
}

/// The open's and the Understand screen's view state.
#[derive(Clone)]
pub struct UnderstandState {
    pub opening: ArcRwSignal<Option<Opening>>,
    pub book: ArcRwSignal<Option<BookId>>,
    pub chapter: ArcRwSignal<u16>,
    pub rail: ArcRwSignal<bool>,
    pub helps: ArcRwSignal<bool>,
    /// The dragged width of the helps panel; none for the design's width (#602).
    pub helps_width: ArcRwSignal<Option<f64>>,
    pub helps_tab: ArcRwSignal<HelpsTab>,
    pub source_tab: ArcRwSignal<String>,
    pub helps_hover: ArcRwSignal<Option<HelpFocus>>,
    pub helps_active: ArcRwSignal<Option<HelpFocus>>,
    pub article: ArcRwSignal<Option<ArticleState>>,
    /// Section or Verse, as this computer last showed the open project.
    pub unit_mode: ArcRwSignal<UnitMode>,
    /// The frame in focus on the story screens (tC4 `storyFrame`; 0 is the title). A story's
    /// number is kept in `chapter`, where tC4 keeps it.
    pub story_frame: ArcRwSignal<Option<u16>>,
}

impl UnderstandState {
    pub fn new(book: Option<BookId>, chapter: u16) -> Self {
        Self {
            opening: ArcRwSignal::new(None),
            book: ArcRwSignal::new(book),
            chapter: ArcRwSignal::new(chapter),
            rail: ArcRwSignal::new(true),
            helps: ArcRwSignal::new(true),
            helps_width: ArcRwSignal::new(None),
            helps_tab: ArcRwSignal::new(HelpsTab::Notes),
            source_tab: ArcRwSignal::new("ult".to_owned()),
            helps_hover: ArcRwSignal::new(None),
            helps_active: ArcRwSignal::new(None),
            article: ArcRwSignal::new(None),
            unit_mode: ArcRwSignal::new(UnitMode::Section),
            story_frame: ArcRwSignal::new(None),
        }
    }

    /// A new project or book: nothing of the last one's focus or article stays.
    fn clear_focus(&self) {
        self.helps_hover.try_set(None);
        self.helps_active.try_set(None);
        self.article.try_set(None);
    }
}

thread_local! {
    /// Counts the opens asked for: only the newest may land (tC4's `openProjectSeqRef`).
    static OPENS: Cell<u64> = const { Cell::new(0) };
    /// Counts the articles asked for (tC4's `articleSeqRef`).
    static ARTICLES: Cell<u64> = const { Cell::new(0) };
}

fn next(counter: &'static std::thread::LocalKey<Cell<u64>>) -> u64 {
    counter.with(|c| {
        c.set(c.get() + 1);
        c.get()
    })
}

fn current(counter: &'static std::thread::LocalKey<Cell<u64>>) -> u64 {
    counter.with(Cell::get)
}

/// Where an open lands, when the person's action names it (the Resume card).
#[derive(Debug, Clone, Copy)]
struct Landing {
    mode: PlaceMode,
    chapter: u16,
    /// The Check tool open when the person left (B5a).
    tool: Option<tc_api::check::Tool>,
}

fn view_of(mode: PlaceMode) -> View {
    match mode {
        PlaceMode::Read => View::Read,
        PlaceMode::Draft => View::Draft,
        PlaceMode::Check => View::Check,
    }
}

/// The words of a refused open on Home, with the "Ask for help" it offers (tC4 #378).
fn refused_banner(error: &ApiError) -> HomeBanner {
    let (code, text, report) = match error {
        ApiError::Refused(refusal) => {
            let owned = refusal.vars();
            let vars: Vec<(&str, &dyn std::fmt::Display)> = owned
                .iter()
                .map(|(k, v)| (*k, v as &dyn std::fmt::Display))
                .collect();
            // Home's banner leads with "The project could not be opened.", so a refusal that
            // carries the store's reason shows the reason, as tC4 shows its store's message.
            let text = match refusal.detail() {
                Some(detail) => detail.to_owned(),
                None => t_dyn(refusal.key(), &vars, None),
            };
            (refusal.code().unwrap_or("open").to_owned(), text, None)
        }
        ApiError::Internal { report } => ("internal".to_owned(), error.to_string(), Some(*report)),
        other => ("open".to_owned(), other.to_string(), None),
    };
    HomeBanner {
        help: Some(HelpRequest {
            code,
            text: text.clone(),
            report,
        }),
        text,
    }
}

/// What an open that Settings asked for goes on to do.
enum After {
    Nothing,
    /// Get source texts shows over the project.
    Sources,
    /// The conflict review shows in Translate at its tab (B11).
    Review(tc_api::conflicts::ReviewTab),
    /// The change of checking language is asked over the project.
    Gateway(Box<GatewayPackage>),
}

impl Actions {
    // ---- the open (S14) --------------------------------------------------------------------

    /// A book tile or a story tile (tC4 `openProjectAt`): the project opens there, at the
    /// place remembered for it, else in Understand at chapter 1 (#507).
    pub fn open_project_at(&self, project: ProjectRef, target: OpenTarget) {
        let unit = match target {
            OpenTarget::Book(book) => OpenUnit::Book(book),
            OpenTarget::Story(story) => OpenUnit::Story(story),
        };
        self.open(project, Some(unit), None, After::Nothing);
    }

    /// The Resume card (tC4's `ResumeCard`): the project opens at the record's book and
    /// chapter, in its mode.
    pub fn resume(&self, resume: &LastEditView) {
        let (unit, chapter) = match &resume.place {
            ResumePlace::Book { book, chapter, .. } => (OpenUnit::Book(*book), *chapter),
            ResumePlace::Story { story, .. } => (OpenUnit::Story(*story), u16::from(story.get())),
        };
        self.open(
            resume.project.clone(),
            Some(unit),
            Some(Landing {
                mode: resume.mode,
                chapter,
                tool: resume.tool,
            }),
            After::Nothing,
        );
    }

    /// Settings' "Manage source texts" (tC4 `openSettingsProject`, then `openSources`): the
    /// project opens at its first book, and Get source texts shows over it.
    pub fn open_then_sources(&self, project: ProjectRef) {
        self.open(project, None, None, After::Sources);
    }

    /// Settings' checking-language card (tC4 `chooseSettingsGateway`): the project opens, and the
    /// change of checking language is asked over it.
    pub fn open_then_gateway(&self, project: ProjectRef, package: GatewayPackage) {
        self.open(project, None, None, After::Gateway(Box::new(package)));
    }

    /// A card's conflict chip and the sync toast's Review (B11): the project opens in
    /// Translate at `book`, then the review.
    pub(super) fn open_for_review(
        &self,
        project: ProjectRef,
        book: Option<BookId>,
        tab: tc_api::conflicts::ReviewTab,
    ) {
        self.open(
            project,
            book.map(OpenUnit::Book),
            Some(Landing {
                mode: PlaceMode::Draft,
                chapter: 1,
                tool: None,
            }),
            After::Review(tab),
        );
    }

    /// The review is open over the project: the sync toast that asked for it closes (#724),
    /// and Translate goes to the book of the tab's first conflict (tC4
    /// `openProjectConflicts`), so Back and the ULT verse are that verse's.
    fn reviewing(&self, tab: tc_api::conflicts::ReviewTab) {
        self.open_review(tab, None);
        let s = self.s();
        let Some(at) = s
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
        else {
            return;
        };
        if s.review.with_untracked(Option::is_none) {
            return;
        }
        if s.sync_result
            .with_untracked(|t| t.as_ref().map(|t| t.project.clone()))
            .is_some_and(|p| p == at.project)
        {
            s.sync_result.try_set(None);
        }
        let this = self.clone();
        spawn_local(async move {
            let list = tc_api::conflicts::List {
                at: at.clone(),
                tab,
            };
            let Ok(view) = this.runtime().api().call(&list).await else {
                return;
            };
            let tc_api::conflicts::ReviewEntries::Conflicts(entries) = &view.entries else {
                return;
            };
            let Some(book) = entries
                .first()
                .and_then(|e| crate::app::review::place_book(&e.place))
            else {
                return;
            };
            let s = this.s();
            let same = s
                .project
                .with_untracked(|p| p.as_ref().is_some_and(|p| p.at == at));
            if same
                && s.review.with_untracked(Option::is_some)
                && s.u.book.get_untracked() != Some(book)
            {
                s.u.clear_focus();
                s.u.chapter.try_set(1);
                s.u.book.try_set(Some(book));
            }
        });
    }

    fn open(
        &self,
        project: ProjectRef,
        unit: Option<OpenUnit>,
        landing: Option<Landing>,
        after: After,
    ) {
        let attempt = next(&OPENS);
        let s = self.s().clone();
        s.u.opening.try_set(Some(Opening::new(js::now_ms())));
        s.book_error.try_set(None);
        let this = self.clone();
        spawn_local(async move {
            if !this.runtime().saves().gate().await {
                if current(&OPENS) == attempt {
                    this.s().u.opening.try_set(None);
                }
                return;
            }
            let request = Open {
                request: js::random_request_id(),
                project,
                unit,
            };
            let ended = match this.runtime().api().call(&request).await {
                Ok(job) => {
                    let report = |progress: &Progress| {
                        this.s().u.opening.try_update(|opening| {
                            if let Some(opening) = opening {
                                opening.report(progress);
                            }
                        });
                    };
                    match this
                        .follow::<Open>(job, report, || current(&OPENS) == attempt)
                        .await
                    {
                        JobEnd::Done(opened) => Some(Ok(*opened)),
                        JobEnd::Failed(error) => Some(Err(error)),
                        JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => None,
                    }
                }
                Err(error) => Some(Err(match error {
                    CallError::Api(error) => error,
                    other => ApiError::BadRequest {
                        field: "open".to_owned(),
                        reason: other.to_string(),
                    },
                })),
            };
            if current(&OPENS) != attempt {
                return;
            }
            match ended {
                Some(Ok(opened)) => {
                    this.opened(opened, unit, landing);
                    match after {
                        After::Nothing => {}
                        After::Sources => this.open_sources(),
                        After::Gateway(package) => this.ask_gateway_change(*package),
                        After::Review(tab) => this.reviewing(tab),
                    }
                }
                Some(Err(error)) => {
                    let s = this.s();
                    s.u.opening.try_set(None);
                    s.book_error.try_set(Some(refused_banner(&error)));
                }
                None => {
                    this.s().u.opening.try_set(None);
                }
            }
        });
    }

    /// The open's outcome: the project's screens, at the place the person returns to.
    fn opened(&self, opened: OpenedProject, unit: Option<OpenUnit>, landing: Option<Landing>) {
        let s = self.s();
        let flavor = match opened.kind {
            ProjectKind::Bible => ProjectFlavor::TextTranslation,
            ProjectKind::Stories => ProjectFlavor::TextStories,
        };
        let place = landing.or(opened.place.map(|p| Landing {
            mode: p.mode,
            chapter: p.chapter,
            tool: p.tool,
        }));
        let book = match unit {
            Some(OpenUnit::Book(book)) => Some(book),
            _ => opened.books.first().copied(),
        };
        s.u.clear_focus();
        s.u.unit_mode.try_set(match opened.draft_unit {
            DraftUnit::Section => UnitMode::Section,
            DraftUnit::Verse => UnitMode::Verse,
        });
        s.u.book
            .try_set(book.filter(|_| flavor == ProjectFlavor::TextTranslation));
        // A story project opens at the story a tile names, else where the person left it, else
        // at story 1 (#507); the number is kept where a Bible keeps its chapter.
        let chapter = match unit {
            Some(OpenUnit::Story(story)) if flavor == ProjectFlavor::TextStories => {
                u16::from(story.get())
            }
            _ => place.map_or(1, |p| p.chapter.max(1)),
        };
        s.u.chapter.try_set(chapter);
        s.u.story_frame
            .try_set((flavor == ProjectFlavor::TextStories).then_some(1));
        s.editing.try_set(Editing::None);
        s.view_error.try_set(None);
        s.project.try_set(Some(CurrentProject {
            at: opened.session.at,
            id: opened.session.id,
            name: opened.name,
            language_tag: opened.tag,
            dir: opened.dir,
            flavor,
        }));
        let view = place.map_or(View::Read, |p| view_of(p.mode));
        s.review.try_set(None);
        // A resume into Check opens its tool at once: no picker shows meanwhile (#518, #513).
        self.reset_check();
        if let Some(place) = place
            && let crate::app::check::CheckView::Tool(tool) =
                crate::app::check::landing(place.mode, place.tool)
        {
            self.open_check_tool(tool);
        }
        s.view.try_set(view);
        // Understand keeps the overlay up, as tC4's 'prepare' stage, until its book (or its
        // story) is read; the screens later units build have no read yet.
        if view == View::Read {
            s.u.opening.try_update(|opening| {
                if let Some(opening) = opening {
                    opening.preparing();
                }
            });
        } else {
            s.u.opening.try_set(None);
        }
    }

    /// The screen has its book: the open is over (tC4 clears `s.opening` after `openBook`).
    pub fn book_shown(&self) {
        self.s().u.opening.try_set(None);
    }

    /// The close of the screen's session at Home, behind the person (§4.3). A stale scope
    /// (the server ran again) closes nothing.
    pub(super) fn close_session(&self, project: Option<CurrentProject>) {
        let s = self.s();
        self.reset_check();
        s.u.clear_focus();
        s.u.book.try_set(None);
        s.u.chapter.try_set(1);
        s.u.story_frame.try_set(None);
        s.u.opening.try_set(None);
        let Some(project) = project else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let close = Close {
                request: js::random_request_id(),
                at: project.at,
            };
            let _ = this.runtime().api().call(&close).await;
        });
    }

    // ---- the book and the chapter --------------------------------------------------------

    /// A book of the rail (tC4 `openBook`): drained first; chapter 1 of that book.
    pub fn open_book(&self, book: BookId) {
        let this = self.clone();
        spawn_local(async move {
            if !this.runtime().saves().gate().await {
                return;
            }
            let u = &this.s().u;
            u.clear_focus();
            u.chapter.try_set(1);
            if u.book.get_untracked() != Some(book) {
                // A tool's list is the book's: another book shows the picker.
                this.reset_check();
            }
            u.book.try_set(Some(book));
        });
    }

    /// A chapter of the rail (tC4 `setChapter`): a help's focus names a verse of the chapter
    /// left, so it goes; the place is remembered.
    pub fn set_chapter(&self, chapter: u16) {
        let this = self.clone();
        spawn_local(async move {
            if !this.runtime().saves().gate().await {
                return;
            }
            let s = this.s();
            s.u.helps_hover.try_set(None);
            s.u.helps_active.try_set(None);
            s.u.chapter.try_set(chapter);
            let (Some(project), Some(book)) = (s.project.get_untracked(), s.u.book.get_untracked())
            else {
                return;
            };
            let place = RecordPlace {
                at: project.at,
                request: js::random_request_id(),
                place: PlaceUpdate::Book {
                    book,
                    mode: Some(PlaceMode::Read),
                    chapter: Some(chapter),
                    verse: PlaceChange::Clear,
                    tool: None,
                },
            };
            let _ = this.runtime().api().call(&place).await;
        });
    }

    /// A story of the story rail (tC4 `openStory`): drained first; the story opens at frame 1,
    /// and the place is remembered. A check list is the story's: another story shows the
    /// picker.
    pub fn open_story(&self, story: tc_api::ids::StoryNo) {
        if self.s().editing.with_untracked(|editing| editing.is_open()) {
            return;
        }
        let this = self.clone();
        spawn_local(async move {
            if !this.runtime().saves().gate().await {
                return;
            }
            if this.s().editing.with_untracked(|editing| editing.is_open()) {
                return;
            }
            let s = this.s();
            if s.u.chapter.get_untracked() != u16::from(story.get()) {
                this.reset_check();
            }
            s.u.helps_hover.try_set(None);
            s.u.helps_active.try_set(None);
            s.u.article.try_set(None);
            s.u.chapter.try_set(u16::from(story.get()));
            s.u.story_frame.try_set(Some(1));
            let Some(project) = s.project.get_untracked() else {
                return;
            };
            let mode = match s.view.get_untracked() {
                View::Draft => PlaceMode::Draft,
                _ => PlaceMode::Read,
            };
            let place = RecordPlace {
                at: project.at,
                request: js::random_request_id(),
                place: PlaceUpdate::Story {
                    story,
                    mode: Some(mode),
                    frame: PlaceChange::Set(1),
                    tool: None,
                },
            };
            let _ = this.runtime().api().call(&place).await;
        });
    }

    /// The frame in focus on a story screen (tC4 `setStoryFrame`): its helps lead, and the place
    /// is remembered.
    pub fn set_story_frame(&self, frame: u16) {
        let s = self.s();
        s.u.story_frame.try_set(Some(frame));
        let (Some(project), Some(story)) = (
            s.project.get_untracked(),
            crate::app::story::story_of(s.u.chapter.get_untracked()),
        ) else {
            return;
        };
        let request = RecordPlace {
            at: project.at,
            request: js::random_request_id(),
            place: PlaceUpdate::Story {
                story,
                mode: None,
                frame: PlaceChange::Set(frame),
                tool: None,
            },
        };
        let this = self.clone();
        spawn_local(async move {
            let _ = this.runtime().api().call(&request).await;
        });
    }

    pub fn set_source_tab(&self, tab: String) {
        self.s().u.source_tab.try_set(tab);
    }

    pub fn toggle_rail(&self) {
        self.s().u.rail.try_update(|on| *on = !*on);
    }

    pub fn toggle_helps(&self) {
        self.s().u.helps.try_update(|on| *on = !*on);
    }

    pub fn set_helps_tab(&self, tab: HelpsTab) {
        self.s().u.helps_tab.try_set(tab);
    }

    pub fn set_helps_width(&self, width: f64) {
        self.s().u.helps_width.try_set(Some(width));
    }

    /// The Section/Verse switch: the screen's grouping now, and this computer's for the
    /// project.
    pub fn set_unit_mode(&self, mode: UnitMode) {
        let s = self.s();
        s.u.unit_mode.try_set(mode);
        let Some(project) = s.project.get_untracked() else {
            return;
        };
        let request = SetDraftUnit {
            at: project.at,
            request: js::random_request_id(),
            unit: match mode {
                UnitMode::Section => DraftUnit::Section,
                UnitMode::Verse => DraftUnit::Verse,
            },
        };
        let this = self.clone();
        spawn_local(async move {
            let _ = this.runtime().api().call(&request).await;
        });
    }

    // ---- the helps' focus and articles ---------------------------------------------------

    /// Hovering a card lights its words in the source; leaving it ends that.
    pub fn hover_help(&self, focus: Option<HelpFocus>) {
        self.s().u.helps_hover.try_set(focus);
    }

    /// A click on a card focuses it, or ends the focus when it had it (tC4 `focusHelp`).
    pub fn focus_help(&self, focus: Option<HelpFocus>) {
        self.s().u.helps_active.try_update(|active| {
            *active = match (active.take(), focus) {
                (Some(was), Some(new)) if was.id == new.id => None,
                (_, new) => new,
            };
        });
    }

    /// An article link (tC4 `loadHelpArticle`): the pane opens on it; a second click on the
    /// article shown changes nothing, a failed one reads again.
    pub fn load_help_article(&self, link: ArticleLink) {
        let s = self.s().clone();
        let same = s.u.article.with_untracked(|article| {
            matches!(article, Some(ArticleState::Loading(l) | ArticleState::Read(l, _)) if *l == link)
        });
        if same {
            return;
        }
        let Some(project) = s.project.get_untracked() else {
            return;
        };
        let book = s.u.book.get_untracked();
        let attempt = next(&ARTICLES);
        s.u.article
            .try_set(Some(ArticleState::Loading(link.clone())));
        let this = self.clone();
        spawn_local(async move {
            // A story project's helps are its own: no book to name.
            let result = match book {
                Some(book) => {
                    this.runtime()
                        .read_once(&Article {
                            at: project.at.clone(),
                            book,
                            link: link.clone(),
                        })
                        .await
                }
                None => {
                    this.runtime()
                        .read_once(&tc_api::helps::StoryArticle {
                            at: project.at.clone(),
                            link: link.clone(),
                        })
                        .await
                }
            };
            if current(&ARTICLES) != attempt {
                return;
            }
            let state = match result {
                Ok(view) => ArticleState::Read(link, view),
                Err(CallError::Dropped) => return,
                Err(error) => ArticleState::Failed(link, error.to_string()),
            };
            this.s().u.article.try_set(Some(state));
        });
    }

    /// Retry on a tab that failed: the chapter's helps (a story's, its own) are read again.
    pub fn reload_helps(&self) {
        if let Some((at, book, chapter)) = self.place() {
            self.runtime()
                .cache()
                .refresh(&tc_api::helps::Passage { at, book, chapter });
        } else if let Some((at, story)) = self.story_place() {
            self.runtime()
                .cache()
                .refresh(&tc_api::helps::Story { at, story });
        }
    }

    /// The open story project's scope and story.
    pub(super) fn story_place(&self) -> Option<(tc_api::ids::Scope, tc_api::ids::StoryNo)> {
        let s = self.s();
        Some((
            s.project.get_untracked()?.at,
            crate::app::story::story_of(s.u.chapter.get_untracked())?,
        ))
    }

    /// Retry on a pane that failed: the chapter's panes are read again.
    pub fn reload_sources(&self) {
        if let Some((at, book, chapter)) = self.place() {
            self.runtime()
                .cache()
                .refresh(&tc_api::editor::Sources { at, book, chapter });
        }
    }

    fn place(&self) -> Option<(tc_api::ids::Scope, BookId, u16)> {
        let s = self.s();
        Some((
            s.project.get_untracked()?.at,
            s.u.book.get_untracked()?,
            s.u.chapter.get_untracked(),
        ))
    }

    pub fn close_help_article(&self) {
        next(&ARTICLES);
        self.s().u.article.try_set(None);
    }
}
