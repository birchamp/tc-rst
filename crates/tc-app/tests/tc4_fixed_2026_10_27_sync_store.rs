//! tC4 issues fixed upstream by 2026-10-27, tried on the rebuild's app with the real Door43
//! port on `tc-door43-fake` (report of 2026-10-27, area sync-store): two computers, each an
//! app over its own store, and one Door43 repository.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test code: a failed unwrap is a failed test; the shared support modules are \
              larger than what this file uses"
)]

#[path = "support/store_world.rs"]
mod store_world;

#[path = "app/common.rs"]
mod common;

use std::path::Path;
use std::sync::Arc;

use common::{Parts, TestApp, folder, open_progress, post, rid, tit, until};
use tc_api::account::SignIn;
use tc_api::door43::{OpenDoor43, RepoSource, SyncProject};
use tc_api::editor::SaveVerse;
use tc_api::ids::{JobId, ProjectRef, Scope};
use tc_api::jobs::{JobOutcome, JobState, ReplacedKind, SyncResultView};
use tc_api::session::Close;
use tc_api::shown::Who;
use tc_api::write::Stored;
use tc_app::App;
use tc_app::StoreChoice;
use tc_app::backend::StoreCommand;
use tc_app::backend::scenario::{ScenarioState, install_resources_into};
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::config::ProjectsFolder;
use tc_door43_fake::seeded::{PARITY_PASSWORD, PARITY_USER};
use tc_door43_fake::{FakeDoor43, RepoSeed};
use tc_journal::command::AlignmentEdit;
use tc_journal::keys::PinGroup;
use tc_journal::record::VerseValue;
use tc_journal::{BookId, ChapterNo, Record, RecordKey, VerseNo};
use tc_store::FakePlatform;
use tc_store::fault::FaultFs;

const SAMPLE: &str = "sample_burrito";
const REPO: &str = "es-419_tit_equipo";

/// One computer: an app on the real Door43 port, the internet on, the parity user signed in.
struct Computer {
    t: TestApp,
    _dir: tempfile::TempDir,
}

/// Computer A: the `shared` scenario's projects on its store, the sample connected to an empty
/// repository of the parity user's on the fake, and its first sync pushed.
async fn computer_a(fake: &FakeDoor43) -> Computer {
    let fs = FaultFs::new();
    let (backend, watch) = store_world::backend_over(
        &fs,
        store_world::APP,
        FakePlatform::default(),
        Arc::default(),
    );
    let state = ScenarioState::from_repository(&common::repo_root(), "shared").unwrap();
    let (store, _) = ScenarioStore::write(backend, &state).unwrap();
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let mut t = app_over(dir.path(), StoreChoice::Given(Arc::new(store)), fake).await;
    t.watch = Some(watch);
    fake.add_repo(RepoSeed {
        owner: PARITY_USER.to_owned(),
        name: REPO.to_owned(),
        subject: "Bible".to_owned(),
        language: "es-419".to_owned(),
        topics: Vec::new(),
        private: false,
        commits: Vec::new(),
    })
    .await
    .unwrap();
    t.app
        .connect_door43(&folder(SAMPLE), PARITY_USER, REPO, "master")
        .await
        .unwrap();
    let a = Computer { t, _dir: dir };
    online(&a.t.app, 1).await;
    a
}

/// Computer B: its own device and an empty projects folder.
async fn computer_b(fake: &FakeDoor43) -> Computer {
    let (backend, watch) = store_world::backend_over(
        &FaultFs::new(),
        store_world::APP,
        FakePlatform {
            host: "computer-b".to_owned(),
            machine: "machine-of-b".to_owned(),
            ..FakePlatform::default()
        },
        Arc::default(),
    );
    let dir = tempfile::tempdir().unwrap();
    let mut t = app_over(dir.path(), StoreChoice::Given(Arc::new(backend)), fake).await;
    t.watch = Some(watch);
    let b = Computer { t, _dir: dir };
    online(&b.t.app, 2).await;
    b
}

async fn app_over(dir: &Path, store: StoreChoice, fake: &FakeDoor43) -> TestApp {
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let url = fake.url().to_owned();
    TestApp::start_with(dir, &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        config.door43 = url.parse().unwrap();
        deps.store = store;
        deps.sync = Arc::new(tc_app::ports::door43::Door43Sync);
    })
    .await
}

