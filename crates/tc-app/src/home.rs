//! Home (APP-DESIGN §4.5, unit B2): the cards, their summaries, their order and the Resume
//! record, and the reads of Home's dialogs.
//!
//! A card's frame (name, language, books, Door43 place) comes from `ProjectStore::peek`, at
//! every read. Its counts (each book's drafted percentage, the stories, the verse conflicts)
//! come from a [`Summary`], computed off the request by one `HomeSummary` job at a time
//! (AD13) from the project's derived files, by the same functions for every backend, and
//! kept in memory under the [`SummaryStamp`] it was read at. A card whose summary is missing
//! or stale shows "—" (tC4 D30) and queues the job; the job's end tells that card to read
//! again (`Event::HomeCard`). A save gives a new stamp, so nothing is invalidated by hand. A
//! failed summary is not retried until the next Home visit (`home/list`), as tC4 reads a
//! card's progress again only at a new mount.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tc_api::event::Event;
use tc_api::home::{
    BookTileView, Card, Door43Card, HomeCard, HomeView, LanguageSuggestion, Languages,
    LanguagesView, LastEditView, List, ProjectKind, RepoPlace, ResumePlace, StoriesView,
    StoryTileView, UnusableCard, UnusableReason, Versifications, VersificationsView,
};
use tc_api::ids::{ProjectRef, StoryNo, WallMs};
use tc_api::jobs::{JobKind, JobOutcome, Phase};
use tc_api::shown::{Direction, Known};
use tc_api::{ApiError, Refusal};
use tc_core::BookId;
use tc_door43::ServerUrl;

use crate::app::{App, lock};
use crate::backend::{
    ProjectEntry, ProjectPeek, ProjectStore, StoreError, SummaryFiles, SummaryStamp, SummaryWatch,
};
use crate::counters::AppCounters;
use crate::dispatch::Handle;
use crate::drafted::{BookDraft, usfm_draft};
use crate::jobs::{CancelToken, JobEnd, JobRegistry, JobSpec};
use crate::language_search::{
    LanguageIndex, MAX_QUERY, Offer, SUGGESTION_LIMIT, language_name, offered_code,
};
use crate::prefs::{EditUnit, LastEdit};

// ---------------------------------------------------------------------------------------
// The summary
// ---------------------------------------------------------------------------------------

/// What a card's counts are made of. Serializable: C1a keeps it across launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// The asked books, in order; `None` for a book whose text could not be read.
    pub books: Vec<(BookId, Option<BookDraft>)>,
    pub stories: Option<StoriesSummary>,
    /// Verse conflicts by book; `None` when `conflicts.json` could not be read.
    pub verse_conflicts: Option<BTreeMap<BookId, u32>>,
}

/// A story project's progress (`obsStoryProgress`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoriesSummary {
    /// `None` when a story could not be read, or the stories could not be listed.
    pub percent: Option<u8>,
    pub stories: Vec<StorySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorySummary {
    pub number: u8,
    /// The drafted title, else the gateway's, else empty. Kept, so a persisted summary
    /// spares a later launch every resource read.
    pub title: String,
    pub percent: Option<u8>,
}

/// The verse conflict kinds (tC4 #679 counts verse conflicts; `ConflictKind::is_verse`).
const VERSE_KINDS: [&str; 4] = ["verseText", "verseStructure", "passage", "structureIssue"];

/// Verse conflicts by book, from `conflicts.json`.
fn verse_conflicts(bytes: &[u8]) -> Option<BTreeMap<BookId, u32>> {
    #[derive(Deserialize)]
    struct File {
        conflicts: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        key: Value,
        kind: String,
    }
    let file: File = serde_json::from_slice(bytes).ok()?;
    let mut counts = BTreeMap::new();
    for entry in file.conflicts {
        if !VERSE_KINDS.contains(&entry.kind.as_str()) {
            continue;
        }
        let key = serde_json::to_vec(&entry.key).ok()?;
        let key = tc_journal::json::parse(&key).ok()?;
        let book = tc_journal::canon::decode_key(&key).ok()?.book()?;
        *counts.entry(book).or_insert(0) += 1;
    }
    Some(counts)
}

