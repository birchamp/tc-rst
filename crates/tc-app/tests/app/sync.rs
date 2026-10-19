//! Sync with Door43 from a Home card (unit B10), on the parity `shared` scenario written to the
//! store, with the fake Door43 on loopback for the sign-in: the internet question and the
//! sign-in come first, a sync with no scripted outcome is not available until unit X, and a
//! scripted sync's teammates' changes reach the fold, so the toast, the card's chip and the
//! Titus tile all count the one verse conflict tC4's `sync-ok-teammates` shows.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tc_api::ApiError;
use tc_api::account::SignIn;
use tc_api::door43::SyncProject;
use tc_api::endpoint::NetKind;
use tc_api::error::{Feature, SignInReason};
use tc_api::home::{BookTileView, Card, HomeCard, List};
use tc_api::ids::{JobId, RequestId};
use tc_api::jobs::{JobOutcome, JobState, SyncResultView};
use tc_api::net::Set;
use tc_api::refusal::{Refusal, SyncFailure, SyncRefusal};
use tc_api::shown::{Known, PersonLabel, Who};
use tc_api::test::{Script, Scripted, ScriptedSync, TeammatesEdit};
use tc_app::App;
use tc_app::StoreChoice;
use tc_app::backend::scenario::install_resources_into;
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::config::ProjectsFolder;
use tc_app::sync::ConflictTally;
use tc_core::BookId;
use tc_door43_fake::FakeDoor43;
use tc_door43_fake::seeded::{PARITY_PASSWORD, PARITY_USER};
use tc_journal::fold::{ConflictCounts, ConflictKind};
use tc_store::FakePlatform;
use tc_store::fault::FaultFs;

use super::common::{Parts, TestApp, folder, post, rid, until};
use super::scenario::scenario;
use crate::store_world::{APP, backend_over};

const SAMPLE: &str = "sample_burrito";

struct World {
    t: TestApp,
    _fake: FakeDoor43,
    _dir: tempfile::TempDir,
}

/// The `shared` scenario on the store, its sideloaded resources installed as `tc4 serve
/// --scenario` installs them, and the app's Door43 the fake.
async fn shared() -> World {
    let fake = FakeDoor43::start().await.unwrap();
    tc_door43_fake::seeded::parity_account(&fake).await.unwrap();
    let fs = FaultFs::new();
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let state = scenario("shared");
    let (store, _) = ScenarioStore::write(backend, state).unwrap();
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let t = app_over(dir.path(), Arc::new(store), fake.url()).await;
    World {
        t,
        _fake: fake,
        _dir: dir,
    }
}

async fn app_over(dir: &Path, store: Arc<ScenarioStore>, door43: &str) -> TestApp {
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let url = door43.to_owned();
    TestApp::start_with(dir, &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        config.door43 = url.parse().unwrap();
        deps.store = StoreChoice::Given(store);
    })
    .await
}

fn sync(n: u8) -> SyncProject {
    SyncProject {
        request: rid(n),
        project: folder(SAMPLE),
    }
}

fn script(app: &App, n: u8, outcome: ScriptedSync) -> impl Future<Output = ()> {
    let app = app.clone();
    async move {
        post(
            &app,
            &Script {
                request: rid(n),
                next: Scripted::Sync {
                    project: folder(SAMPLE),
                    outcome,
                },
            },
        )
        .await
        .unwrap();
    }
}

async fn internet_on(app: &App) {
    post(
        app,
        &Set {
            request: RequestId::from_bytes([0xB1; 16]),
            on: true,
        },
    )
    .await
    .unwrap();
}