async fn online(app: &App, n: u8) {
    post(
        app,
        &tc_api::net::Set {
            request: rid(0xB0 ^ n),
            on: true,
        },
    )
    .await
    .unwrap();
    sign_in(app, 0xB8 ^ n).await;
}

/// Both computers sign in as the parity user, and a sign-in replaces the user's token: each
/// signs in again before it syncs.
async fn sign_in(app: &App, n: u8) {
    post(
        app,
        &SignIn {
            request: rid(n),
            login: PARITY_USER.to_owned(),
            password: PARITY_PASSWORD.to_owned(),
            stay: false,
        },
    )
    .await
    .unwrap();
}

async fn ended(app: &App, job: JobId) -> JobState {
    let mut state = None;
    until("the job ends", || {
        state = app.jobs().view(job).map(|v| v.state);
        matches!(
            state,
            Some(JobState::Done(_) | JobState::Failed(_) | JobState::Cancelled)
        )
    })
    .await;
    state.unwrap()
}

async fn sync(app: &App, project: &ProjectRef, n: u8) -> SyncResultView {
    sign_in(app, n).await;
    let job = post(
        app,
        &SyncProject {
            request: rid(n.wrapping_add(1)),
            project: project.clone(),
        },
    )
    .await
    .unwrap();
    match ended(app, job).await {
        JobState::Done(JobOutcome::Synced(view)) => view,
        other => panic!("the sync ends synced: {other:?}"),
    }
}

async fn open_from_door43(app: &App, n: u8, repo: &str) -> ProjectRef {
    let job = post(
        app,
        &OpenDoor43 {
            request: rid(n),
            repo: RepoSource::new(PARITY_USER, repo).unwrap(),
        },
    )
    .await
    .unwrap();
    match ended(app, job).await {
        JobState::Done(JobOutcome::Created { project }) => project,
        other => panic!("the open from Door43 makes a project: {other:?}"),
    }
}

async fn open_screen(app: &App, project: &ProjectRef) -> Scope {
    app.sessions()
        .open_screen(project, open_progress())
        .await
        .unwrap()
        .at
}

async fn close(app: &App, at: &Scope, n: u8) -> Result<(), tc_api::ApiError> {
    post(
        app,
        &Close {
            at: at.clone(),
            request: rid(n),
        },
    )
    .await
    .map(|_| ())
}

fn projected(app: &App, at: &Scope, key: &RecordKey) -> Option<Record> {
    app.sessions().reader(at).unwrap().read().projected(key)
}

fn words(app: &App, at: &Scope, verse: u16) -> Option<String> {
    match projected(app, at, &tit(1, verse)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        }) => Some(words),
        _ => None,
    }
}

async fn save_verse(app: &App, at: &Scope, verse: u16, n: u8, text: &str) -> Stored {
    let minted = app
        .sessions()
        .reader(at)
        .unwrap()
        .read()
        .mint(&tit(1, verse), None)
        .unwrap();
    post(
        app,
        &SaveVerse {
            at: at.clone(),
            request: rid(n),
            shown: tc_app::wire::token::shown(&minted).unwrap(),
            words: text.to_owned(),
        },
    )
    .await
    .unwrap()
    .stored
}

fn alignment_key(verse: u16) -> RecordKey {
    RecordKey::Alignment(
        BookId::Titus,
        ChapterNo::new(1).unwrap(),
        VerseNo::new(verse).unwrap(),
    )
}

/// The aligner's save of Titus 1:`verse` in the open `at`: its placements as they are, marked
/// done or not, so two computers' saves differ.
async fn align(app: &App, at: &Scope, verse: u16, done: bool) {
    let session = app.sessions().reader(at).unwrap();
    let read = session.read();
    let (alignments, word_bank) = match read.projected(&alignment_key(verse)) {
        Some(Record::Alignment { value, .. }) => (value.alignments, value.word_bank),
        _ => (Vec::new(), Vec::new()),
    };
    let command = StoreCommand::save_alignment(
        read.mint(&alignment_key(verse), None).unwrap(),
        read.mint(&tit(1, verse), None).unwrap(),
        AlignmentEdit {
            alignments,
            word_bank,
            done,
        },
    );
    session.handle().submit(command).await.unwrap();
}

fn pins(app: &App, at: &Scope) -> Vec<(PinGroup, Option<Record>)> {
    PinGroup::ALL
        .into_iter()
        .map(|g| (g, projected(app, at, &RecordKey::Pins(g))))
        .collect()
}

