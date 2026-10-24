//! C5's links through the real store: quit flushes them before the checkpoint, and a
//! crash or a failed final save leaves the actor's newest links in the next open's offer.

use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tc_api::align::{AlignEdit, AlignSave, AlignerView, RailState};
use tc_api::ids::Scope;
use tc_api::session::{DraftKind, DraftOffer, Drafts, KeepDraft, OfferedDrafts};
use tc_app::StoreChoice;
use tc_app::backend::ProjectStore;
use tc_app::backend::scenario::install_resources_into;
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::config::ProjectsFolder;
use tc_app::session::ProjectSession;
use tc_journal::command::AlignmentEdit;
use tc_journal::{BookId, ChapterNo, Record, RecordKey, VerseNo};
use tc_store::fault::FaultFs;
use tc_store::{FakePlatform, Hooks, Point};

use super::super::common::{Parts, TestApp, folder, post};
use super::super::scenario::scenario;
use super::{SAMPLE, edit, place, ready, request};
use crate::store_world::{APP, PROJECTS, START_MS, assert_valid_burrito, backend_over, progress};

struct Rig {
    dir: tempfile::TempDir,
    fs: FaultFs,
    hooks: Arc<Hooks>,
    t: TestApp,
}

impl Rig {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let fs = FaultFs::new();
        let hooks = Arc::new(Hooks::default());
        let state = scenario("sample");
        let backend = backend_over(&fs, APP, FakePlatform::default(), hooks.clone());
        let (store, _) = ScenarioStore::write(backend, state).unwrap();
        install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
        let t = start(dir.path(), Arc::new(store)).await;
        Self { dir, fs, hooks, t }
    }

    async fn open(&self) -> Arc<ProjectSession> {
        let opened = self
            .t
            .app
            .sessions()
            .open_screen(&folder(SAMPLE), progress())
            .await
            .unwrap();
        let session = self.t.app.sessions().reader(&opened.at).unwrap();
        session.handle().wait_idle();
        session
    }

    async fn restart(&mut self, power_cut: bool) {
        let fs = self.fs.crash(power_cut);
        let hooks = Arc::new(Hooks::default());
        let store = backend_over(&fs, APP, FakePlatform::default(), hooks.clone());
        self.t = start(self.dir.path(), Arc::new(store)).await;
        self.fs = fs;
        self.hooks = hooks;
    }

    async fn offers(&self, at: &Scope) -> OfferedDrafts {
        post(
            &self.t.app,
            &Drafts {
                at: at.clone(),
                request: request(200),
                buffered: Vec::new(),
                mirror: true,
            },
        )
        .await
        .unwrap()
    }

    async fn keep(&self, at: &Scope, offer: &DraftOffer) {
        post(
            &self.t.app,
            &KeepDraft {
                at: at.clone(),
                request: request(201),
                kind: offer.kind,
                shown: offer.shown.clone(),
                text: offer.text.clone(),
                discard: offer.settles.stored.clone(),
            },
        )
        .await
        .unwrap();
    }

    fn assert_burrito(&self) {
        assert_valid_burrito(&self.fs, &Path::new(PROJECTS).join(SAMPLE));
    }
}

async fn start(dir: &Path, store: Arc<dyn ProjectStore>) -> TestApp {
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let parts = Parts::new();
    parts.clock.set(START_MS);
    TestApp::start_with(dir, &parts, |config, deps| {
        config.projects = ProjectsFolder::Given(projects);
        deps.store = StoreChoice::Given(store);
    })
    .await
}

fn stored(session: &ProjectSession) -> AlignmentEdit {
    let key = RecordKey::Alignment(
        BookId::Titus,
        ChapterNo::new(1).unwrap(),
        VerseNo::new(1).unwrap(),
    );
    let Some(Record::Alignment { value, .. }) = session.read().projected(&key) else {
        panic!("Titus 1:1 lost its alignment");
    };
    AlignmentEdit {
        alignments: value.alignments,
        word_bank: value.word_bank,
        done: value.done,
    }
}

fn has_link(edit: &AlignmentEdit, source: &str, target: &str) -> bool {
    edit.alignments.iter().any(|group| {
        group.top_words.iter().any(|word| word.word == source)
            && group.bottom_words.iter().any(|word| word.word == target)
    })
}

