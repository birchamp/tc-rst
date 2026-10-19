//! Home (unit B2): the cards, their summaries and order, the Resume record, the unusable
//! folders, and the reads of the dialogs, through `App::request` and the seam.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tc_api::event::Event;
use tc_api::home::{
    BookTileView, Card, Door43Card, HomeCard, HomeView, Languages, List, ProjectKind, RepoPlace,
    ResumePlace, UnusableCard, UnusableReason, Versifications,
};
use tc_api::ids::{ProjectRef, RefPart, StoryNo, WallMs};
use tc_api::prefs::PlaceMode;
use tc_api::shown::{Direction, Known};
use tc_api::test::LastEdit;
use tc_api::{ApiError, Reply};
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::{
    Door43Remote, Listing, OpenFailure, ProjectEntry, ProjectHandle, ProjectPeek, ProjectStore,
    StoreError, SummaryFiles, SummaryStamp, SummaryWatch,
};
use tc_app::events::{Next, Subscription};
use tc_app::home::{card, card_facts, summary_of};
use tc_app::jobs::ProgressSink;
use tc_app::language_search::LanguageIndex;
use tc_app::{App, StoreChoice};
use tc_core::BookId;
use tc_door43::ServerUrl;

use super::common::{Parts, TestApp, folder, repo_root, rid, sample_bible, tit};
use crate::store_world::{self, store_world};

async fn start(store: Arc<dyn ProjectStore>) -> (tempfile::TempDir, TestApp) {
    start_with(store, ServerUrl::production()).await
}

async fn start_with(
    store: Arc<dyn ProjectStore>,
    door43: ServerUrl,
) -> (tempfile::TempDir, TestApp) {
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.door43 = door43;
        deps.store = StoreChoice::Given(store);
    })
    .await;
    (dir, t)
}

async fn ok<E: tc_api::Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

async fn list(app: &App) -> HomeView {
    ok(app, &List {}).await
}

async fn home_card(app: &App, name: &str) -> HomeCard {
    ok(
        app,
        &Card {
            project: folder(name),
        },
    )
    .await
}