/// tC4 #766 "Pin the installed source texts when an import creates a project with no pins".
/// In 4.1.0 the reachable path that creates a project from somewhere else is Open from Door43.
///
/// Steps: a teammate's repository holds a Titus project with no pins (made by a tool that
/// writes none); computer B opens it from Door43. The project opens with the pins a new
/// project gets, never "No resources pinned". Then the shared sample, which has pins, is
/// opened from Door43 on B: its pins stay exactly the team's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_766_a_project_opened_from_door43_with_no_pins_gets_the_creation_pins() {
    let fake = FakeDoor43::start().await.unwrap();
    tc_door43_fake::seeded::parity_account(&fake).await.unwrap();
    // A repository with a project and no pins, pushed by a store that writes none.
    let raw = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(raw.path().join("projects")).unwrap();
    let store = tc_store::Store::start(tc_store::StoreConfig {
        app_data: raw.path().join("app"),
        projects: raw.path().join("projects"),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(FakePlatform {
            host: "teammate".to_owned(),
            machine: "machine-of-teammate".to_owned(),
            ..FakePlatform::default()
        }),
        clock: Arc::new(tc_store::ManualClock::at(store_world::START_MS)),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    let sources = std::collections::BTreeMap::from([(
        BookId::Titus,
        std::fs::read(common::repo_root().join("fixtures/tc4/test-fixtures/en_ult/TIT.usfm"))
            .unwrap(),
    )]);
    let made = store
        .create(&tc_store::NewProject {
            name: "no_pins".to_owned(),
            kind: tc_journal::fold::ProjectKind::Bible,
            records: store_world::sample_records(&sources, None),
            books: Vec::new(),
        })
        .unwrap();
    let handle = store
        .open(&made, &tc_store::OpenOptions::default())
        .unwrap();
    handle.wait_idle();
    assert!(
        PinGroup::ALL.iter().all(|g| handle
            .snapshot()
            .fold()
            .projected(&RecordKey::Pins(*g))
            .is_none()),
        "the repository's project holds no pin"
    );
    let switch = tc_door43::InternetSwitch::new();
    switch.turn_on();
    let client = tc_door43::Door43Client::new(fake.url().parse().unwrap(), switch).unwrap();
    let session = client
        .mint_token(
            &tc_door43::Credentials::new(
                PARITY_USER,
                tc_door43::Password::new(PARITY_PASSWORD.to_owned()),
            )
            .unwrap(),
            // Another tool's token, so the app's own `translationCore` token stays valid.
            &"another-tool".parse().unwrap(),
            &tc_door43::TokenScope::FOR_SHARING,
        )
        .await
        .unwrap();
    client
        .create_repo(
            &session,
            &tc_door43::NewRepoOwner::SignedInUser,
            &"no_pins".parse().unwrap(),
            tc_door43::Visibility::Public,
        )
        .await
        .unwrap();
    let repo: tc_door43::RepoRef = format!("{PARITY_USER}/no_pins").parse().unwrap();
    tc_sync::connect(&handle, client.server(), &repo, "main")
        .await
        .unwrap();
    struct NoPause;
    impl tc_sync::Pause for NoPause {
        async fn pause(&self, _attempt: u8) {}
    }
    let report = tc_sync::sync(&handle, &client, Some(&session), &NoPause).await;
    assert!(
        matches!(report.ended, tc_sync::machine::Ended::Synced { .. }),
        "{report:?}"
    );
    handle.close().unwrap();

    let b = computer_b(&fake).await;
    let project = open_from_door43(&b.t.app, 10, "no_pins").await;
    let at = open_screen(&b.t.app, &project).await;
    let creation = tc_resources::suite::project_pins();
    for (group, record) in pins(&b.t.app, &at) {
        let Some(Record::Pins { value, .. }) = record else {
            panic!("{group:?} is pinned at the first open: {record:?}");
        };
        assert_eq!(
            Some(value),
            creation.group_value(group).ok(),
            "{group:?} holds the creation pin"
        );
    }
    close(&b.t.app, &at, 11).await.unwrap();

    // A project with pins keeps them: the shared sample, opened on B from Door43.
    let a = computer_a(&fake).await;
    sync(&a.t.app, &folder(SAMPLE), 20).await;
    let at_a = open_screen(&a.t.app, &folder(SAMPLE)).await;
    let team = pins(&a.t.app, &at_a);
    close(&a.t.app, &at_a, 22).await.unwrap();
    // A's sign-in replaced the parity user's token on B.
    sign_in(&b.t.app, 25).await;
    let project = open_from_door43(&b.t.app, 23, REPO).await;
    let at = open_screen(&b.t.app, &project).await;
    assert_eq!(pins(&b.t.app, &at), team, "the team's pins, unchanged");
    close(&b.t.app, &at, 24).await.unwrap();
}

/// tC4 #791 "should team sync tell a translator when a teammate's alignment of the same verse
/// replaced theirs?" Rebuild ruling H5 (`.audit/roadmap/rulings/2026-10-24-H5.md`): the
/// newest wins, the older stays in History, and the Received sheet says what was replaced,
/// as "Juan's alignment of Titus 1:1 replaced yours. Your version is in History."
///
/// Steps: A and B have the shared sample; A aligns Titus 1:1 and marks it done; B aligns
/// Titus 1:1 differently (not done); B syncs, then A syncs, then B syncs again. The sync
/// result of the computer whose alignment lost names it: an alignment row "replaced yours"
/// at Titus 1:1; and the "kept the newest" count counts it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "tC4 #791: same bug in the rebuild, report 2026-10-27"]
async fn tc4_791_after_sync_the_translator_is_told_a_teammates_alignment_replaced_theirs() {
    let fake = FakeDoor43::start().await.unwrap();
    tc_door43_fake::seeded::parity_account(&fake).await.unwrap();
    let a = computer_a(&fake).await;
    sync(&a.t.app, &folder(SAMPLE), 10).await;
    let b = computer_b(&fake).await;
    let project_b = open_from_door43(&b.t.app, 12, REPO).await;

    let at_a = open_screen(&a.t.app, &folder(SAMPLE)).await;
    align(&a.t.app, &at_a, 1, true).await;
    close(&a.t.app, &at_a, 13).await.unwrap();
    let at_b = open_screen(&b.t.app, &project_b).await;
    align(&b.t.app, &at_b, 1, false).await;
    close(&b.t.app, &at_b, 14).await.unwrap();

    let first_b = sync(&b.t.app, &project_b, 20).await;
    let view_a = sync(&a.t.app, &folder(SAMPLE), 22).await;
    let view_b = sync(&b.t.app, &project_b, 24).await;
    // Both versions reached A: the record holds two heads, settled on the newest.
    let at_a = open_screen(&a.t.app, &folder(SAMPLE)).await;
    let heads =
        a.t.app
            .sessions()
            .reader(&at_a)
            .unwrap()
            .read()
            .heads(&alignment_key(1));
    close(&a.t.app, &at_a, 26).await.unwrap();
    assert_eq!(heads.len(), 2, "the two alignments of Titus 1:1 met at A");
    let rows: Vec<_> = view_a
        .replaced
        .iter()
        .chain(&view_b.replaced)
        .filter(|row| row.kind == ReplacedKind::Alignment)
        .cloned()
        .collect();
    assert!(
        rows.iter().any(|row| row.whose.who == Who::Mine),
        "the computer whose alignment of Titus 1:1 lost is told it was replaced \
         (A's result: {view_a:?}; B's second: {view_b:?}; B's first: {first_b:?})"
    );
    assert!(
        view_a.kept_newest + view_b.kept_newest >= 1,
        "the newest-kept count names the alignment (A: {view_a:?}; B: {view_b:?})"
    );
}

/// tC4 #801 "After a sync, show the plain refusal message, and let the user go Home".
///
/// Steps: A has the shared sample open in Translate. B saves Titus 1:2 and syncs. A syncs
/// while the project is open (tC4: every store opened before the sync turns old and refuses
/// each later action with a raw message holding the project's path). A then saves Titus 1:1
/// in the open it had before the sync, and goes Home. The save is not refused (or, if it
/// were, the refusal holds no path), the text is kept, and Home leaves on the first press.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_801_a_save_after_a_sync_is_kept_and_home_leaves() {
    let fake = FakeDoor43::start().await.unwrap();
    tc_door43_fake::seeded::parity_account(&fake).await.unwrap();
    let a = computer_a(&fake).await;
    sync(&a.t.app, &folder(SAMPLE), 10).await;
    let b = computer_b(&fake).await;
    let project_b = open_from_door43(&b.t.app, 12, REPO).await;
    let at_b = open_screen(&b.t.app, &project_b).await;
    assert_eq!(
        save_verse(
            &b.t.app,
            &at_b,
            2,
            13,
            "en la esperanza, como B lo escribio"
        )
        .await,
        Stored::Written
    );
    close(&b.t.app, &at_b, 14).await.unwrap();
    sync(&b.t.app, &project_b, 15).await;

    let at_a = open_screen(&a.t.app, &folder(SAMPLE)).await;
    let view = sync(&a.t.app, &folder(SAMPLE), 20).await;
    assert!(view.received > 0, "the sync brought B's save: {view:?}");
    const TYPED: &str = "Pablo, siervo de Dios, escrito despues de la sincronizacion";
    let minted =
        a.t.app
            .sessions()
            .reader(&at_a)
            .map(|s| s.read().mint(&tit(1, 1), None).unwrap());
    let saved = match minted {
        Ok(minted) => post(
            &a.t.app,
            &SaveVerse {
                at: at_a.clone(),
                request: rid(22),
                shown: tc_app::wire::token::shown(&minted).unwrap(),
                words: TYPED.to_owned(),
            },
        )
        .await
        .map(|ack| ack.stored),
        Err(epoch) => panic!("the open from before the sync is gone: {epoch:?}"),
    };
    if let Err(refused) = &saved {
        let said = format!("{refused:?}");
        assert!(
            !said.contains("/projects") && !said.contains(SAMPLE),
            "a refusal names no project path: {said}"
        );
    }
    assert_eq!(
        saved,
        Ok(Stored::Written),
        "the save after the sync is kept"
    );
    assert_eq!(words(&a.t.app, &at_a, 1).as_deref(), Some(TYPED));
    assert_eq!(
        words(&a.t.app, &at_a, 2).as_deref(),
        Some("en la esperanza, como B lo escribio")
    );
    close(&a.t.app, &at_a, 23)
        .await
        .expect("Home leaves on the first press");
}

