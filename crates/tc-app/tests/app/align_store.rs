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
        let (backend, watch) = backend_over(&fs, APP, FakePlatform::default(), hooks.clone());
        let (store, _) = ScenarioStore::write(backend, state).unwrap();
        install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
        let mut t = start(dir.path(), Arc::new(store)).await;
        t.watch = Some(watch);
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
        let (store, watch) = backend_over(&fs, APP, FakePlatform::default(), hooks.clone());
        self.t = start(self.dir.path(), Arc::new(store)).await;
        self.t.watch = Some(watch);
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

/// Whether the project's last commit links πίστιν to "fe" in its alignments of Titus.
fn committed_link(rig: &Rig) -> bool {
    use tc_store::Vfs;
    let dir = rig
        .fs
        .outside_dir(&Path::new(PROJECTS).join(SAMPLE))
        .unwrap();
    let repo = git2::Repository::open(dir).unwrap();
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    let Ok(entry) = tree.get_path(Path::new("ingredients/checking/alignments/TIT.json")) else {
        return false;
    };
    let blob = repo.find_blob(entry.id()).unwrap();
    let text = std::str::from_utf8(blob.content()).unwrap();
    text.split(r#""word":"πίστιν"}],"bottomWords":["#)
        .nth(1)
        .and_then(|bottom| bottom.split(']').next())
        .is_some_and(|bottom| bottom.contains(r#""word":"fe""#))
}

/// INV-QUIT-SAVES-LINKS-THEN-CHECKPOINTS (D1, RT-4): a quit with the aligner's links still
/// unsaved saves them before the leave checkpoint, so the commit Door43 receives holds them,
/// and it is the leave checkpoint that commits them, whose failure the screens would show (the
/// store's own commit at close tells no one).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_quit_commit_holds_the_aligner_links() {
    let rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    assert!(!committed_link(&rig));
    let placed = link(&rig, &at, &view).await;
    assert_eq!(placed.save, AlignSave::Unsaved);
    drop(session);
    rig.t.app.quit().await;
    assert!(committed_link(&rig), "the quit's commit holds the link");
    assert_eq!(
        rig.t.app.sessions().checkpoint_state(&folder(SAMPLE)),
        tc_app::session::CheckpointState::Ended(tc_api::event::CheckpointOutcome::Committed),
        "the leave checkpoint made the commit"
    );
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
        Duration::from_secs(60),
        tokio::task::spawn_blocking(move || {
            hooks.wait_reached(Point::BeforeDurablePending, 1);
        }),
    )
    .await
    .expect("hung: the save reaching its hold")
    .unwrap();
    let newer = place(&rig.t.app, &at, &placed, 63, "verdad", "ἀληθείας").await;
    assert_eq!(newer.save, AlignSave::Saving);
    rig.hooks.release(Point::BeforeDurablePending);
    // Read the actor until its first save has replied; the newer links have a fresh timer.
    for () in crate::store_world::for_a_minute() {
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
    // The aligner hears the store's reply on its own task after the store is idle; wait for it,
    // bounded only to catch a hang (ruling 2026-10-23: behavior, not speed).
    let started = std::time::Instant::now();
    loop {
        let save = ready(&rig.t.app, &at, 1, 1).await.save;
        if save == AlignSave::Saved {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "hung: the aligner still shows {save:?} 60 s after the store went idle"
        );
        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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

/// INV-TEXT-DIRECTION-HAS-ONE-SOURCE (tC4 #832.1): a direction changed in Project settings is
/// the direction Home's card, the open project, Align and Settings show, in this app and in a
/// new app on the same folders: Settings writes the metadata field every reader reads first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_direction_changed_in_settings_shows_on_home_and_in_every_tool_after_restart() {
    use tc_api::align::{Open, Opened};
    use tc_api::home::{Card, HomeCard};
    use tc_api::settings::{Save, SettingsView, ShownValues, ViewFromHome};
    use tc_api::shown::Direction;
    let mut rig = Rig::new().await;
    let project = folder(SAMPLE);
    let settings = |t: &TestApp| {
        let (app, project) = (t.app.clone(), project.clone());
        async move {
            post::<ViewFromHome>(&app, &ViewFromHome { project })
                .await
                .unwrap()
        }
    };
    let view: SettingsView = settings(&rig.t).await;
    assert_eq!(view.direction, Direction::Ltr);
    post(
        &rig.t.app,
        &Save {
            request: request(230),
            project: project.clone(),
            shown: ShownValues {
                direction: view.direction,
                font: view.font.clone(),
            },
            direction: Direction::Rtl,
            font: view.font.clone().unwrap_or_else(|| "Noto Sans".to_owned()),
        },
    )
    .await
    .unwrap();
    for restarted in [false, true] {
        if restarted {
            rig.restart(false).await;
        }
        let where_ = if restarted {
            "after a restart"
        } else {
            "in this app"
        };
        let card: HomeCard = post(
            &rig.t.app,
            &Card {
                project: project.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(card.dir, Direction::Rtl, "Home's card {where_}");
        let opened = super::super::understand::opened(&rig.t.app).await;
        assert_eq!(opened.dir, Direction::Rtl, "the open project {where_}");
        let align = post(
            &rig.t.app,
            &Open {
                at: opened.session.at.clone(),
                request: request(231),
                book: BookId::Titus,
                verse: None,
            },
        )
        .await
        .unwrap();
        let Opened::Ready(align) = align else {
            panic!("{align:?}")
        };
        assert_eq!(align.target_dir, Direction::Rtl, "Align {where_}");
        post(
            &rig.t.app,
            &tc_api::session::Close {
                at: opened.session.at,
                request: request(232),
            },
        )
        .await
        .unwrap();
        rig.t.app.sessions().settled(&project).await;
        assert_eq!(
            settings(&rig.t).await.direction,
            Direction::Rtl,
            "Settings {where_}"
        );
    }
}

/// tC4 #832.2: a project imported from tC4 names no language in settings.json; Settings then
/// shows the name the metadata gives the language, not an empty field.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_imported_project_shows_its_metadata_language_name_in_settings() {
    use tc_api::settings::{SettingsView, ViewFromHome};
    let rig = Rig::new().await;
    let view: SettingsView = post(
        &rig.t.app,
        &ViewFromHome {
            project: folder(SAMPLE),
        },
    )
    .await
    .unwrap();
    assert_eq!(view.language_name, "Español (Latinoamérica)");
}

/// tC4 #795: a verse typed in decomposed (NFD) form, aligned and marked valid in the same
/// session, is still valid after a restart. The aligner and the stored alignment hash the same
/// text: the stored verse, in NFC.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen() {
    use tc_api::editor::SaveVerse;
    let mut rig = Rig::new().await;
    let session = rig.open().await;
    let at = session.scope().clone();
    let key = RecordKey::Verse(
        BookId::Titus,
        ChapterNo::new(1).unwrap(),
        VerseNo::new(1).unwrap(),
    );
    let token = session.read().mint(&key, None).unwrap();
    // "apóstol" with its accent as a combining mark, as some keyboards type it.
    let nfd = "Pablo, siervo de Dios y apo\u{301}stol de Jesucristo";
    post(
        &rig.t.app,
        &SaveVerse {
            at: at.clone(),
            request: request(240),
            shown: tc_app::wire::token::shown(&token).unwrap(),
            words: nfd.to_owned(),
        },
    )
    .await
    .unwrap();
    let view = ready(&rig.t.app, &at, 1, 1).await;
    let marked = edit(&rig.t.app, &at, &view, 241, AlignEdit::MarkValid)
        .await
        .unwrap();
    assert_eq!(marked.status.state, RailState::Valid);
    drop(session);
    rig.t.app.quit().await;
    rig.assert_burrito();

    rig.restart(false).await;
    let session = rig.open().await;
    let reopened = ready(&rig.t.app, session.scope(), 1, 1).await;
    assert_eq!(
        reopened.status.state,
        RailState::Valid,
        "the alignment made for the NFD-typed verse stays valid"
    );
    // The fold's own check (§7.7): the hash the alignment recorded is the stored verse's, so
    // the alignment files and the export keep it too.
    let read = session.read();
    let Some(Record::Verse {
        value: tc_journal::record::VerseValue::Text { words, layout, .. },
        ..
    }) = read.projected(&key)
    else {
        panic!("Titus 1:1 holds text")
    };
    let Some(Record::Alignment { value, .. }) = read.projected(&RecordKey::Alignment(
        BookId::Titus,
        ChapterNo::new(1).unwrap(),
        VerseNo::new(1).unwrap(),
    )) else {
        panic!("Titus 1:1 is aligned")
    };
    assert_eq!(
        value.target_verse_md5,
        tc_journal::decision::verse_text_md5_v1(&format!("{words}{layout}")),
        "one hash for the aligner and the alignment files"
    );
    drop(read);
    drop(session);
    rig.t.app.quit().await;
}