/// A summary of `files`. `gateway_title` gives the gateway's title of a story with no
/// drafted title (tC4 #328).
pub fn summary_of(
    files: &SummaryFiles,
    gateway_title: impl FnMut(u8) -> Option<String>,
) -> Summary {
    let books = files
        .books
        .iter()
        .map(|(book, usfm)| {
            let draft = usfm.as_ref().ok().and_then(|usfm| usfm_draft(usfm).ok());
            (*book, draft)
        })
        .collect();
    let stories = files.stories.as_ref().map(|listing| match listing {
        Err(_) => StoriesSummary {
            percent: None,
            stories: Vec::new(),
        },
        Ok(stories) => {
            let parsed: Vec<(u8, Option<_>)> = stories
                .iter()
                .map(|(n, text)| {
                    let story = text
                        .as_ref()
                        .ok()
                        .and_then(|text| tc_obs::story::parse_story(text).ok());
                    (*n, story)
                })
                .collect();
            let progress = tc_obs::progress::obs_progress(
                parsed.iter().map(|(n, story)| (*n, story.as_ref())),
                gateway_title,
            );
            StoriesSummary {
                percent: progress.percent,
                stories: progress
                    .stories
                    .into_iter()
                    .map(|tile| StorySummary {
                        number: tile.number,
                        title: tile.title,
                        percent: tile.percent,
                    })
                    .collect(),
            }
        }
    });
    let verse_conflicts = match &files.conflicts {
        Ok(None) => Some(BTreeMap::new()),
        Ok(Some(bytes)) => verse_conflicts(bytes),
        Err(_) => None,
    };
    Summary {
        books,
        stories,
        verse_conflicts,
    }
}

// ---------------------------------------------------------------------------------------
// The card
// ---------------------------------------------------------------------------------------

/// What a card's frame shows, read from `metadata.json` as tC4's platform summary reads it
/// (the mock's `summaryOf`, `toProjectSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFacts {
    pub name: String,
    /// `None` when `metadata.json` could not be read.
    pub tag: Option<String>,
    pub dir: Direction,
    /// `meta.dateCreated` in milliseconds; 0 when it is missing or does not parse
    /// (`Date.parse(generated_date) || 0`).
    pub created: i64,
    /// A Bible's books in canonical order; empty for a story project; `None` when
    /// `metadata.json` could not be read.
    pub books: Option<Vec<BookId>>,
}

