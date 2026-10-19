//! The contract cases of APP-DESIGN §4.2 that run through tc-app (unit C0): the request bytes
//! go through `App::call`, which takes and gives exactly what tc-server forwards, then
//! dispatch, the session's reply cache, the seam and the store. `test/saveVerse` is the write;
//! C2 moves these cases onto `editor/saveVerse` and runs the conflict case through HTTP too.

use std::sync::Arc;
use std::time::Duration;

use tc_api::app::OpenSession;
use tc_api::event::{ChangeCause, ChangeScope, ChapterRef, Event, SessionEvent};
use tc_api::ids::RequestId;
use tc_api::test::SaveVerse;
use tc_api::write::{Ack, Stored};
use tc_api::{ApiError, Endpoint, NotSaved, Reply};
use tc_app::Answer;
use tc_app::config::ProjectsFolder;
use tc_app::events::Next;
use tc_app::session::ProjectSession;
use tc_app::wire::token::{review, shown};
use tc_journal::{BookId, ChangeId, RecordKey};
use tc_store::Point;

use crate::common::{Parts, TestApp, answer, rid, until};
use crate::counting_store::{CountingStore, SharedStore};
use crate::store_world::{StoreWorld, progress, store_world, tit, words_of};

/// An app over the world's store, with test hooks on.
struct Rig {
    world: StoreWorld,
    t: TestApp,
    _dir: tempfile::TempDir,
}

impl Rig {
    async fn new() -> Rig {
        let world = store_world();
        let store = Arc::new(CountingStore::new(SharedStore(world.store())));
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        std::fs::create_dir(&projects).unwrap();
        let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
            config.projects = ProjectsFolder::Given(projects.clone());
            deps.store = store.clone();
        })
        .await;
        Rig {
            world,
            t,
            _dir: dir,
        }
    }

    async fn open(&self, folder: &tc_api::ids::ProjectRef) -> OpenSession {
        let open = self
            .t
            .app
            .sessions()
            .open_screen(folder, progress())
            .await
            .unwrap();
        self.session(&open).handle().wait_idle();
        open
    }

    fn session(&self, open: &OpenSession) -> Arc<ProjectSession> {
        self.t.app.sessions().reader(&open.at).unwrap()
    }

    /// A save of `words` over `key` as the screen's snapshot shows it.
    fn request(&self, open: &OpenSession, id: u8, key: &RecordKey, words: &str) -> SaveVerse {
        let minted = self.session(open).read().mint(key, None).unwrap();
        SaveVerse {
            at: open.at.clone(),
            request: rid(id),
            shown: shown(&minted).unwrap(),
            words: words.to_owned(),
        }
    }

    async fn send(&self, req: &SaveVerse) -> Answer {
        answer(&self.t.app, req).await
    }
}

fn body_of(answer: &Answer) -> Vec<u8> {
    match answer {
        Answer::Reply { body, .. } => body.clone(),
        other => panic!("{other:?}"),
    }
}

fn result_of(answer: &Answer) -> Result<Ack, ApiError> {
    serde_json::from_slice::<Reply<Ack>>(&body_of(answer))
        .unwrap()
        .into_result()
}

fn heads(session: &ProjectSession, key: &RecordKey) -> Vec<ChangeId> {
    let mut heads = session.handle().snapshot().fold().heads(key);
    heads.sort();
    heads
}

/// AD4: a retry after a lost reply gets the first attempt's reply and runs nothing again.
#[tokio::test]
async fn retry_after_lost_reply_is_idempotent() {
    let rig = Rig::new().await;
    let open = rig.open(&rig.world.bible).await;
    let session = rig.session(&open);
    let key = tit(1, 4);
    let before = (session.handle().counters(), heads(&session, &key));
    let history = session.handle().snapshot().fold().history(&key).len();
    let req = rig.request(&open, 1, &key, "a word in season");

    rig.world.hooks.hold(Point::BeforeDurablePending);
    let first = {
        let app = rig.t.app.clone();
        let body = serde_json::to_vec(&req).unwrap();
        tokio::spawn(async move { app.call(SaveVerse::PATH, &body).await })
    };
    until("the save reaches the durability point", || {
        rig.world.hooks.reached(Point::BeforeDurablePending) == 1
    })
    .await;
    // The caller went away; the write goes on.
    first.abort();
    rig.world.hooks.release(Point::BeforeDurablePending);

    let retried = rig.send(&req).await;
    let ack = result_of(&retried).unwrap();
    assert_eq!(ack.stored, Stored::Written);
    assert_eq!(ack.saved[0].text, "a word in season");
    let again = rig.send(&req).await;
    assert_eq!(body_of(&again), body_of(&retried), "byte for byte");

    let spent = session.handle().counters().since(&before.0);
    assert_eq!(spent.commands, 1, "the write ran once");
    let after = heads(&session, &key);
    assert_eq!(after.len(), 1);
    assert!(!before.1.contains(&after[0]), "a new change");
    assert_eq!(
        session.handle().snapshot().fold().history(&key).len(),
        history + 1,
        "exactly one"
    );
}