/// Waits for `HomeCard` of `name`; panics after five seconds.
async fn card_event(events: &mut Subscription, name: &str) {
    let wanted = Event::HomeCard {
        project: folder(name),
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.next().await {
                Next::Frame(frame) if frame.event == wanted => return,
                Next::Frame(_) => {}
                other => panic!("{other:?}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no HomeCard event for {name}"));
}

/// Whether a `HomeCard` event came by now.
fn card_events(events: &mut Subscription) -> usize {
    let mut n = 0;
    while let Some(Next::Frame(frame)) = events.try_next() {
        n += usize::from(matches!(frame.event, Event::HomeCard { .. }));
    }
    n
}

/// Until no job runs: the summaries the reads queued are done.
async fn quiet(app: &App) {
    for _ in 0..500 {
        let quiet: tc_api::test::QuietView = ok(app, &tc_api::test::Quiet {}).await;
        if !quiet.busy {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the app did not settle");
}

fn drafted(card: &HomeCard) -> Vec<(BookId, Known<u8>)> {
    match &card.books {
        Known::Known(books) => books.iter().map(|b| (b.book, b.drafted.clone())).collect(),
        Known::Unknown => panic!("books unknown"),
    }
}

fn sample_backend_with(names: &[&str]) -> MemoryBackend {
    let backend = MemoryBackend::new();
    for name in names {
        backend.add(folder(name), sample_bible()).unwrap();
    }
    backend
}

/// The server half of tC4's `homeProjectCard` test: before its summary a card's tiles are
/// unknown ("—"), never 0; one job computes it, the card is told to read again, and then it
/// shows tC4's tiles (Titus 11%, Jonah 4%) and no conflict.
#[tokio::test]
async fn a_card_is_unknown_until_its_summary_then_known() {
    let (_dir, t) = start(Arc::new(sample_backend_with(&["sample_burrito"]))).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let home = list(&t.app).await;
    assert_eq!(home.cards.len(), 1);
    let first = &home.cards[0];
    assert_eq!(first.name, "Equipo Ejemplo — Tito y Jonás");
    assert_eq!(first.tag, Known::Known("es-419".into()));
    assert_eq!(first.kind, ProjectKind::Bible);
    assert_eq!(first.door43, Door43Card::Local);
    // In canonical order, as tC4's `sortCanonical`.
    assert_eq!(
        drafted(first),
        [
            (BookId::Jonah, Known::Unknown),
            (BookId::Titus, Known::Unknown)
        ]
    );
    assert_eq!(first.conflicts, Known::Unknown);
    card_event(&mut events, "sample_burrito").await;
    let known = home_card(&t.app, "sample_burrito").await;
    assert_eq!(
        drafted(&known),
        [
            (BookId::Jonah, Known::Known(4)),
            (BookId::Titus, Known::Known(11))
        ]
    );
    assert_eq!(known.conflicts, Known::Known(0));
    // A second read computes nothing again.
    quiet(&t.app).await;
    let before = t.app.counters().summaries_computed;
    let _ = list(&t.app).await;
    let _ = home_card(&t.app, "sample_burrito").await;
    quiet(&t.app).await;
    assert_eq!(t.app.counters().summaries_computed, before);
}

/// tC4's order: newest first by the later of the last use and `meta.dateCreated`, ties by
/// folder name.
#[tokio::test]
async fn home_lists_cards_in_tc4s_order() {
    let backend = sample_backend_with(&["b_copy", "a_copy"]);
    // An empty project has no dateCreated: 0, so after both copies (2026-07-02).
    backend
        .add(
            folder("kr_ulb"),
            tc_app::backend::memory::MemoryProject::empty(
                tc_journal::ProjectId::parse(super::common::PROJECT_ID).unwrap(),
                tc_journal::fold::ProjectKind::Bible,
            ),
        )
        .unwrap();
    let (_dir, t) = start(Arc::new(backend)).await;
    let order = |home: &HomeView| -> Vec<String> {
        home.cards.iter().map(|c| c.project.to_string()).collect()
    };
    assert_eq!(order(&list(&t.app).await), ["a_copy", "b_copy", "kr_ulb"]);
    // Used later than both were created: first.
    let edit = LastEdit {
        request: rid(1),
        project: folder("kr_ulb"),
        place: ResumePlace::Story {
            story: StoryNo::new(1).unwrap(),
            frame: 1,
        },
        mode: PlaceMode::Draft,
        snippet: String::new(),
        at: WallMs(1_800_000_000_000),
    };
    let () = ok(&t.app, &edit).await;
    let home = list(&t.app).await;
    assert_eq!(order(&home), ["kr_ulb", "a_copy", "b_copy"]);
    // A story place in a Bible project offers no Resume.
    assert_eq!(home.resume, None);
}

/// The Resume card names the last place a change was made while its project is listed and
/// holds that place (`resumeRecordHolds`). `test/lastEdit` records it as a save will, and
/// Home hears that the projects changed.
#[tokio::test]
async fn resume_shows_only_a_record_its_project_holds() {
    let (_dir, t) = start(Arc::new(sample_backend_with(&["sample_burrito"]))).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let edit = |book: BookId| LastEdit {
        request: rid(2),
        project: folder("sample_burrito"),
        place: ResumePlace::Book {
            book,
            chapter: 1,
            verse: RefPart::parse("1").unwrap(),
        },
        mode: PlaceMode::Draft,
        // Kept as given, both spaces too.
        snippet: "Pablo, siervo de Dios y  xapóstol".into(),
        at: WallMs(1_792_378_282_100),
    };
    let () = ok(&t.app, &edit(BookId::Titus)).await;
    let mut changed = false;
    while let Some(Next::Frame(frame)) = events.try_next() {
        changed |= frame.event == Event::ProjectsChanged;
    }
    assert!(changed, "Home is told the projects changed");
    let resume = list(&t.app).await.resume.expect("a Resume card");
    assert_eq!(resume.project, folder("sample_burrito"));
    assert_eq!(resume.snippet, "Pablo, siervo de Dios y  xapóstol");
    assert_eq!(resume.mode, PlaceMode::Draft);
    // A book the project does not hold.
    let () = ok(
        &t.app,
        &LastEdit {
            request: rid(3),
            ..edit(BookId::Genesis)
        },
    )
    .await;
    assert_eq!(list(&t.app).await.resume, None);
    // A folder that is not listed.
    let missing = t
        .app
        .request(&LastEdit {
            request: rid(4),
            project: folder("gone"),
            ..edit(BookId::Titus)
        })
        .await
        .unwrap();
    assert!(matches!(missing, Reply::Err(ApiError::BadRequest { .. })));
}

/// A store whose summaries fail, or wait until a test lets them go.
struct Summaries {
    inner: MemoryBackend,
    gate: Arc<(Mutex<Gate>, Condvar)>,
}

#[derive(Default)]
struct Gate {
    fail: bool,
    /// Each card's peek fails as a disk does.
    fail_peek: bool,
    hold: bool,
    asked: usize,
}

impl Summaries {
    fn new(inner: MemoryBackend) -> Self {
        Self {
            inner,
            gate: Arc::default(),
        }
    }

    fn set(&self, edit: impl FnOnce(&mut Gate)) {
        edit(&mut self.gate.0.lock().unwrap());
        self.gate.1.notify_all();
    }

    fn asked(&self) -> usize {
        self.gate.0.lock().unwrap().asked
    }

    /// Waits until a summary is waiting at the gate.
    fn wait_asked(&self, n: usize) {
        let mut gate = self.gate.0.lock().unwrap();
        while gate.asked < n {
            gate = self.gate.1.wait(gate).unwrap();
        }
    }
}

impl ProjectStore for Summaries {
    fn list(&self) -> Result<Listing, StoreError> {
        self.inner.list()
    }

    fn peek(&self, folder: &ProjectRef) -> Result<ProjectPeek, StoreError> {
        if self.gate.0.lock().unwrap().fail_peek {
            return Err(StoreError::Other("the disk said no".into()));
        }
        self.inner.peek(folder)
    }

    fn summary_files(
        &self,
        folder: &ProjectRef,
        books: &[BookId],
        watch: &SummaryWatch,
    ) -> Result<SummaryFiles, StoreError> {
        let mut gate = self.gate.0.lock().unwrap();
        gate.asked += 1;
        self.gate.1.notify_all();
        while gate.hold {
            gate = self.gate.1.wait(gate).unwrap();
        }
        if gate.fail {
            return Err(StoreError::Other("the disk said no".into()));
        }
        drop(gate);
        watch
            .cancel
            .check()
            .map_err(|_| StoreError::Other("cancelled".into()))?;
        self.inner.summary_files(folder, books, watch)
    }

    fn open(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        self.inner.open(folder, progress)
    }

    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        self.inner.reset()
    }
}

/// A failed summary leaves the card "—" and is not tried again by the card's own reads:
/// only the next Home visit tries again (tC4 caches nothing on a failed read and reads again
/// at the next mount).
#[tokio::test]
async fn a_failed_summary_waits_for_the_next_home_visit() {
    let store = Arc::new(Summaries::new(sample_backend_with(&["sample_burrito"])));
    store.set(|g| g.fail = true);
    let (_dir, t) = start(store.clone()).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let _ = list(&t.app).await;
    quiet(&t.app).await;
    assert_eq!(store.asked(), 1);
    assert_eq!(
        card_events(&mut events),
        0,
        "a failure tells no card to read"
    );
    let again = home_card(&t.app, "sample_burrito").await;
    quiet(&t.app).await;
    assert_eq!(drafted(&again)[0].1, Known::Unknown);
    assert_eq!(store.asked(), 1, "the card's read starts no second job");
    store.set(|g| g.fail = false);
    let _ = list(&t.app).await;
    card_event(&mut events, "sample_burrito").await;
    assert_eq!(store.asked(), 2);
    assert_eq!(
        drafted(&home_card(&t.app, "sample_burrito").await)[1].1,
        Known::Known(11)
    );
}

/// A project whose card cannot be read, for another reason than that it went away, is
/// listed as unusable with the reason, never left off Home.
#[tokio::test]
async fn a_card_that_cannot_be_read_is_listed_with_its_reason() {
    let store = Arc::new(Summaries::new(sample_backend_with(&["sample_burrito"])));
    store.set(|g| g.fail_peek = true);
    let (_dir, t) = start(store.clone()).await;
    let home = list(&t.app).await;
    assert!(home.cards.is_empty());
    assert_eq!(
        home.unusable,
        [UnusableCard {
            name: "sample_burrito".into(),
            reason: UnusableReason::FolderUnreadable {
                reason: "the disk said no".into()
            },
        }]
    );
}

/// One summary job runs at a time (AD13): the second card's waits for the first's.
#[tokio::test]
async fn one_summary_job_runs_at_a_time() {
    let store = Arc::new(Summaries::new(sample_backend_with(&["a_copy", "b_copy"])));
    store.set(|g| g.hold = true);
    let (_dir, t) = start(store.clone()).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let _ = list(&t.app).await;
    store.wait_asked(1);
    // Both cards read again while the first job is held: still one job.
    let _ = home_card(&t.app, "a_copy").await;
    let _ = home_card(&t.app, "b_copy").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(store.asked(), 1);
    store.set(|g| g.hold = false);
    card_event(&mut events, "a_copy").await;
    card_event(&mut events, "b_copy").await;
    assert_eq!(store.asked(), 2);
}

/// Opening a project cancels the running summary (AD13): its card stays "—" and no card is
/// told to read; the next Home visit computes it.
#[tokio::test]
async fn cancelling_the_summary_leaves_the_card_unknown() {
    let store = Arc::new(Summaries::new(sample_backend_with(&["sample_burrito"])));
    store.set(|g| g.hold = true);
    let (_dir, t) = start(store.clone()).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let _ = list(&t.app).await;
    store.wait_asked(1);
    t.app.cancel_home_summary();
    store.set(|g| g.hold = false);
    quiet(&t.app).await;
    assert_eq!(card_events(&mut events), 0);
    assert_eq!(
        drafted(&home_card(&t.app, "sample_burrito").await)[0].1,
        Known::Unknown
    );
    let _ = list(&t.app).await;
    card_event(&mut events, "sample_burrito").await;
}

/// A change of the project gives a new stamp, so the next read computes its summary again:
/// two teammates editing Titus 1:1 make one verse conflict on Titus.
#[tokio::test]
async fn a_new_stamp_computes_again() {
    let backend = sample_backend_with(&["sample_burrito"]);
    let (_dir, t) = start(Arc::new(backend.clone())).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let _ = list(&t.app).await;
    card_event(&mut events, "sample_burrito").await;
    let p = folder("sample_burrito");
    let changes = backend
        .teammates_edit_verse(&p, &tit(1, 1), &["Pablo, siervo", "Pablo, esclavo"])
        .unwrap();
    backend.teammate_edits(&p, changes).unwrap();
    let stale = home_card(&t.app, "sample_burrito").await;
    assert_eq!(
        stale.conflicts,
        Known::Unknown,
        "a stale summary is not shown"
    );
    card_event(&mut events, "sample_burrito").await;
    let fresh = home_card(&t.app, "sample_burrito").await;
    assert_eq!(fresh.conflicts, Known::Known(1));
    let Known::Known(books) = &fresh.books else {
        panic!()
    };
    assert_eq!(
        books[1],
        BookTileView {
            book: BookId::Titus,
            drafted: Known::Known(11),
            conflicts: 1
        }
    );
}

/// `test/reset` forgets every summary: the next Home computes them again.
#[tokio::test]
async fn reset_forgets_summaries() {
    let (_dir, t) = start(Arc::new(sample_backend_with(&["sample_burrito"]))).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let _ = list(&t.app).await;
    card_event(&mut events, "sample_burrito").await;
    let () = ok(&t.app, &tc_api::test::Reset { request: rid(9) }).await;
    let home = list(&t.app).await;
    assert_eq!(drafted(&home.cards[0])[0].1, Known::Unknown);
    card_event(&mut events, "sample_burrito").await;
}

/// tC4 #506: a development build names the QA server before the repository when the
/// repository is on it; production names none, and a repository on another server neither.
#[test]
fn debug_card_names_qa_server() {
    let entry = ProjectEntry {
        folder: folder("shared"),
        id: tc_journal::ProjectId::parse(super::common::PROJECT_ID).unwrap(),
        kind: tc_journal::fold::ProjectKind::Bible,
    };
    let peek = |host: &str| ProjectPeek {
        metadata: Err("not read".into()),
        remote: Door43Remote::parse_url(&format!("https://{host}/o/r.git")),
        stamp: SummaryStamp::parse_hex(&"0".repeat(64)),
    };
    let place = |host: &str, server: &ServerUrl| {
        let peek = peek(host);
        let facts = card_facts(&peek.metadata, &entry.folder, ProjectKind::Bible);
        match card(&entry, &facts, &peek, None, server).door43 {
            Door43Card::Shared { place } => place,
            Door43Card::Local => panic!("shared"),
        }
    };
    assert_eq!(
        place("qa.door43.org", &ServerUrl::qa()),
        RepoPlace {
            owner: "o".into(),
            name: "r".into(),
            server: Some("qa.door43.org".into())
        }
    );
    assert_eq!(
        place("qa.door43.org", &ServerUrl::production()).server,
        None
    );
    assert_eq!(place("git.door43.org", &ServerUrl::qa()).server, None);
    assert_eq!(
        place("git.door43.org", &ServerUrl::production()).server,
        None
    );
}

/// A `metadata.json` that cannot be read still gives a card, named after its folder, with
/// its language and books unknown.
#[test]
fn a_card_without_metadata_is_named_after_its_folder() {
    let facts = card_facts(&Err("gone".into()), &folder("mine"), ProjectKind::Bible);
    assert_eq!(facts.name, "mine");
    assert_eq!((facts.tag, facts.books, facts.created), (None, None, 0));
}

#[tokio::test]
async fn home_versifications_are_the_standard_schemes() {
    let (_dir, t) = start(Arc::new(MemoryBackend::new())).await;
    let view: tc_api::home::VersificationsView = ok(&t.app, &Versifications {}).await;
    assert_eq!(view.names, ["eng", "lxx", "org", "rsc", "rso", "vul"]);
}

/// `home/languages` ranks as tC4 and offers a code a project can hold (Q35), and the index
/// is built once per process.
#[tokio::test]
async fn home_languages_answers_tc4s_order() {
    let (_dir, t) = start(Arc::new(MemoryBackend::new())).await;
    let ask = |query: &str| Languages {
        query: query.to_owned(),
    };
    let hausa: tc_api::home::LanguagesView = ok(&t.app, &ask("Hausa")).await;
    let codes: Vec<_> = hausa.suggestions.iter().map(|s| s.code.clone()).collect();
    assert_eq!(codes[..2], [Some("ha".into()), Some("hsl".into())]);
    assert_eq!(hausa.suggestions[0].name, "Hausa");
    assert_eq!(hausa.suggestions[0].door43, None);
    assert!(hausa.suggestions[0].own.is_some());
    let lebanese: tc_api::home::LanguagesView = ok(&t.app, &ask("Lebanese Arabic")).await;
    let first = &lebanese.suggestions[0];
    assert_eq!(first.code.as_deref(), Some("acm-x-lebarabi-c"));
    assert_eq!(first.door43.as_deref(), Some("acm-x-lebarabic"));
    assert_eq!(first.dir, Direction::Rtl);
    let none: tc_api::home::LanguagesView = ok(&t.app, &ask(&"a".repeat(201))).await;
    assert!(none.suggestions.is_empty());
    // the_index_is_built_once
    let builds = LanguageIndex::shared_builds();
    let _: tc_api::home::LanguagesView = ok(&t.app, &ask("es")).await;
    assert_eq!(LanguageIndex::shared_builds(), builds);
    assert_eq!(builds, 1);
}

/// Every folder that holds someone's work is on Home: a folder that cannot be opened is
/// listed by name with its reason, through the wire.
#[tokio::test]
async fn unusable_folders_are_listed_with_their_reason() {
    let world = store_world();
    let projects = std::path::Path::new(store_world::PROJECTS);
    world
        .fs
        .put(&projects.join("broken/metadata.json"), b"{}", None);
    world.fs.put(
        &projects.join("broken/ingredients/x.usfm"),
        b"\\id TIT",
        None,
    );
    world.fs.put(
        &projects.join("damaged/ingredients/tc/project.json"),
        b"{}",
        None,
    );
    tc_store::Vfs::create_dir(&world.fs, &projects.join("empty")).unwrap();
    world.fs.put(&projects.join("stray.txt"), b"x", None);
    let (_dir, t) = start(world.store()).await;
    let home = list(&t.app).await;
    let unusable: Vec<(&str, &UnusableReason)> = home
        .unusable
        .iter()
        .map(|u| (u.name.as_str(), &u.reason))
        .collect();
    assert_eq!(unusable.len(), 2, "{unusable:?}");
    assert_eq!(unusable[0], ("broken", &UnusableReason::NoProjectFile));
    assert!(matches!(
        unusable[1],
        ("damaged", UnusableReason::ProjectFileDamaged { .. })
    ));
    assert_eq!(home.cards.len(), 3);
}

/// The card one backend shows of a project is the card the other shows of the same project:
/// the store reads the derived files, the memory backend renders them from the fold, and one
/// function makes the card of either.
#[tokio::test]
async fn peek_and_summary_agree_on_the_store_and_in_memory() {
    let world = store_world();
    let (conflicted, _) = world.conflicted.clone();
    let memory = MemoryBackend::new();
    for name in [&world.bible, &conflicted] {
        let handle = world.open(name);
        let fold = handle.snapshot().fold().clone();
        let facts = handle.facts();
        handle.close().await.unwrap();
        memory
            .add(
                name.clone(),
                tc_app::backend::memory::MemoryProject {
                    id: facts.id,
                    kind: facts.kind,
                    fold,
                    carried: Default::default(),
                    remote: None,
                },
            )
            .unwrap();
    }
    let card_of = |store: &dyn ProjectStore, name: &ProjectRef| {
        let entry = store
            .list()
            .unwrap()
            .projects
            .into_iter()
            .find(|e| e.folder == *name)
            .unwrap();
        let peek = store.peek(name).unwrap();
        let facts = card_facts(&peek.metadata, name, ProjectKind::Bible);
        let watch = SummaryWatch {
            cancel: Default::default(),
            counters: Default::default(),
        };
        let files = store
            .summary_files(name, facts.books.as_deref().unwrap(), &watch)
            .unwrap();
        let summary = summary_of(&files, |_| None);
        card(
            &entry,
            &facts,
            &peek,
            Some(&summary),
            &ServerUrl::production(),
        )
    };
    for name in [&world.bible, &conflicted] {
        let on_disk = card_of(world.backend.as_ref(), name);
        let in_memory = card_of(&memory, name);
        assert_eq!(on_disk, in_memory, "{name}");
        assert_eq!(on_disk.name, "Sample translation");
        assert_eq!(on_disk.tag, Known::Known("en".into()));
    }
    let conflicted_card = card_of(world.backend.as_ref(), &conflicted);
    assert_eq!(conflicted_card.conflicts, Known::Known(1));
    assert_eq!(
        card_of(world.backend.as_ref(), &world.bible).conflicts,
        Known::Known(0)
    );
}

/// A store project's stamp follows its saves: the same twice, new after a save.
#[tokio::test]
async fn a_store_stamp_follows_saves() {
    let world = store_world();
    let first = world.backend.summary_stamp(&world.bible).unwrap();
    assert_eq!(world.backend.summary_stamp(&world.bible).unwrap(), first);
    let handle = world.open(&world.bible);
    let snapshot = handle.snapshot();
    let token = snapshot.mint(&tit(1, 2), None).unwrap();
    handle
        .submit(tc_app::backend::StoreCommand::save_verse(
            token,
            "Uno".into(),
        ))
        .await
        .unwrap();
    assert_ne!(world.backend.summary_stamp(&world.bible).unwrap(), first);
    handle.close().await.unwrap();
}

/// A project whose saves cannot be looked at (another program holds `.tc/materialized.json`
/// or `.tc/pending`) stays on Home, its counts "—": a folder that holds someone's work never
/// disappears from Home without a word (§4.5).
#[tokio::test]
async fn a_project_whose_stamp_cannot_be_read_stays_on_home() {
    for held in [tc_store::layout::MANIFEST, tc_store::layout::PENDING] {
        let world = store_world();
        let path = held
            .split('/')
            .fold(world.folder(&world.bible), |p, part| p.join(part));
        world.fs.set_unreadable(&path, true);
        let (_dir, t) = start(world.store()).await;
        let home = list(&t.app).await;
        let card = home
            .cards
            .iter()
            .find(|card| card.project == world.bible)
            .unwrap_or_else(|| panic!("{held} held: {home:?}"));
        assert!(
            drafted(card).iter().all(|(_, d)| *d == Known::Unknown),
            "{held} held: {card:?}"
        );
        quiet(&t.app).await;
        let again = home_card(&t.app, world.bible.as_str()).await;
        assert!(drafted(&again).iter().all(|(_, d)| *d == Known::Unknown));
    }
}

/// The story tiles of tC4's sample OBS project (`homeObsCard`, #328): fifty tiles, story 1
/// "La Creación" at 13%, stories 2 and 3 untitled (no gateway on this computer) at 0%, and
/// the project at 1% drafted.
#[test]
fn the_obs_summary_has_tc4s_story_tiles() {
    let content = repo_root().join("fixtures/tc4/sample-burrito-obs/ingredients/content");
    let mut stories: Vec<(u8, Result<String, String>)> = Vec::new();
    for n in 1..=50u8 {
        let path = content.join(format!("{n:02}.md"));
        if let Ok(text) = std::fs::read_to_string(path) {
            stories.push((n, Ok(text)));
        }
    }
    let files = SummaryFiles {
        stories: Some(Ok(stories)),
        ..SummaryFiles::default()
    };
    let summary = summary_of(&files, |_| None);
    let stories = summary.stories.unwrap();
    assert_eq!(stories.stories.len(), 50);
    assert_eq!(stories.stories[0].title, "La Creación");
    assert_eq!(stories.stories[0].percent, Some(13));
    assert_eq!(stories.stories[1].title, "");
    assert_eq!(stories.stories[1].percent, Some(0));
    assert_eq!(stories.stories[2].percent, Some(0));
    assert_eq!(stories.percent, Some(1));
    // A story that does not read is unknown, never 0, and so is the project.
    let mut broken = files.clone();
    if let Some(Ok(list)) = &mut broken.stories {
        list[1].1 = Err("unreadable".into());
    }
    let summary = summary_of(&broken, |_| None);
    let stories = summary.stories.unwrap();
    assert_eq!(stories.stories[1].percent, None);
    assert_eq!(stories.percent, None);
    // An untitled story takes the gateway's title.
    let summary = summary_of(&files, |n| Some(format!("Gateway {n}")));
    assert_eq!(summary.stories.unwrap().stories[1].title, "Gateway 2");
}

/// The card of a story project has no book tiles and no conflict chip (tC4 draws none).
#[test]
fn a_story_card_has_stories_and_no_chip() {
    let entry = ProjectEntry {
        folder: folder("obs"),
        id: tc_journal::ProjectId::parse(super::common::PROJECT_ID).unwrap(),
        kind: tc_journal::fold::ProjectKind::Stories,
    };
    let peek = ProjectPeek {
        metadata: Ok(serde_json::json!({"identification": {"name": {"en": "Historias"}}})),
        remote: None,
        stamp: SummaryStamp::parse_hex(&"0".repeat(64)),
    };
    let facts = card_facts(&peek.metadata, &entry.folder, ProjectKind::Stories);
    let unknown = card(&entry, &facts, &peek, None, &ServerUrl::production());
    assert_eq!(unknown.books, Known::Known(Vec::new()));
    assert_eq!(unknown.conflicts, Known::Unknown);
    let stories = unknown.stories.unwrap();
    assert_eq!(stories.drafted, Known::Unknown);
    assert!(stories.stories.is_empty());
}