/// The facts of a card from its `metadata.json`. A file that cannot be read still gives a
/// card, named after its folder: the project opens, and its open writes the file again.
pub fn card_facts(
    metadata: &Result<Value, String>,
    folder: &ProjectRef,
    kind: ProjectKind,
) -> CardFacts {
    let Ok(meta) = metadata else {
        return CardFacts {
            name: folder.to_string(),
            tag: None,
            dir: Direction::Ltr,
            created: 0,
            books: None,
        };
    };
    let locale = meta
        .pointer("/meta/defaultLocale")
        .and_then(Value::as_str)
        .unwrap_or("en");
    let names = meta.pointer("/identification/name");
    let name = names
        .and_then(|names| names.get(locale))
        .or_else(|| names.and_then(Value::as_object)?.values().next())
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map_or_else(|| folder.to_string(), str::to_owned);
    let language = meta.pointer("/languages/0");
    let tag = language
        .and_then(|l| l.get("tag"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let dir = match language
        .and_then(|l| l.get("scriptDirection"))
        .and_then(Value::as_str)
    {
        Some("rtl") => Direction::Rtl,
        _ => Direction::Ltr,
    };
    let created = meta
        .pointer("/meta/dateCreated")
        .and_then(Value::as_str)
        .and_then(crate::js::parse_utc_millis)
        .unwrap_or(0);
    let books = match kind {
        ProjectKind::Stories => Vec::new(),
        ProjectKind::Bible => {
            let scope = meta.pointer("/type/flavorType/currentScope");
            let mut books: Vec<BookId> = scope
                .and_then(Value::as_object)
                .map(|scope| scope.keys().filter_map(|code| code.parse().ok()).collect())
                .unwrap_or_default();
            books.sort();
            books.dedup();
            books
        }
    };
    CardFacts {
        name,
        tag: Some(tag),
        dir,
        created,
        books: Some(books),
    }
}

/// Where a shared project is, for its card. The server is named only when the app talks
/// to a server other than production and the repository is on it (tC4 #506).
fn door43_card(peek: &ProjectPeek, door43: &ServerUrl) -> Door43Card {
    let Some(remote) = &peek.remote else {
        return Door43Card::Local;
    };
    let ours = door43.as_url().host_str().map(str::to_owned);
    let production = ServerUrl::production()
        .as_url()
        .host_str()
        .map(str::to_owned);
    let server = (ours != production && ours.as_deref() == Some(remote.host.as_str()))
        .then(|| remote.host.clone());
    Door43Card::Shared {
        place: RepoPlace {
            owner: remote.owner.clone(),
            name: remote.repo.clone(),
            server,
        },
    }
}

fn known<T>(value: Option<T>) -> Known<T> {
    value.map_or(Known::Unknown, Known::Known)
}

/// A card from its facts and, when it is known, its summary.
pub fn card(
    entry: &ProjectEntry,
    facts: &CardFacts,
    peek: &ProjectPeek,
    summary: Option<&Summary>,
    door43: &ServerUrl,
) -> HomeCard {
    let kind = api_kind(entry.kind);
    let drafted_of = |book: BookId| -> Option<u8> {
        summary?
            .books
            .iter()
            .find(|(b, _)| *b == book)
            .and_then(|(_, draft)| draft.map(BookDraft::percent))
    };
    let conflicts = summary.and_then(|s| s.verse_conflicts.as_ref());
    let books = facts.books.as_ref().map(|books| {
        books
            .iter()
            .map(|book| BookTileView {
                book: *book,
                drafted: known(drafted_of(*book)),
                conflicts: conflicts.and_then(|c| c.get(book)).copied().unwrap_or(0),
            })
            .collect()
    });
    let stories = (kind == ProjectKind::Stories).then(|| {
        let stories = summary.and_then(|s| s.stories.as_ref());
        StoriesView {
            drafted: known(stories.and_then(|s| s.percent)),
            stories: stories
                .map(|s| {
                    s.stories
                        .iter()
                        .filter_map(|story| {
                            Some(StoryTileView {
                                story: StoryNo::new(story.number)?,
                                title: story.title.clone(),
                                drafted: known(story.percent),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            // B7 and C3 record the stories a person edits; until then none is recent.
            recent: Vec::new(),
        }
    });
    HomeCard {
        project: entry.folder.clone(),
        name: facts.name.clone(),
        tag: known(facts.tag.clone()),
        dir: facts.dir,
        kind,
        door43: door43_card(peek, door43),
        books: known(books),
        stories,
        // tC4 draws no conflict chip on a story project (`performLoadConflicts`).
        conflicts: match kind {
            ProjectKind::Bible => known(conflicts.map(|c| c.values().sum())),
            ProjectKind::Stories => Known::Unknown,
        },
    }
}

pub(crate) fn api_kind(kind: tc_journal::fold::ProjectKind) -> ProjectKind {
    match kind {
        tc_journal::fold::ProjectKind::Bible => ProjectKind::Bible,
        tc_journal::fold::ProjectKind::Stories => ProjectKind::Stories,
    }
}

/// Newest first by the later of the project's last use and `meta.dateCreated`, either
/// missing as 0 (`refreshProjects`, `tC4:src/state.jsx:3925-3929`); ties by folder name.
pub fn order(cards: &mut [(HomeCard, i64)], last_used: impl Fn(&ProjectRef) -> Option<WallMs>) {
    cards.sort_by(|(a, a_created), (b, b_created)| {
        let at = |card: &HomeCard, created: i64| {
            last_used(&card.project).map_or(0, |at| at.0).max(created)
        };
        at(b, *b_created)
            .cmp(&at(a, *a_created))
            .then_with(|| a.project.cmp(&b.project))
    });
}

/// The Resume card, only while its project is on Home and holds its place
/// (`resumeRecordHolds`): a book of a Bible project, or a story of a story project.
pub fn resume(edit: Option<LastEdit>, cards: &[HomeCard]) -> Option<LastEditView> {
    let edit = edit?;
    let card = cards.iter().find(|card| card.project == edit.project)?;
    let place = match edit.unit {
        EditUnit::Book {
            book,
            chapter,
            verse,
        } => {
            let Known::Known(books) = &card.books else {
                return None;
            };
            if card.kind != ProjectKind::Bible || !books.iter().any(|b| b.book == book) {
                return None;
            }
            ResumePlace::Book {
                book,
                chapter,
                verse,
            }
        }
        EditUnit::Story { story, frame } => {
            if card.kind != ProjectKind::Stories {
                return None;
            }
            ResumePlace::Story { story, frame }
        }
    };
    Some(LastEditView {
        project: edit.project,
        place,
        mode: edit.mode,
        tool: edit.tool,
        snippet: edit.snippet,
    })
}

// ---------------------------------------------------------------------------------------
// The cache and its job
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Kept {
    Ready(Arc<Summary>),
    Failed,
}

struct Queued {
    folder: ProjectRef,
    /// The project's id, which the kept summary names.
    project: tc_journal::ProjectId,
    books: Vec<BookId>,
    /// The stamp read before the summary's files: a save meanwhile gives a newer one, so
    /// the next card sees this summary stale.
    stamp: SummaryStamp,
}

#[derive(Default)]
struct CacheState {
    /// By folder: the project and the stamp each summary was computed of.
    kept: HashMap<ProjectRef, (tc_journal::ProjectId, SummaryStamp, Kept)>,
    /// The projects of the last listing, so a card's own read needs no listing of its own.
    listed: HashMap<ProjectRef, ProjectEntry>,
    queue: VecDeque<Queued>,
    running: Option<(ProjectRef, CancelToken)>,
    /// Raised by a reset: a job of an earlier generation stores nothing.
    generation: u64,
}

/// The summaries of this launch, over those app data keeps across launches (§4.4, C1a's D7).
/// One job at a time.
#[derive(Default)]
pub(crate) struct SummaryCache {
    state: Mutex<CacheState>,
}

/// What the cache holds for a folder at a stamp.
#[derive(Debug, Clone)]
pub(crate) enum Lookup {
    Ready(Arc<Summary>),
    Failed,
    Missing,
}

/// What a summary job needs, cloned out of the app.
#[derive(Clone)]
struct JobDeps {
    app: App,
    store: Arc<dyn ProjectStore>,
    counters: Arc<AppCounters>,
    jobs: JobRegistry,
}

impl SummaryCache {
    /// What is kept for `entry`'s folder, when it was computed of `entry`'s project at `stamp`.
    pub(crate) fn get(&self, entry: &ProjectEntry, stamp: SummaryStamp) -> Lookup {
        match lock(&self.state).kept.get(&entry.folder) {
            Some((project, kept, summary)) if *project == entry.id && *kept == stamp => {
                match summary {
                    Kept::Ready(summary) => Lookup::Ready(summary.clone()),
                    Kept::Failed => Lookup::Failed,
                }
            }
            _ => Lookup::Missing,
        }
    }

    /// The last summary of `folder` this launch computed or read, whatever its stamp: the
    /// book rail shows it for the books that are not open, as tC4's rail shows the progress
    /// Home loaded (`progressByProject`).
    pub(crate) fn last_known(&self, folder: &ProjectRef) -> Option<Arc<Summary>> {
        match lock(&self.state).kept.get(folder) {
            Some((_, _, Kept::Ready(summary))) => Some(summary.clone()),
            _ => None,
        }
    }

    /// Keeps `summary` as this launch's for `entry`'s folder at `stamp`.
    fn keep(&self, entry: &ProjectEntry, stamp: SummaryStamp, summary: Arc<Summary>) {
        lock(&self.state).kept.insert(
            entry.folder.clone(),
            (entry.id, stamp, Kept::Ready(summary)),
        );
    }

    /// The projects of the last listing.
    fn remember_listing(&self, entries: &[ProjectEntry]) {
        lock(&self.state).listed = entries
            .iter()
            .map(|entry| (entry.folder.clone(), entry.clone()))
            .collect();
    }

    pub(crate) fn listed(&self, folder: &ProjectRef) -> Option<ProjectEntry> {
        lock(&self.state).listed.get(folder).cloned()
    }

    /// Queues one summary of `entry`'s folder unless one is queued or running.
    fn ensure(
        &self,
        deps: &JobDeps,
        entry: &ProjectEntry,
        books: Vec<BookId>,
        stamp: SummaryStamp,
    ) {
        let mut state = lock(&self.state);
        let folder = &entry.folder;
        let busy = state.running.as_ref().is_some_and(|(f, _)| f == folder)
            || state.queue.iter().any(|q| q.folder == *folder);
        if busy {
            return;
        }
        state.queue.push_back(Queued {
            folder: folder.clone(),
            project: entry.id,
            books,
            stamp,
        });
        Self::start_next(&mut state, deps);
    }

    /// A new Home visit tries a failed summary again.
    pub(crate) fn forget_failures(&self) {
        lock(&self.state)
            .kept
            .retain(|_, (_, _, kept)| matches!(kept, Kept::Ready(_)));
    }

    /// Stops the running summary: opening a project cancels it (AD13; B3 calls it through
    /// [`App::cancel_home_summary`]). The card stays "—" until the next Home visit.
    fn cancel_running(&self) {
        if let Some((_, cancel)) = &lock(&self.state).running {
            cancel.cancel();
        }
    }

    /// `test/reset`: the running job stops and every summary and failure is forgotten.
    #[cfg(feature = "test-hooks")]
    pub(crate) fn reset(&self) {
        let mut state = lock(&self.state);
        if let Some((_, cancel)) = state.running.take() {
            cancel.cancel();
        }
        state.queue.clear();
        state.kept.clear();
        state.generation += 1;
    }

    fn start_next(state: &mut CacheState, deps: &JobDeps) {
        if state.running.is_some() {
            return;
        }
        let Some(next) = state.queue.pop_front() else {
            return;
        };
        let generation = state.generation;
        let cancel = CancelToken::default();
        state.running = Some((next.folder.clone(), cancel.clone()));
        let job_deps = deps.clone();
        let spec = JobSpec::new(JobKind::HomeSummary, Phase::Summary).project(next.folder.clone());
        deps.jobs.start_blocking(spec, move |ctx| {
            let watch = SummaryWatch {
                cancel: cancel.clone(),
                counters: job_deps.counters.clone(),
            };
            // The job's own cancel (`jobs/cancel`) stops it as the cache's does.
            let job_cancel = ctx.cancel.clone();
            let outcome = job_deps
                .store
                .summary_files(&next.folder, &next.books, &watch)
                .ok()
                .filter(|_| !cancel.is_cancelled() && !job_cancel.is_cancelled())
                .map(|files| {
                    let mut titles = GatewayTitles::new(&job_deps.app, &files);
                    summary_of(&files, |n| titles.title(n))
                });
            job_deps
                .counters
                .add(crate::counters::AppCounter::SummariesComputed, 1);
            job_deps.app.summary_done(generation, &next, outcome);
            if cancel.is_cancelled() || job_cancel.is_cancelled() {
                Err(JobEnd::Cancelled)
            } else {
                Ok(JobOutcome::Finished)
            }
        });
    }
}

/// The gateway's story titles of a story project (tC4 #328): from the project's `obs` pin,
/// when the pinned story resource is on this computer. Read once per summary, lazily.
struct GatewayTitles<'a> {
    app: &'a App,
    resources: Option<&'a Vec<u8>>,
    installed: Option<Option<tc_resources::Installed>>,
}

impl<'a> GatewayTitles<'a> {
    fn new(app: &'a App, files: &'a SummaryFiles) -> Self {
        Self {
            app,
            resources: files.resources.as_ref().and_then(|r| r.as_ref().ok()),
            installed: None,
        }
    }

    fn resolve(&self) -> Option<tc_resources::Installed> {
        let json: Value = serde_json::from_slice(self.resources?).ok()?;
        let pins = tc_resources::ProjectPins::from_resources_json(&json).ok()?;
        let store = &self.app.inner.resources;
        let resolution = tc_resources::resolve::resolve_slot(
            &pins,
            tc_resources::SetSlot::Obs,
            tc_resources::Collection::Stories,
            store,
        )?
        .usable()?;
        store.dcs(&resolution.pin).ok()?
    }

    fn title(&mut self, story: u8) -> Option<String> {
        if self.installed.is_none() {
            self.installed = Some(self.resolve());
        }
        let installed = self.installed.as_ref()?.as_ref()?;
        let wanted = format!("content/{story:02}.md");
        let path = installed
            .files()
            .find(|path| path.strip_prefix("ingredients/").unwrap_or(path) == wanted)?
            .to_owned();
        let text = installed.read_text(&path).ok()?;
        let title = tc_obs::story::parse_story(&text).ok()?.title;
        (!title.is_empty()).then_some(title)
    }
}

impl App {
    /// Stops the running Home summary: opening a project cancels it (AD13).
    pub fn cancel_home_summary(&self) {
        self.inner.home.cancel_running();
    }

    fn job_deps(&self) -> JobDeps {
        JobDeps {
            app: self.clone(),
            store: self.inner.sessions.store().clone(),
            counters: self.inner.counters.clone(),
            jobs: self.inner.jobs.clone(),
        }
    }

    /// A summary job ended: its summary is kept (or its failure, until the next Home visit),
    /// its card is told to read again, and the next queued job starts.
    fn summary_done(&self, generation: u64, job: &Queued, summary: Option<Summary>) {
        let cache = &self.inner.home;
        let mut state = lock(&cache.state);
        if state.generation != generation {
            return;
        }
        state.running = None;
        let summary = summary.map(Arc::new);
        let kept = summary.clone().map_or(Kept::Failed, Kept::Ready);
        state
            .kept
            .insert(job.folder.clone(), (job.project, job.stamp, kept));
        SummaryCache::start_next(&mut state, &self.job_deps());
        drop(state);
        if let Some(summary) = summary {
            // App data keeps it for the next launch (C1a's D7).
            if let Ok(value) = serde_json::to_value(&*summary) {
                self.inner.prefs.record_summary(
                    job.folder.clone(),
                    crate::prefs::StoredSummary {
                        project: job.project,
                        stamp: job.stamp,
                        summary: value,
                    },
                );
            }
            self.inner.bus.publish(Event::HomeCard {
                project: job.folder.clone(),
            });
        }
    }

    /// The summary app data keeps for `entry`'s folder, when it was computed of `entry`'s
    /// project at `stamp` and reads as a summary: what a launch shows with no job (C1a's D7).
    /// A kept summary that does not read is no summary.
    fn kept_summary(&self, entry: &ProjectEntry, stamp: SummaryStamp) -> Option<Arc<Summary>> {
        let stored = self.inner.prefs.summary(&entry.folder)?;
        if stored.project != entry.id || stored.stamp != stamp {
            return None;
        }
        let summary: Summary = serde_json::from_value(stored.summary).ok()?;
        let summary = Arc::new(summary);
        self.inner.home.keep(entry, stamp, summary.clone());
        Some(summary)
    }

    /// One card: its frame now, its counts from the kept summary, and a summary queued when
    /// there is none for this stamp.
    fn home_card(&self, entry: &ProjectEntry, peek: &ProjectPeek) -> (HomeCard, i64) {
        let kind = api_kind(entry.kind);
        let facts = card_facts(&peek.metadata, &entry.folder, kind);
        // With no stamp no summary can be said to be the folder's, and none could be kept.
        let summary = peek
            .stamp
            .and_then(|stamp| match self.inner.home.get(entry, stamp) {
                Lookup::Ready(summary) => Some(summary),
                Lookup::Failed => None,
                Lookup::Missing => {
                    let kept = self.kept_summary(entry, stamp);
                    if kept.is_none() {
                        self.inner.home.ensure(
                            &self.job_deps(),
                            entry,
                            facts.books.clone().unwrap_or_default(),
                            stamp,
                        );
                    }
                    kept
                }
            });
        let card = card(
            entry,
            &facts,
            peek,
            summary.as_deref(),
            &self.inner.config.door43,
        );
        (card, facts.created)
    }

    /// Runs `f` with the store on the blocking pool: the store's reads read files.
    pub(crate) async fn with_store<T: Send + 'static>(
        &self,
        f: impl FnOnce(&dyn ProjectStore) -> T + Send + 'static,
    ) -> Result<T, ApiError> {
        let store = self.inner.sessions.store().clone();
        tokio::task::spawn_blocking(move || f(store.as_ref()))
            .await
            .map_err(|e| {
                self.internal(
                    "Home's read of the projects folder stopped",
                    crate::reports::facts(serde_json::json!({"reason": e.to_string()})),
                )
            })
    }
}

impl Handle<List> for App {
    async fn handle(&self, _req: List) -> Result<HomeView, ApiError> {
        self.inner.home.forget_failures();
        let read = self
            .with_store(|store| {
                let listing = store.list()?;
                let peeks: Vec<_> = listing
                    .projects
                    .iter()
                    .map(|entry| (entry.clone(), store.peek(&entry.folder)))
                    .collect();
                Ok::<_, StoreError>((peeks, listing.unusable, listing.others))
            })
            .await?;
        let (peeks, unusable, others) = read.map_err(|e| {
            self.internal(
                "the projects folder could not be listed",
                crate::reports::facts(serde_json::json!({"reason": e.to_string()})),
            )
        })?;
        let entries: Vec<ProjectEntry> = peeks.iter().map(|(entry, _)| entry.clone()).collect();
        self.inner.home.remember_listing(&entries);
        let mut unusable: Vec<UnusableCard> = unusable
            .into_iter()
            .map(|folder| UnusableCard {
                name: folder.name,
                reason: folder.reason.to_api(),
            })
            .collect();
        let mut cards: Vec<(HomeCard, i64)> = Vec::new();
        for (entry, peek) in &peeks {
            match peek {
                Ok(peek) => cards.push(self.home_card(entry, peek)),
                // The folder went away between the listing and its peek.
                Err(StoreError::NotAProject) => {}
                Err(error @ (StoreError::Other(_) | StoreError::NotKept)) => {
                    unusable.push(UnusableCard {
                        name: entry.folder.to_string(),
                        reason: UnusableReason::FolderUnreadable {
                            reason: error.to_string(),
                        },
                    });
                }
            }
        }
        unusable.sort_by(|a, b| a.name.cmp(&b.name));
        let prefs = &self.inner.prefs;
        order(&mut cards, |folder| prefs.last_used(folder));
        let cards: Vec<HomeCard> = cards.into_iter().map(|(card, _)| card).collect();
        Ok(HomeView {
            resume: resume(prefs.last_edit(), &cards),
            cards,
            unusable,
            others,
        })
    }
}

impl Handle<Card> for App {
    async fn handle(&self, req: Card) -> Result<HomeCard, ApiError> {
        let folder = req.project;
        // The card's project as the last listing named it, so a card's read costs its own
        // files and no listing (§8.2); a folder Home has not listed yet is looked for.
        let listed = self.inner.home.listed(&folder);
        let read = self
            .with_store(move |store| {
                let entry = match listed {
                    Some(entry) => entry,
                    None => store
                        .list()
                        .ok()?
                        .projects
                        .into_iter()
                        .find(|e| e.folder == folder)?,
                };
                let peek = store.peek(&entry.folder).ok()?;
                Some((entry, peek))
            })
            .await?;
        let (entry, peek) = read.ok_or(ApiError::Refused(Refusal::Open(
            tc_api::refusal::OpenRefusal::NotAProject,
        )))?;
        Ok(self.home_card(&entry, &peek).0)
    }
}

impl Handle<Versifications> for App {
    async fn handle(&self, _req: Versifications) -> Result<VersificationsView, ApiError> {
        // `eng` first, as tC4's platform lists them.
        Ok(VersificationsView {
            names: tc_versification::StandardScheme::ALL
                .iter()
                .map(|scheme| scheme.name().to_owned())
                .collect(),
        })
    }
}

impl Handle<Languages> for App {
    async fn handle(&self, req: Languages) -> Result<LanguagesView, ApiError> {
        if req.query.chars().count() > MAX_QUERY {
            return Ok(LanguagesView {
                suggestions: Vec::new(),
            });
        }
        let index = LanguageIndex::shared().map_err(|e| {
            self.internal(
                "the language list does not read",
                crate::reports::facts(serde_json::json!({"reason": e.to_string()})),
            )
        })?;
        let suggestions = index
            .suggest(&req.query, SUGGESTION_LIMIT)
            .into_iter()
            .map(|row| {
                let name = language_name(row).to_owned();
                let code = match offered_code(&row.lc) {
                    Offer::Same => Some(row.lc.clone()),
                    Offer::Adjusted(code) => Some(code),
                    Offer::Unusable => None,
                };
                LanguageSuggestion {
                    own: (row.ln != name).then(|| row.ln.clone()),
                    door43: (code.as_deref() != Some(row.lc.as_str())).then(|| row.lc.clone()),
                    name,
                    code,
                    dir: row.ld,
                }
            })
            .collect();
        Ok(LanguagesView { suggestions })
    }
}