/// tC4 #807 "After a sync, a span save that waits on a refused alignment save blocks Home
/// until the app restarts".
///
/// Steps: A has the shared sample open; A aligns a word in Titus 1:1; a sync runs while the
/// project is open; A then saves a section that joins Titus 1:1-2 into a span, in the open it
/// had before the sync; A goes Home. The alignment save, the span save and Home all go
/// through: no save waits on another one that the sync made fail.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_807_a_span_save_after_a_sync_does_not_block_home() {
    let fake = FakeDoor43::start().await.unwrap();
    tc_door43_fake::seeded::parity_account(&fake).await.unwrap();
    let a = computer_a(&fake).await;
    sync(&a.t.app, &folder(SAMPLE), 10).await;
    let at = open_screen(&a.t.app, &folder(SAMPLE)).await;
    align(&a.t.app, &at, 1, true).await;
    sync(&a.t.app, &folder(SAMPLE), 12).await;
    align(&a.t.app, &at, 1, false).await;
    let session = a.t.app.sessions().reader(&at).unwrap();
    let read = session.read();
    let one = words(&a.t.app, &at, 1).unwrap();
    let two = words(&a.t.app, &at, 2).unwrap();
    let displayed = vec![
        tc_app::wire::token::shown(&read.mint(&tit(1, 1), None).unwrap()).unwrap(),
        tc_app::wire::token::shown(&read.mint(&tit(1, 2), None).unwrap()).unwrap(),
    ];
    drop(read);
    drop(session);
    let joined = format!("1-2 {} {}", one.trim(), two.trim());
    let ack = post(
        &a.t.app,
        &tc_api::editor::SaveSection {
            at: at.clone(),
            request: rid(14),
            book: BookId::Titus,
            displayed,
            text: joined,
        },
    )
    .await;
    assert!(
        matches!(&ack, Ok(ack) if ack.stored == Stored::Written),
        "the span save after the sync is kept: {ack:?}"
    );
    assert!(
        matches!(
            projected(&a.t.app, &at, &tit(1, 2)),
            Some(Record::Verse {
                value: VerseValue::Covered { .. },
                ..
            })
        ),
        "1:2 is in the span"
    );
    close(&a.t.app, &at, 15)
        .await
        .expect("Home leaves on the first press");
}