fn only_alignment(offers: &OfferedDrafts) -> &DraftOffer {
    assert_eq!(offers.offers.len(), 1, "{offers:#?}");
    let offer = &offers.offers[0];
    assert_eq!(offer.kind, DraftKind::Alignment);
    assert_eq!(
        offer.shown.len(),
        2,
        "the alignment and the displayed verse"
    );
    offer
}

async fn link(rig: &Rig, at: &Scope, view: &AlignerView) -> AlignerView {
    place(&rig.t.app, at, view, 61, "fe", "πίστιν").await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn links_saved_when_quitting_within_2_s() {
    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    let placed = link(&rig, &at, &view).await;
    assert_eq!(placed.save, AlignSave::Unsaved);
    assert!(
        !has_link(&stored(&session), "πίστιν", "fe"),
        "the timer has not saved it"
    );
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();

    rig.restart(false).await;
    let session = rig.open().await;
    assert!(has_link(&stored(&session), "πίστιν", "fe"));
    let reopened = ready(&rig.t.app, session.scope(), 1, 1).await;
    assert_eq!(reopened.cards, placed.cards);
    assert!(
        rig.offers(session.scope()).await.offers.is_empty(),
        "saved links need no offer"
    );
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crash_offers_the_aligners_links_with_both_displayed_tokens() {
    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    link(&rig, &at, &view).await;
    session.handle().wait_idle();
    assert!(
        !has_link(&stored(&session), "πίστιν", "fe"),
        "still a draft, not a save"
    );
    drop(session);

    rig.restart(true).await;
    let session = rig.open().await;
    let offers = rig.offers(session.scope()).await;
    let offer = only_alignment(&offers);
    assert!(has_link(
        &AlignmentEdit::from_json(&offer.text).unwrap(),
        "πίστιν",
        "fe"
    ));
    rig.keep(session.scope(), offer).await;
    assert!(has_link(&stored(&session), "πίστιν", "fe"));
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_save_ack_restores_the_draft_of_links_changed_while_it_waited() {
    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    rig.hooks.hold(Point::BeforeDurablePending);
    let placed = link(&rig, &at, &view).await;
    let hooks = rig.hooks.clone();
    tokio::time::timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            hooks.wait_reached(Point::BeforeDurablePending, 1);
        }),
    )
    .await
    .unwrap()
    .unwrap();
    let newer = place(&rig.t.app, &at, &placed, 63, "verdad", "ἀληθείας").await;
    assert_eq!(newer.save, AlignSave::Saving);
    rig.hooks.release(Point::BeforeDurablePending);
    // Read the actor until its first save has replied; the newer links have a fresh timer.
    for _ in 0..1000 {
        if ready(&rig.t.app, &at, 1, 1).await.save == AlignSave::Unsaved {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(ready(&rig.t.app, &at, 1, 1).await.save, AlignSave::Unsaved);
    session.handle().wait_idle();
    let written = stored(&session);
    assert!(has_link(&written, "πίστιν", "fe"));
    assert!(
        !has_link(&written, "ἀληθείας", "verdad"),
        "the second save has not run"
    );
    drop(session);

    rig.restart(true).await;
    let session = rig.open().await;
    let offers = rig.offers(session.scope()).await;
    let offer = only_alignment(&offers);
    let recovered = AlignmentEdit::from_json(&offer.text).unwrap();
    assert!(has_link(&recovered, "πίστιν", "fe"));
    assert!(has_link(&recovered, "ἀληθείας", "verdad"));
    rig.keep(session.scope(), offer).await;
    assert_eq!(stored(&session), recovered);
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_final_quit_save_keeps_the_last_durable_links_draft() {
    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    link(&rig, &at, &view).await;
    session.handle().wait_idle();
    assert!(!has_link(&stored(&session), "πίστιν", "fe"));
    rig.fs.fail_writes(Some(ErrorKind::StorageFull));
    drop(session);
    rig.t.app.quit().await;
    rig.fs.fail_writes(None);

    rig.restart(false).await;
    let session = rig.open().await;
    assert!(
        !has_link(&stored(&session), "πίστιν", "fe"),
        "the final save failed"
    );
    let offers = rig.offers(session.scope()).await;
    let offer = only_alignment(&offers);
    assert!(has_link(
        &AlignmentEdit::from_json(&offer.text).unwrap(),
        "πίστιν",
        "fe"
    ));
    rig.keep(session.scope(), offer).await;
    assert!(has_link(&stored(&session), "πίστιν", "fe"));
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();
}

/// The timer owns the durable save: ending the process after it fires needs neither a
/// navigation flush nor the quit path to preserve either of the verse's new links.
#[tokio::test]
async fn the_timer_saves_links_durably_two_seconds_after_the_last_change() {
    use tc_api::event::{Event, SessionEvent};
    use tc_app::events::Next;

    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    let first = link(&rig, &at, &view).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_millis(1500)).await;
    assert!(!has_link(&stored(&session), "πίστιν", "fe"));
    let more = place(&rig.t.app, &at, &first, 63, "verdad", "ἀληθείας").await;
    tokio::time::advance(Duration::from_millis(1500)).await;
    assert_eq!(ready(&rig.t.app, &at, 1, 1).await.save, AlignSave::Unsaved);
    assert!(
        !has_link(&stored(&session), "πίστιν", "fe"),
        "the second change restarted the timer"
    );
    let mut events = rig.t.app.events().subscribe(rig.t.app.events().head());
    tokio::time::advance(Duration::from_millis(500)).await;
    // Advancing the clock does not order the actor's due timer before another request.
    // Wait for its announcement; the paused clock also handles timer-wheel rounding. The
    // limit only catches a hang (rulings/2026-10-23-tests-prove-behavior-not-speed.md).
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let Next::Frame(frame) = events.next().await
                && let Event::Session {
                    at: scope,
                    event: SessionEvent::AlignSave { aligner, state },
                } = frame.event
                && scope == at
                && aligner == more.aligner
                && state == AlignSave::Saving
            {
                break;
            }
        }
    })
    .await
    .expect("the due timer announces its save");
    // Real fsync/relay work is not simulated by advancing tokio's clock.
    session.handle().wait_idle();
    assert_eq!(ready(&rig.t.app, &at, 1, 1).await.save, AlignSave::Saved);
    assert!(has_link(&stored(&session), "πίστιν", "fe"));
    assert!(has_link(&stored(&session), "ἀληθείας", "verdad"));
    tokio::time::resume();
    rig.assert_burrito();
    drop(session);

    rig.restart(true).await;
    let session = rig.open().await;
    assert!(has_link(&stored(&session), "πίστιν", "fe"));
    assert!(has_link(&stored(&session), "ἀληθείας", "verdad"));
    let reopened = ready(&rig.t.app, session.scope(), 1, 1).await;
    assert_eq!(reopened.cards, more.cards);
    assert!(rig.offers(session.scope()).await.offers.is_empty());
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changing_verse_flushes_the_previous_verses_links_to_the_store() {
    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    let placed = link(&rig, &at, &view).await;
    assert_eq!(placed.save, AlignSave::Unsaved);
    assert!(!has_link(&stored(&session), "πίστιν", "fe"));
    let next = ready(&rig.t.app, &at, 1, 2).await;
    assert_eq!(next.place.label(), "1:2");
    assert!(
        has_link(&stored(&session), "πίστιν", "fe"),
        "the previous verse saved before the next opened"
    );
    session.handle().wait_idle();
    rig.assert_burrito();
    drop(session);

    rig.restart(true).await;
    let session = rig.open().await;
    assert!(has_link(&stored(&session), "πίστιν", "fe"));
    assert_eq!(
        ready(&rig.t.app, session.scope(), 1, 1).await.cards,
        placed.cards
    );
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mark_valid_survives_quit_and_reopen_on_the_store() {
    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    assert!(!view.done);
    let marked = edit(&rig.t.app, &at, &view, 80, AlignEdit::MarkValid)
        .await
        .unwrap();
    assert!(marked.done);
    assert_eq!(marked.status.state, RailState::Valid);
    assert_eq!(marked.save, AlignSave::Unsaved);
    assert!(
        !stored(&session).done,
        "Mark valid is still waiting for its save"
    );
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();

    rig.restart(false).await;
    let session = rig.open().await;
    assert!(stored(&session).done);
    let reopened = ready(&rig.t.app, session.scope(), 1, 1).await;
    assert!(reopened.done);
    assert_eq!(reopened.status.state, RailState::Valid);
    assert_eq!(reopened.save, AlignSave::Saved);
    assert!(rig.offers(session.scope()).await.offers.is_empty());
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();
}