/// AD4, A-2: a retry while the first attempt waits on the actor waits for it, and both get
/// one reply; the write runs once.
#[tokio::test]
async fn retry_while_first_attempt_waits_runs_once() {
    let rig = Rig::new().await;
    let open = rig.open(&rig.world.bible).await;
    let session = rig.session(&open);
    let key = tit(1, 4);
    let before = (session.handle().counters(), heads(&session, &key));
    let req = rig.request(&open, 2, &key, "waiting its turn");

    rig.world.hooks.hold(Point::BeforeDurablePending);
    let call = |req: SaveVerse| {
        let app = rig.t.app.clone();
        let body = serde_json::to_vec(&req).unwrap();
        tokio::spawn(async move { app.call(SaveVerse::PATH, &body).await })
    };
    let first = call(req.clone());
    until("the save reaches the durability point", || {
        rig.world.hooks.reached(Point::BeforeDurablePending) == 1
    })
    .await;
    let second = call(req.clone());
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!first.is_finished() && !second.is_finished(), "both wait");
    rig.world.hooks.release(Point::BeforeDurablePending);
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(body_of(&first), body_of(&second));
    assert_eq!(result_of(&first).unwrap().stored, Stored::Written);
    assert_eq!(session.handle().counters().since(&before.0).commands, 1);
    let after = heads(&session, &key);
    assert_eq!(after.len(), 1);
    assert!(!before.1.contains(&after[0]));
}

/// AD4, A-2: an id that comes back with another body is a bug, refused; the first words
/// stand.
#[tokio::test]
async fn retry_with_another_body_is_request_reused() {
    let rig = Rig::new().await;
    let open = rig.open(&rig.world.bible).await;
    let session = rig.session(&open);
    let key = tit(1, 4);
    let counters = session.handle().counters();
    let req = rig.request(&open, 3, &key, "the first words");
    assert_eq!(
        result_of(&rig.send(&req).await).unwrap().stored,
        Stored::Written
    );
    let other = SaveVerse {
        words: "other words".to_owned(),
        ..req
    };
    assert_eq!(
        result_of(&rig.send(&other).await).unwrap_err(),
        ApiError::RequestReused
    );
    assert_eq!(session.handle().counters().since(&counters).commands, 1);
    assert_eq!(
        words_of(&session.handle().snapshot(), &key).as_deref(),
        Some("the first words")
    );
}

/// AD3, I4 (A-7): a save made from an edit token of the displayed head of a verse in
/// conflict keeps the conflict: the other head stays a head, the superseded one stays in the
/// history. A review token relabelled as an edit token through JSON is refused. The token
/// makes its trip through JSON text exactly as through HTTP, since `App::call` takes and
/// gives the bytes tc-server forwards.
#[tokio::test]
async fn edit_of_a_verse_in_conflict_keeps_the_conflict() {
    let rig = Rig::new().await;
    let (folder, key) = rig.world.conflicted.clone();
    let open = rig.open(&folder).await;
    let session = rig.session(&open);
    let read = session.read();
    let before = heads(&session, &key);
    assert_eq!(before.len(), 2);
    let displayed = read.mint(&key, None).unwrap().heads()[0];
    let other = *before.iter().find(|h| **h != displayed).unwrap();
    let other_words = match session
        .handle()
        .snapshot()
        .fold()
        .body(&other)
        .unwrap()
        .record
    {
        tc_journal::Record::Verse {
            value: tc_journal::record::VerseValue::Text { words, .. },
            ..
        } => words,
        record => panic!("{record:?}"),
    };

    let req = rig.request(&open, 4, &key, "a third text");
    let ack = result_of(&rig.send(&req).await).unwrap();
    assert_eq!(ack.stored, Stored::Written);
    assert_eq!(ack.new_conflicts.len(), 1, "{ack:?}");
    let after = heads(&session, &key);
    assert_eq!(after.len(), 2);
    assert!(
        after.contains(&other),
        "the head the person did not edit stays"
    );
    assert!(
        !after.contains(&displayed),
        "the displayed head is superseded"
    );
    let fold = session.handle().snapshot();
    assert!(
        fold.fold().body(&displayed).is_some(),
        "and kept in history"
    );
    match fold.fold().body(&other).unwrap().record {
        tc_journal::Record::Verse {
            value: tc_journal::record::VerseValue::Text { words, .. },
            ..
        } => assert_eq!(words, other_words),
        record => panic!("{record:?}"),
    }

    // A review token relabelled `st1.` is refused before any command is built.
    let rt = review(&session.read().mint_review(&key).unwrap()).unwrap();
    let relabelled: tc_api::ShownToken =
        serde_json::from_str(&format!("\"st1.{}\"", rt.body())).unwrap();
    let bad = SaveVerse {
        request: rid(5),
        shown: relabelled,
        ..req
    };
    assert_eq!(
        result_of(&rig.send(&bad).await).unwrap_err(),
        ApiError::NotSaved(NotSaved::BadToken)
    );
    assert_eq!(heads(&session, &key), after);
}

/// AD37, §5.4: the `Changed` event of a person's own save names their request, its chapter
/// and the view the `Ack` gives.
#[tokio::test]
async fn a_save_through_the_app_is_announced_as_its_own_request() {
    let rig = Rig::new().await;
    let open = rig.open(&rig.world.bible).await;
    let bus = rig.t.app.events();
    let mut events = bus.subscribe(bus.head());
    let request: RequestId = rid(6);
    let req = rig.request(&open, 6, &tit(1, 6), "announced as mine");
    let ack = result_of(&rig.send(&req).await).unwrap();
    loop {
        let next = tokio::time::timeout(Duration::from_secs(5), events.next())
            .await
            .expect("the event comes");
        let Next::Frame(frame) = next else {
            panic!("{next:?}")
        };
        let Event::Session { at, event } = frame.event else {
            continue;
        };
        assert_eq!(at, open.at);
        let SessionEvent::Changed { view, scope, cause } = event else {
            continue;
        };
        assert_eq!(cause, ChangeCause::LocalSave { request });
        assert_eq!(view, ack.view);
        let ChangeScope::Parts(parts) = scope else {
            panic!("{scope:?}")
        };
        assert!(parts.chapters.contains(&ChapterRef {
            book: BookId::Titus,
            chapter: 1
        }));
        break;
    }
}