async fn sign_in(app: &App) {
    post(
        app,
        &SignIn {
            request: RequestId::from_bytes([0xB2; 16]),
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
    until("the sync ends", || {
        state = app.jobs().view(job).map(|v| v.state);
        matches!(
            state,
            Some(JobState::Done(_) | JobState::Failed(_) | JobState::Cancelled)
        )
    })
    .await;
    state.unwrap()
}

async fn synced(app: &App, n: u8) -> SyncResultView {
    let job = post(app, &sync(n)).await.unwrap();
    match ended(app, job).await {
        JobState::Done(JobOutcome::Synced(view)) => view,
        other => panic!("{other:?}"),
    }
}

/// The card once its summary is computed, after the sync told Home it changed.
async fn card_with_counts(app: &App) -> HomeCard {
    for _ in 0..1000 {
        let card: HomeCard = post(
            app,
            &Card {
                project: folder(SAMPLE),
            },
        )
        .await
        .unwrap();
        if card.conflicts != Known::Unknown {
            return card;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the card's summary never came");
}

fn teammate(n: u16) -> PersonLabel {
    PersonLabel {
        who: Who::Teammate(n),
        computer: None,
    }
}

fn titus_tile(card: &HomeCard) -> BookTileView {
    let Known::Known(books) = &card.books else {
        panic!("{card:?}")
    };
    books
        .iter()
        .find(|b| b.book == BookId::Titus)
        .unwrap()
        .clone()
}

const TWO_TEAMMATES: fn() -> ScriptedSync = || ScriptedSync::Synced {
    teammates: Some(TeammatesEdit {
        book: BookId::Titus,
        chapter: 1,
        verse: 1,
        texts: vec![
            "Pablo, versión del compañero 1.".into(),
            "Pablo, versión del compañero 2.".into(),
        ],
    }),
    restored: 0,
};

/// tC4 #678: only the press syncs, and it asks the internet question, then the sign-in, before
/// anything is sent; with no outcome scripted, the sync is unit X's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sync_asks_the_internet_then_a_sign_in_and_until_unit_x_is_not_available() {
    let w = shared().await;
    let app = &w.t.app;
    assert_eq!(
        post(app, &sync(1)).await,
        Err(ApiError::NeedsInternet(NetKind::Sync))
    );
    internet_on(app).await;
    assert_eq!(
        post(app, &sync(2)).await,
        Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn))
    );
    sign_in(app).await;
    let job = post(app, &sync(3)).await.unwrap();
    assert_eq!(
        ended(app, job).await,
        JobState::Failed(ApiError::Unavailable(Feature::Sync))
    );
}

/// tC4's `sync-ok` and `sync-ok-teammates`: a sync with no teammate change counts nothing, and
/// one that brings two teammates' versions of Titus 1:1 counts two changes from Teammate 1 and
/// Teammate 2 and one verse conflict, which the card's chip and the Titus tile then show from
/// the fold's own conflicts (X12). The note on missing versions counts tC4's 5.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn teammates_a_sync_receives_stand_as_one_verse_conflict_on_the_toast_the_chip_and_the_tile()
{
    let w = shared().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    let _: tc_api::home::HomeView = post(app, &List {}).await.unwrap();
    assert_eq!(card_with_counts(app).await.conflicts, Known::Known(0));

    script(
        app,
        1,
        ScriptedSync::Synced {
            teammates: None,
            restored: 0,
        },
    )
    .await;
    let nothing = synced(app, 2).await;
    assert_eq!(
        nothing,
        SyncResultView {
            received: 0,
            from: Vec::new(),
            kept_newest: 0,
            verse_conflicts: 0,
            alignments: 0,
            restored: 0,
            missing_pins: 5,
        }
    );

    script(app, 3, TWO_TEAMMATES()).await;
    let view = synced(app, 4).await;
    assert_eq!(
        view,
        SyncResultView {
            received: 2,
            from: vec![teammate(1), teammate(2)],
            kept_newest: 0,
            verse_conflicts: 1,
            alignments: 0,
            restored: 0,
            missing_pins: 5,
        }
    );
    let card = card_with_counts(app).await;
    assert_eq!(card.conflicts, Known::Known(1));
    assert_eq!(titus_tile(&card).conflicts, 1);
}

/// tC4's `sync-ok-restored`: a second sync after Door43 lost a teammate's change receives
/// nothing, sends the change again, and still names the conflict the first sync brought.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_sync_sends_again_what_door43_lost_and_still_names_the_conflict() {
    let w = shared().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    script(app, 1, TWO_TEAMMATES()).await;
    let _ = synced(app, 2).await;
    script(
        app,
        3,
        ScriptedSync::Synced {
            teammates: None,
            restored: 1,
        },
    )
    .await;
    let again = synced(app, 4).await;
    assert_eq!(
        (again.received, again.restored, again.verse_conflicts),
        (0, 1, 1)
    );
    assert!(again.from.is_empty());
}

/// tC4's `sync-failed`: a sync whose clone fails ends with its refusal and changes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_sync_ends_with_its_refusal_and_changes_nothing() {
    let w = shared().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    let failure = SyncFailure::BeforeIntake(SyncRefusal::Unreachable);
    script(app, 1, ScriptedSync::Failed(failure.clone())).await;
    let job = post(app, &sync(2)).await.unwrap();
    assert_eq!(
        ended(app, job).await,
        JobState::Failed(ApiError::Refused(Refusal::Sync(failure)))
    );
    let _: tc_api::home::HomeView = post(app, &List {}).await.unwrap();
    assert_eq!(card_with_counts(app).await.conflicts, Known::Known(0));
}

/// #791: verses with two alignments are counted apart from verse conflicts and from the kinds
/// that show their newest version, from the fold's counts by kind.
#[test]
fn the_toast_counts_verse_conflicts_alignments_and_the_rest_apart() {
    let mut counts = ConflictCounts::new();
    counts.insert((Some(BookId::Titus), ConflictKind::VerseText.order()), 2);
    counts.insert((Some(BookId::Titus), ConflictKind::Passage.order()), 1);
    counts.insert(
        (Some(BookId::Jonah), ConflictKind::StructureIssue.order()),
        1,
    );
    counts.insert((Some(BookId::Titus), ConflictKind::Alignment.order()), 3);
    counts.insert((Some(BookId::Titus), ConflictKind::Check.order()), 1);
    counts.insert((None, ConflictKind::Setting.order()), 1);
    assert_eq!(
        ConflictTally::of(&counts),
        ConflictTally {
            verse: 4,
            alignments: 3,
            others: 2,
        }
    );
}
