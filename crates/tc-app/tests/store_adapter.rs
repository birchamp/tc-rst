//! The store adapter's own tests (unit C0, APP-DESIGN §4.2): what `StoreBackend` adds to
//! tc-store (every fold step reaches the seam with who asked for it, events that end with the
//! project, a close that lets go of the lock, a listing that hides no folder, exact names,
//! forwarded open progress, drafts beside the actor), a property test over random command
//! sequences, a crash at every write point of a save, and a count test that the adapter adds
//! no store work.
// Tests read whole folds to check results; the lint on whole-fold walks guards the product's
// paths (crates/tc-app/clippy.toml).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

#[path = "app/common.rs"]
mod common;
#[path = "support/store_world.rs"]
mod store_world;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use proptest::prelude::*;
use tc_api::event::{ChangeCause, CheckpointOutcome, Event, SessionEvent};
use tc_api::jobs::{OpenPhase, Phase};
use tc_app::StoreChoice;
use tc_app::backend::{
    CheckpointReason, CommandError, EditToken, OpenFailure, Origin, ProjectHandle, ProjectStore,
    StoreCause, StoreCommand, StoreEvent, StoreReply, Stored, UnusableReason,
};
use tc_app::config::ProjectsFolder;
use tc_app::events::Next;
use tc_app::jobs::ProgressSink;
use tc_app::wire::token::{edit_token, shown};
use tc_journal::command::{Command, DraftField};
use tc_journal::fold::Counters;
use tc_journal::{BookId, ChangeId, RecordKey};
use tc_store::{FakePlatform, Point, Vfs};

use common::{Parts, TestApp, rid};
use store_world::{
    APP, PROJECTS, StoreWorld, assert_valid_burrito, backend_over, folder, progress, save,
    store_world, tit, token, words_of,
};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
}

/// The store events a subscriber gets until it holds the batch of `last`, or `timeout`.
async fn committed_until(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<StoreEvent>,
    last: u64,
) -> Vec<(u64, tc_journal::fold::Delta, StoreCause)> {
    let mut out = Vec::new();
    while out.last().is_none_or(|(seq, _, _)| *seq < last) {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("the event comes")
            .expect("the channel is open");
        if let StoreEvent::Committed { seq, delta, cause } = event {
            out.push((seq, delta, cause));
        }
    }
    out
}

// ---- What the adapter adds ----------------------------------------------------------------

/// Every fold step S1 makes after an open is a command's or a second writer's, and each
/// reaches the seam as a `Committed`: the seqs a subscriber gets are consecutive and end at
/// the newest snapshot's, so a step without an event would be a gap. This is why C0 builds no
/// frontier check.
#[tokio::test]
async fn every_fold_step_reaches_the_seam() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let first = handle.snapshot().seq();
    let mut events = handle.subscribe();
    let mut by_seq = BTreeMap::new();
    for v in [1, 2] {
        let reply = save(handle.as_ref(), &tit(1, v), "one at a time")
            .await
            .unwrap();
        by_seq.insert(reply.seq, tit(1, v));
    }
    // Two at once, neither awaited before the other is submitted.
    let a = handle.submit(StoreCommand::save_verse(
        token(handle.as_ref(), &tit(1, 3)),
        "together, first".to_owned(),
    ));
    let b = handle.submit(StoreCommand::save_verse(
        token(handle.as_ref(), &tit(1, 4)),
        "together, second".to_owned(),
    ));
    let (a, b) = tokio::join!(a, b);
    by_seq.insert(a.unwrap().seq, tit(1, 3));
    by_seq.insert(b.unwrap().seq, tit(1, 4));
    let reply = save(handle.as_ref(), &tit(1, 5), "the last").await.unwrap();
    by_seq.insert(reply.seq, tit(1, 5));

    let last = handle.snapshot().seq();
    let got = committed_until(&mut events, last).await;
    let seqs: Vec<u64> = got.iter().map(|(seq, _, _)| *seq).collect();
    assert_eq!(seqs, ((first + 1)..=last).collect::<Vec<_>>());
    for (seq, delta, _) in &got {
        assert!(delta.keys.contains(&by_seq[seq]), "{seq}: {:?}", delta.keys);
    }
}

/// AD37: a command `requested_by` a request is announced with that origin, one without as the
/// server's; the pump makes them `LocalSave { request }` and `System`.
#[tokio::test]
async fn a_command_is_announced_with_its_origin() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let mut events = handle.subscribe();
    let asked = handle
        .submit(
            StoreCommand::save_verse(token(handle.as_ref(), &tit(1, 2)), "asked".to_owned())
                .requested_by(rid(9)),
        )
        .await
        .unwrap();
    let own = save(handle.as_ref(), &tit(1, 3), "the server's")
        .await
        .unwrap();
    let got = committed_until(&mut events, own.seq).await;
    assert_eq!(
        got.iter()
            .map(|(seq, _, cause)| (*seq, cause.clone()))
            .collect::<Vec<_>>(),
        vec![
            (
                asked.seq,
                StoreCause::Command {
                    origin: Origin::Request(rid(9))
                }
            ),
            (
                own.seq,
                StoreCause::Command {
                    origin: Origin::Server
                }
            ),
        ]
    );
    drop(handle);

    // Through the app: the session's pump and the bus.
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let store = world.store();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(store);
    })
    .await;
    // The handle above closes on a thread of its own; the registry's open waits for its lock.
    let open = loop {
        match t.app.sessions().open_screen(&world.bible, progress()).await {
            Ok(open) => break open,
            Err(OpenFailure::LockHeld { .. }) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(other) => panic!("{other:?}"),
        }
    };
    let bus = t.app.events();
    let mut frames = bus.subscribe(bus.head());
    let session = t.app.sessions().reader(&open.at).unwrap();
    let handle = session.handle().clone();
    handle
        .submit(
            StoreCommand::save_verse(token(handle.as_ref(), &tit(1, 4)), "mine".to_owned())
                .requested_by(rid(10)),
        )
        .await
        .unwrap();
    save(handle.as_ref(), &tit(1, 5), "the server's")
        .await
        .unwrap();
    let mut causes = Vec::new();
    while causes.len() < 2 {
        let next = tokio::time::timeout(Duration::from_secs(5), frames.next())
            .await
            .expect("the frame comes");
        let Next::Frame(frame) = next else {
            panic!("{next:?}")
        };
        if let Event::Session {
            event: SessionEvent::Changed { cause, .. },
            ..
        } = frame.event
        {
            causes.push(cause);
        }
    }
    assert_eq!(
        causes,
        vec![
            ChangeCause::LocalSave { request: rid(10) },
            ChangeCause::System
        ]
    );
}

/// D1, AD37: the relay settles every reply whether or not anyone awaits it. A submit whose
/// future is dropped unpolled (an HTTP caller that went away) is stored, its batch is still
/// announced as its request's, and the events after it are not held back.
#[tokio::test]
async fn a_submit_nobody_awaits_is_stored_and_announced_as_its_request() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let mut events = handle.subscribe();
    let key = tit(1, 8);
    drop(
        handle.submit(
            StoreCommand::save_verse(token(handle.as_ref(), &key), "nobody waits".to_owned())
                .requested_by(rid(7)),
        ),
    );
    let next = save(handle.as_ref(), &tit(1, 9), "the next save")
        .await
        .unwrap();
    let got = committed_until(&mut events, next.seq).await;
    assert_eq!(
        got.into_iter()
            .map(|(_, _, cause)| cause)
            .collect::<Vec<_>>(),
        vec![
            StoreCause::Command {
                origin: Origin::Request(rid(7))
            },
            StoreCause::Command {
                origin: Origin::Server
            },
        ]
    );
    assert_eq!(
        words_of(&handle.snapshot(), &key).as_deref(),
        Some("nobody waits")
    );
    handle.close().await.unwrap();
    let reopened = world.open(&world.bible);
    assert_eq!(
        words_of(&reopened.snapshot(), &key).as_deref(),
        Some("nobody waits"),
        "on disk"
    );
}

/// D1, AD37: a save the actor still holds when the handle closes is answered as saved, and
/// its batch is announced as its request's before the event channel ends: the relay waits for
/// every reply already queued before it stops. The reopened project holds it.
#[tokio::test]
async fn a_save_held_at_the_close_is_kept_and_announced_as_its_request() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let mut events = handle.subscribe();
    let key = tit(1, 10);
    world.hooks.hold(Point::BeforeDurablePending);
    let saving = handle.submit(
        StoreCommand::save_verse(token(handle.as_ref(), &key), "held at the close".to_owned())
            .requested_by(rid(8)),
    );
    let hooks = world.hooks.clone();
    tokio::task::spawn_blocking(move || hooks.wait_reached(Point::BeforeDurablePending, 1))
        .await
        .unwrap();
    // The close takes the store and the relay at once and finishes on a thread of its own, so
    // `close()` returns while the save is held. It is called on a thread of the test's own and
    // the hold is released before the verdict, so a close that blocks fails the test instead of
    // hanging it.
    let (returned, closer) = {
        let handle = handle.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let closer = std::thread::spawn(move || {
            let closing = handle.close();
            let _ = tx.send(());
            closing
        });
        (rx.recv_timeout(Duration::from_secs(1)).is_ok(), closer)
    };
    world.hooks.release(Point::BeforeDurablePending);
    let closing = closer.join().unwrap();
    assert!(returned, "close() returned while the save was held");
    let reply = saving.await.unwrap();
    assert_eq!(reply.stored, Stored::Durable);

    let mut batches = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("the channel ends with the close");
        match event {
            Some(StoreEvent::Committed { seq, cause, .. }) => batches.push((seq, cause)),
            Some(_) => {}
            None => break,
        }
    }
    assert_eq!(
        batches,
        vec![(
            reply.seq,
            StoreCause::Command {
                origin: Origin::Request(rid(8))
            }
        )]
    );
    closing.await.unwrap();
    let reopened = world.open(&world.bible);
    assert_eq!(
        words_of(&reopened.snapshot(), &key).as_deref(),
        Some("held at the close")
    );
}

/// A request id that names a round and a task.
fn request(round: u8, task: u8) -> tc_api::ids::RequestId {
    let mut bytes = [0xaa; 16];
    bytes[0] = round;
    bytes[1] = task;
    tc_api::ids::RequestId::from_bytes(bytes)
}

fn multi_thread() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .build()
        .unwrap()
}

/// The verse among `keys` that one of `ours` names, of a batch that saved one verse.
fn saved_verse<'a>(
    keys: &[RecordKey],
    ours: &'a BTreeMap<RecordKey, tc_api::ids::RequestId>,
) -> Option<(&'a RecordKey, &'a tc_api::ids::RequestId)> {
    keys.iter().find_map(|key| ours.get_key_value(key))
}

/// D1, AD37: the relay takes the replies in the order the actor takes the commands, which
/// `Origins::take` relies on. Many tasks on several threads submit at once, half of them
/// dropping their futures unpolled; every batch is still announced as the request that wrote
/// it, never as the store's own.
#[test]
fn concurrent_submits_are_each_announced_as_their_own_request() {
    const ROUNDS: u8 = 15;
    const TASKS: u8 = 8;
    multi_thread().block_on(async {
        let world = store_world();
        let handle = world.open(&world.bible);
        let mut events = handle.subscribe();
        for round in 0..ROUNDS {
            let ours: BTreeMap<RecordKey, tc_api::ids::RequestId> = (0..TASKS)
                .map(|task| (tit(1, 1 + u16::from(task)), request(round, task)))
                .collect();
            let start = Arc::new(tokio::sync::Barrier::new(usize::from(TASKS)));
            let mut tasks = Vec::new();
            for (task, (key, id)) in ours.iter().enumerate() {
                let (handle, start, key, id) = (handle.clone(), start.clone(), key.clone(), *id);
                tasks.push(tokio::spawn(async move {
                    start.wait().await;
                    let submit = handle.submit(
                        StoreCommand::save_verse(
                            token(handle.as_ref(), &key),
                            format!("round {round}, task {task}"),
                        )
                        .requested_by(id),
                    );
                    if task % 2 == 0 {
                        drop(submit);
                    } else {
                        submit.await.unwrap();
                    }
                }));
            }
            for task in tasks {
                task.await.unwrap();
            }
            let mut announced = BTreeMap::new();
            while announced.len() < ours.len() {
                let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
                    .await
                    .expect("every batch is announced")
                    .expect("the channel is open");
                let StoreEvent::Committed { delta, cause, .. } = event else {
                    continue;
                };
                let (key, id) = saved_verse(&delta.keys, &ours)
                    .unwrap_or_else(|| panic!("round {round}: a batch of {:?}", delta.keys));
                assert_eq!(
                    cause,
                    StoreCause::Command {
                        origin: Origin::Request(*id)
                    },
                    "round {round}: {key:?}"
                );
                announced.insert(key.clone(), cause);
            }
        }
        handle.close().await.unwrap();
    });
}

/// D1, AD37 while someone types: a draft's reply carries the newest snapshot's `seq` and is
/// settled by its submit, out of the actor's order, so it must not end a save's wait for its
/// record. Threads send drafts on one verse without pause while saves of another, each its own
/// request, run one after another: every save is announced as its request, never as the
/// store's own. (Were a draft settled as the relay settles, most saves here would reach the
/// pump as `System`.)
#[test]
fn saves_racing_drafts_are_each_announced_as_their_own_request() {
    const SAVES: u16 = 400;
    const TYPISTS: usize = 4;
    multi_thread().block_on(async {
        let world = store_world();
        let handle = world.open(&world.bible);
        let mut events = handle.subscribe();
        let typed = token(handle.as_ref(), &tit(1, 15));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let typists: Vec<_> = (0..TYPISTS)
            .map(|typist| {
                let (handle, typed, stop) = (handle.clone(), typed.clone(), stop.clone());
                std::thread::spawn(move || {
                    let mut sent = 0_u64;
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        // The store answers a draft at once; its future holds only that answer.
                        drop(handle.submit(StoreCommand::draft(
                            DraftField::Words,
                            vec![typed.clone()],
                            format!("typist {typist}, keystroke {sent}"),
                            tc_journal::WallTime(0),
                        )));
                        sent += 1;
                    }
                    sent
                })
            })
            .collect();
        let saved = tit(1, 12);
        let mut wrong = Vec::new();
        for n in 0..SAVES {
            let mut bytes = [0xcc; 16];
            bytes[..2].copy_from_slice(&n.to_be_bytes());
            let id = tc_api::ids::RequestId::from_bytes(bytes);
            let reply = handle
                .submit(
                    StoreCommand::save_verse(token(handle.as_ref(), &saved), format!("save {n}"))
                        .requested_by(id),
                )
                .await
                .unwrap();
            let (_, _, cause) = committed_until(&mut events, reply.seq)
                .await
                .pop()
                .expect("the save's batch is announced");
            if cause
                != (StoreCause::Command {
                    origin: Origin::Request(id),
                })
            {
                wrong.push((n, cause));
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let drafts: u64 = typists.into_iter().map(|t| t.join().unwrap()).sum();
        assert!(drafts > 0, "the typists sent drafts");
        assert!(
            wrong.is_empty(),
            "{} of {SAVES} saves announced as another's, among {drafts} drafts: {:?}",
            wrong.len(),
            &wrong[..wrong.len().min(5)]
        );
        handle.close().await.unwrap();
    });
}

/// D1, AD37: a close that races submits on several threads. Every save answered as stored is
/// on disk and was announced as its own request before the events ended; every save answered
/// `Closed` is not on disk and was never announced; a save nobody awaited is on disk only if
/// it was announced as its own request.
#[test]
fn a_close_racing_submits_keeps_every_stored_save_and_no_closed_one() {
    const ROUNDS: u8 = 20;
    const TASKS: u8 = 8;
    multi_thread().block_on(async {
        for round in 0..ROUNDS {
            let world = store_world();
            let handle = world.open(&world.bible);
            let mut events = handle.subscribe();
            let start = Arc::new(tokio::sync::Barrier::new(usize::from(TASKS) + 1));
            let mut tasks = Vec::new();
            for task in 0..TASKS {
                let (handle, start) = (handle.clone(), start.clone());
                let key = tit(1, 1 + u16::from(task));
                let words = format!("round {round}, task {task}");
                tasks.push(tokio::spawn(async move {
                    start.wait().await;
                    let submit = handle.submit(
                        StoreCommand::save_verse(token(handle.as_ref(), &key), words.clone())
                            .requested_by(request(round, task)),
                    );
                    let answer = if task % 2 == 0 {
                        drop(submit);
                        None
                    } else {
                        Some(submit.await)
                    };
                    (key, words, request(round, task), answer)
                }));
            }
            let closing = {
                let (handle, start) = (handle.clone(), start.clone());
                tokio::spawn(async move {
                    start.wait().await;
                    handle.close().await
                })
            };
            let mut answers = Vec::new();
            for task in tasks {
                answers.push(task.await.unwrap());
            }
            closing.await.unwrap().unwrap();
            let mut announced = BTreeMap::new();
            loop {
                let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
                    .await
                    .expect("the channel ends with the close");
                match event {
                    Some(StoreEvent::Committed { delta, cause, .. }) => {
                        for key in delta.keys {
                            announced.insert(key, cause.clone());
                        }
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            drop(handle);

            let reopened = world.open(&world.bible);
            let snapshot = reopened.snapshot();
            for (key, words, id, answer) in answers {
                let on_disk = words_of(&snapshot, &key).as_deref() == Some(words.as_str());
                let as_its_own = announced.get(&key)
                    == Some(&StoreCause::Command {
                        origin: Origin::Request(id),
                    });
                match answer {
                    Some(Ok(reply)) => {
                        assert_eq!(reply.stored, Stored::Durable, "round {round}: {key:?}");
                        assert!(on_disk, "round {round}: {key:?} stored, not on disk");
                        assert!(as_its_own, "round {round}: {key:?} {announced:?}");
                    }
                    Some(Err(CommandError::Closed)) => {
                        assert!(!on_disk, "round {round}: {key:?} closed, yet on disk");
                        assert!(!announced.contains_key(&key), "round {round}: {key:?}");
                    }
                    Some(Err(other)) => panic!("round {round}: {key:?} {other:?}"),
                    None => assert_eq!(on_disk, as_its_own, "round {round}: {key:?}"),
                }
            }
            reopened.close().await.unwrap();
        }
    });
}

/// "The channel ends when the handle closes": every subscriber's channel ends with the close,
/// and a subscription after it ends at once.
#[tokio::test]
async fn events_end_when_the_handle_closes() {
    let world = store_world();
    let handle = world.open(&world.bible);
    let mut events = handle.subscribe();
    handle.clone().close().await.unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while events.recv().await.is_some() {}
    })
    .await;
    assert!(ended.is_ok(), "the channel ended");
    let mut late = handle.subscribe();
    assert!(late.try_recv().is_err());
    let late_end = tokio::time::timeout(Duration::from_secs(5), late.recv()).await;
    assert!(
        matches!(late_end, Ok(None)),
        "a subscription after the close ends at once: {late_end:?}"
    );
    // A closed handle refuses commands and answers reads from its last state.
    assert_eq!(
        save(handle.as_ref(), &tit(1, 2), "after the close")
            .await
            .unwrap_err(),
        CommandError::Closed
    );
    assert!(handle.snapshot().seq() >= 1);
    assert!(handle.clone().close().await.is_ok(), "a second close");
}

/// A close writes what is due, keeps every save and lets go of the lock.
#[tokio::test]
async fn close_keeps_every_save_and_lets_go_of_the_lock() {
    let world = store_world();
    let handle = world.open(&world.bible);
    for v in [2, 3, 4] {
        save(handle.as_ref(), &tit(1, v), &format!("kept {v}"))
            .await
            .unwrap();
    }
    handle.close().await.unwrap();
    let path = world.folder(&world.bible);
    let direct = world
        .backend
        .store()
        .open(&path, &tc_store::OpenOptions::default())
        .expect("the lock is free");
    for v in [2, 3, 4] {
        let snapshot = direct.snapshot();
        let words = match snapshot.fold().projected(&tit(1, v)) {
            Some(tc_journal::Record::Verse {
                value: tc_journal::record::VerseValue::Text { words, .. },
                ..
            }) => words,
            other => panic!("{other:?}"),
        };
        assert_eq!(words, format!("kept {v}"));
    }
    direct.close().unwrap();
    assert_valid_burrito(&world.fs, &path);
}

const STAGING_FOLDER: &str = ".staging-create-0123456789abcdef";

/// Copies the files of a folder on this computer into the world's file system at `to`.
fn put_tree(world: &StoreWorld, from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            put_tree(world, &path, &target);
        } else {
            world.fs.put(&target, &std::fs::read(&path).unwrap(), None);
        }
    }
}

/// AD12: no folder that holds someone's work is left off Home. A folder whose `project.json`
/// does not read, is missing while the folder holds a project's other files (its journal,
/// its books, its unsaved drafts; a translationCore 4.0 project copied in), or whose name
/// tc-api cannot carry, is listed as unusable with its reason; a staging folder a create is
/// still writing is no project; a file, an empty folder or one with none of a project's files
/// is neither.
#[tokio::test]
async fn list_names_each_project_and_reports_unusable_folders() {
    let world = store_world();
    let projects = Path::new(PROJECTS);
    let project_json = |name: &str| projects.join(name).join("ingredients/tc/project.json");
    let valid = world
        .fs
        .peek(&project_json("sample_burrito"))
        .expect("a project.json");
    world.fs.put(&project_json(STAGING_FOLDER), &valid, None);
    world.fs.put(&project_json("broken"), b"{}", None);
    world.fs.put(&project_json("..."), &valid, None);
    // The id and the kind alone, which tc-store's open refuses as not a project: listed as
    // unusable with its reason, never as a project that then fails to open.
    let mut two_fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&valid).unwrap();
    two_fields.retain(|field, _| field == "projectId" || field == "kind");
    world.fs.put(
        &project_json("two_fields"),
        &serde_json::to_vec(&two_fields).unwrap(),
        None,
    );
    // A whole project, its journal and its books, that lost only its project.json.
    for (rel, bytes) in store_world::files(&world.fs, &world.folder(&world.bible)) {
        if rel != "ingredients/tc/project.json" {
            world
                .fs
                .put(&projects.join("lost_project_json").join(rel), &bytes, None);
        }
    }
    // A translationCore 4.0 project copied in: metadata.json and ingredients/, no tc/.
    put_tree(
        &world,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tc4/sample-burrito"),
        &projects.join("tc4_sample"),
    );
    // Nothing left but this app's local state: unsaved text.
    world
        .fs
        .put(&projects.join("drafts_only/.tc/drafts.json"), b"[]", None);
    // A project.json another program holds open.
    world.fs.put(&project_json("held"), &valid, None);
    world.fs.set_unreadable(&project_json("held"), true);
    // No one's work: notes, a stray file, an empty folder.
    world.fs.put(&projects.join("notes/readme.txt"), b"x", None);
    world.fs.put(&projects.join("stray.txt"), b"x", None);
    world.fs.create_dir(&projects.join("empty")).unwrap();

    let listing = world.backend.list().unwrap();
    let names: Vec<&str> = listing.projects.iter().map(|e| e.folder.as_str()).collect();
    assert_eq!(
        names,
        [
            "another_project",
            "sample_burrito",
            "sample_burrito_teammates"
        ]
    );
    for entry in &listing.projects {
        assert_eq!(entry.kind, tc_journal::fold::ProjectKind::Bible);
        let handle = world.open(&entry.folder);
        assert_eq!(handle.facts().id, entry.id, "{}", entry.folder);
    }
    // Each with its reason, which Home says in words (B2 D9).
    let reasons: BTreeMap<&str, &UnusableReason> = listing
        .unusable
        .iter()
        .map(|u| (u.name.as_str(), &u.reason))
        .collect();
    assert_eq!(
        reasons.keys().copied().collect::<Vec<_>>(),
        [
            "...",
            "broken",
            "drafts_only",
            "held",
            "lost_project_json",
            "tc4_sample",
            "two_fields"
        ]
    );
    assert_eq!(reasons["..."], &UnusableReason::NameNotAllowed);
    for name in ["drafts_only", "lost_project_json", "tc4_sample"] {
        assert_eq!(reasons[name], &UnusableReason::NoProjectFile, "{name}");
    }
    for name in ["broken", "two_fields"] {
        assert!(
            matches!(reasons[name], UnusableReason::ProjectFileDamaged(r) if !r.is_empty()),
            "{name}: {:?}",
            reasons[name]
        );
    }
    assert!(
        matches!(reasons["held"], UnusableReason::ProjectFileUnreadable(r) if !r.is_empty()),
        "{:?}",
        reasons["held"]
    );
    for name in [
        "two_fields",
        "lost_project_json",
        "tc4_sample",
        "drafts_only",
    ] {
        assert!(
            matches!(
                world.backend.open(&folder(name), progress()),
                Err(OpenFailure::NotAProject)
            ),
            "{name}"
        );
    }
}

/// Rewrites the fields of the JSON file at `file` in the world's files.
fn edit_fields(
    world: &StoreWorld,
    file: &Path,
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) {
    let mut fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&world.fs.peek(file).unwrap()).unwrap();
    edit(&mut fields);
    world
        .fs
        .put(file, &serde_json::to_vec(&fields).unwrap(), None);
}

/// The listing reads `project.json` as tc-store's open reads it: over every way the file can
/// be wrong, missing or merely unusual, a folder is a project in the listing exactly when the
/// open does not refuse it as not a project, and unusable with a reason otherwise. tc-store's
/// reader is private to it, so this holds the adapter's copy to it.
#[tokio::test]
async fn a_folder_is_listed_as_a_project_exactly_when_tc_store_opens_it() {
    type Change = fn(&StoreWorld, &Path);
    // S2 (STORAGE §3.5): a project.json that is wrong or missing is written back from the
    // project's history, so every case opens, and the listing says so too.
    let cases: [(&str, Change, bool); 11] = [
        ("as written", |_, _| {}, true),
        (
            "a field more",
            |w, f| {
                edit_fields(w, f, |m| {
                    m.insert("note".into(), "kept".into());
                });
            },
            true,
        ),
        (
            "no schemaVersion",
            |w, f| edit_fields(w, f, |m| drop(m.remove("schemaVersion"))),
            true,
        ),
        (
            "no created",
            |w, f| edit_fields(w, f, |m| drop(m.remove("created"))),
            true,
        ),
        (
            "no projectId",
            |w, f| edit_fields(w, f, |m| drop(m.remove("projectId"))),
            true,
        ),
        (
            "schemaVersion as text",
            |w, f| {
                edit_fields(w, f, |m| {
                    m.insert("schemaVersion".into(), "1".into());
                });
            },
            true,
        ),
        (
            "a negative schemaVersion",
            |w, f| {
                edit_fields(w, f, |m| {
                    m.insert("schemaVersion".into(), (-1).into());
                });
            },
            true,
        ),
        (
            "created as a number",
            |w, f| {
                edit_fields(w, f, |m| {
                    m.insert("created".into(), 5.into());
                });
            },
            true,
        ),
        (
            "an unknown kind",
            |w, f| {
                edit_fields(w, f, |m| {
                    m.insert("kind".into(), "psalter".into());
                });
            },
            true,
        ),
        (
            "not a project id",
            |w, f| {
                edit_fields(w, f, |m| {
                    m.insert("projectId".into(), "not an id".into());
                });
            },
            true,
        ),
        // The folder keeps its journal, its books and its metadata.json.
        ("deleted", |w, f| w.fs.delete(f).unwrap(), true),
    ];
    for (case, change, project) in cases {
        let world = store_world();
        let file = world
            .folder(&world.bible)
            .join("ingredients/tc/project.json");
        change(&world, &file);

        let listing = world.backend.list().unwrap();
        let listed = listing.projects.iter().any(|e| e.folder == world.bible);
        let unusable = listing
            .unusable
            .iter()
            .find(|u| u.name == world.bible.as_str());
        let opened = world.backend.open(&world.bible, progress());
        let not_a_project = matches!(opened, Err(OpenFailure::NotAProject));
        assert_eq!(listed, project, "{case}: listed as a project");
        assert_eq!(listed, !not_a_project, "{case}: the listing and the open");
        assert_eq!(
            unusable.is_some(),
            !project,
            "{case}: listed as unusable with a reason"
        );
        if let Ok(handle) = opened {
            handle.close().await.unwrap();
        }
    }
}

/// The registry opens only a folder it lists, by the name it listed: another spelling, or a
/// staging folder a create is still writing, is not a project, and nothing is written.
#[tokio::test]
async fn open_refuses_a_name_the_store_does_not_list() {
    let world = store_world();
    let valid = world
        .fs
        .peek(
            &world
                .folder(&world.bible)
                .join("ingredients/tc/project.json"),
        )
        .unwrap();
    world.fs.put(
        &Path::new(PROJECTS)
            .join(STAGING_FOLDER)
            .join("ingredients/tc/project.json"),
        &valid,
        None,
    );
    world.fs.clear_log();
    for name in ["Sample_Burrito", STAGING_FOLDER] {
        assert!(
            matches!(
                world.backend.open(&folder(name), progress()),
                Err(OpenFailure::NotAProject)
            ),
            "{name}"
        );
    }
    assert!(world.fs.log().is_empty(), "{:?}", world.fs.log());
}

/// STORAGE §6.1 O1: a lock another program holds refuses the open and names the holder when
/// the lock does; the refused open writes nothing.
#[tokio::test]
async fn open_of_a_folder_another_program_holds_is_lock_held() {
    let world = store_world();
    let lock = world.folder(&world.bible).join(".tc/lock");
    let held = world.fs.lock(&lock).unwrap().expect("the lock is free");
    world.fs.clear_log();
    assert!(matches!(
        world.backend.open(&world.bible, progress()),
        Err(OpenFailure::LockHeld { holder: None })
    ));
    assert!(world.fs.log().is_empty());
    drop(held);

    // The project open in this installation (pid 4242 on host-1); another installation on
    // the same files asks.
    let handle = world.open(&world.bible);
    let (other, _other_watch) = backend_over(
        &world.fs,
        "/app2",
        FakePlatform {
            pid: 77,
            ..FakePlatform::default()
        },
        Arc::default(),
    );
    world.fs.clear_log();
    let refused = match other.open(&world.bible, progress()) {
        Err(refused) => refused,
        Ok(_) => panic!("opened a held project"),
    };
    assert_eq!(
        refused,
        OpenFailure::LockHeld {
            holder: Some(("host-1".to_owned(), 4242))
        },
        "the lock names its holder, not the asker"
    );
    assert_eq!(
        tc_app::wire::error::open_failure(refused),
        tc_api::ApiError::Refused(tc_api::Refusal::Open(
            tc_api::refusal::OpenRefusal::LockHeld {
                holder: Some(tc_api::refusal::LockHolder {
                    host: "host-1".to_owned(),
                    pid: 4242
                })
            }
        ))
    );
    assert!(world.fs.log().is_empty(), "{:?}", world.fs.log());
    drop(handle);
}

fn phase_index(phase: &Phase) -> usize {
    match phase {
        Phase::Open(OpenPhase::FinishingWork) => 0,
        Phase::Open(OpenPhase::ReadingChanges) => 1,
        Phase::Open(OpenPhase::Folding) => 2,
        Phase::Open(OpenPhase::CheckingFiles) => 3,
        Phase::Open(OpenPhase::WritingFiles) => 4,
        other => panic!("not an open's phase: {other:?}"),
    }
}

/// AD8, PERF rule 4: the open's progress is tc-store's reporter, forwarded with no timer of
/// tc-app's: nothing before 300 ms after the open began, then the store's phases in order,
/// counts that never fall within a phase, and the last report at its total.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_forwards_open_reports_unchanged() {
    let world = store_world();
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir(&projects).unwrap();
    let store = world.store();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(store);
    })
    .await;
    world.fs.set_read_delay(Some(Duration::from_millis(60)));
    let sink = ProgressSink::new(Phase::Open(OpenPhase::ReadingChanges));
    let initial = sink.snapshot();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let began = Instant::now();
    let sampler = {
        let (sink, done) = (sink.clone(), done.clone());
        std::thread::spawn(move || {
            let mut samples = Vec::new();
            loop {
                let finished = done.load(std::sync::atomic::Ordering::SeqCst);
                samples.push((began.elapsed(), sink.snapshot()));
                if finished {
                    return samples;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    };
    let open = t
        .app
        .sessions()
        .open_screen(&world.bible, sink.clone())
        .await
        .unwrap();
    let took = began.elapsed();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let samples = sampler.join().unwrap();
    world.fs.set_read_delay(None);
    assert!(took > Duration::from_millis(600), "the open took {took:?}");

    let first_change = samples
        .iter()
        .position(|(_, p)| *p != initial)
        .expect("the store reported");
    assert!(
        samples[first_change].0 >= Duration::from_millis(290),
        "the first report came at {:?}",
        samples[first_change].0
    );
    let reported = &samples[first_change..];
    for pair in reported.windows(2) {
        let (a, b) = (&pair[0].1, &pair[1].1);
        let (pa, pb) = (phase_index(&a.phase), phase_index(&b.phase));
        assert!(pa <= pb, "phases move forward: {a:?} then {b:?}");
        if pa == pb {
            assert!(a.done <= b.done, "counts never fall: {a:?} then {b:?}");
        }
    }
    let last = &reported.last().unwrap().1;
    assert_eq!(
        Some(last.done),
        last.total,
        "the last report is at its total"
    );
    t.app.sessions().leave_screen(&open.at).unwrap();
}

/// R22, A-20: a draft is answered beside the actor, so it never waits behind a long command.
#[tokio::test]
async fn a_draft_never_waits_behind_a_long_command() {
    let world = store_world();
    let handle = world.open(&world.bible);
    world.hooks.hold(Point::BeforeDurablePending);
    let saving = handle.submit(StoreCommand::save_verse(
        token(handle.as_ref(), &tit(1, 6)),
        "held at the durability point".to_owned(),
    ));
    let hooks = world.hooks.clone();
    tokio::task::spawn_blocking(move || hooks.wait_reached(Point::BeforeDurablePending, 1))
        .await
        .unwrap();
    let draft = tokio::time::timeout(
        Duration::from_secs(1),
        handle.submit(StoreCommand::draft(
            DraftField::Words,
            vec![token(handle.as_ref(), &tit(1, 7))],
            "typed meanwhile".to_owned(),
            tc_journal::WallTime(0),
        )),
    )
    .await
    .expect("the draft did not wait")
    .unwrap();
    assert_eq!(draft.stored, Stored::Unchanged);
    world.hooks.release(Point::BeforeDurablePending);
    assert_eq!(saving.await.unwrap().stored, Stored::Durable);
}

/// The adapter adds no store work: the same save through tc-store and through the seam costs
/// the same file operations, and the seam's reads, mints and parses cost none.
#[tokio::test]
async fn the_adapter_adds_no_store_work() {
    let world = store_world();
    let (a, b) = (world.fs.copy(), world.fs.copy());
    let path = world.folder(&world.bible);
    let key = tit(1, 11);

    let (direct_store, _direct_store_watch) =
        backend_over(&a, APP, FakePlatform::default(), Arc::default());
    let direct = direct_store
        .store()
        .open(&path, &tc_store::OpenOptions::default())
        .unwrap();
    direct.wait_idle();
    let before = direct.counters();
    direct
        .submit(Command::SaveVerse {
            shown: direct.snapshot().token(&key, None).unwrap(),
            words: "the same save".to_owned(),
        })
        .wait()
        .unwrap();
    direct.wait_idle();
    let through_store = direct.counters().since(&before);

    let (seam_store, _seam_store_watch) =
        backend_over(&b, APP, FakePlatform::default(), Arc::default());
    let seam = seam_store.open(&world.bible, progress()).unwrap();
    seam.wait_idle();
    let before = seam.counters();
    save(seam.as_ref(), &key, "the same save").await.unwrap();
    seam.wait_idle();
    let through_seam = seam.counters().since(&before);

    let picked = |c: &tc_store::StoreCounters| {
        [
            ("commands", c.commands),
            ("files_opened", c.files_opened),
            ("files_created", c.files_created),
            ("fsyncs", c.fsyncs),
            ("renames", c.renames),
            ("removes", c.removes),
            ("listings", c.listings),
            ("stats", c.stats),
            ("derived_written", c.derived_written),
            ("drafts_written", c.drafts_written),
        ]
    };
    assert_eq!(picked(&through_seam), picked(&through_store));
    assert_eq!(through_seam.commands, 1);

    let counters = seam.counters();
    let folds = seam.snapshot().fold().counters();
    for _ in 0..1_000 {
        let snapshot = seam.snapshot();
        let minted = snapshot.mint(&key, None).unwrap();
        edit_token(seam.as_ref(), &shown(&minted).unwrap()).unwrap();
    }
    assert_eq!(
        seam.counters().since(&counters),
        tc_store::StoreCounters::default()
    );
    let after = seam.snapshot().fold().counters().since(&folds);
    assert_eq!(after, Counters::default(), "{after:?}");
    direct.close().unwrap();
}

// ---- The property test ------------------------------------------------------------------

const WORDS: [&str; 4] = ["alpha", "beta", "gamma", "delta"];

#[derive(Debug, Clone)]
enum Tok {
    Fresh,
    /// A token minted this many saves ago, if there was one.
    Old(u8),
}

#[derive(Debug, Clone)]
enum Who {
    Request(u8),
    Server,
}

#[derive(Debug, Clone)]
enum Leaf {
    Save {
        verse: u16,
        words: u8,
        token: Tok,
        by: Who,
    },
    Draft {
        verse: u16,
        words: u8,
    },
}

#[derive(Debug, Clone)]
enum Op {
    One(Leaf),
    Checkpoint,
    /// Two submits before either is awaited.
    Together(Leaf, Leaf),
}

fn leaf() -> impl Strategy<Value = Leaf> {
    let tok = prop_oneof![Just(Tok::Fresh), (0u8..4).prop_map(Tok::Old)];
    let who = prop_oneof![(0u8..3).prop_map(Who::Request), Just(Who::Server)];
    prop_oneof![
        4 => (1u16..=6, 0u8..4, tok, who).prop_map(|(verse, words, token, by)| Leaf::Save {
            verse,
            words,
            token,
            by
        }),
        1 => (1u16..=6, 0u8..4).prop_map(|(verse, words)| Leaf::Draft { verse, words }),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => leaf().prop_map(Op::One),
        1 => Just(Op::Checkpoint),
        2 => (leaf(), leaf()).prop_map(|(a, b)| Op::Together(a, b)),
    ]
}

/// One submitted leaf and what is known of it before its reply.
struct Sent {
    key: RecordKey,
    origin: Origin,
    /// The heads the token named; `None` for a draft.
    named: Option<Vec<ChangeId>>,
}

struct Run {
    handle: Arc<dyn ProjectHandle>,
    minted: Vec<EditToken>,
    replies: Vec<(Sent, Result<StoreReply, CommandError>)>,
}

impl Run {
    fn command(&mut self, leaf: &Leaf) -> (Sent, StoreCommand) {
        match leaf {
            Leaf::Save {
                verse,
                words,
                token: tok,
                by,
            } => {
                let key = tit(1, *verse);
                let fresh = token(self.handle.as_ref(), &key);
                let shown = match tok {
                    Tok::Old(n) => self
                        .minted
                        .iter()
                        .rev()
                        .filter(|t| t.key() == &key)
                        .nth(usize::from(*n))
                        .cloned()
                        .unwrap_or(fresh),
                    Tok::Fresh => fresh,
                };
                self.minted.push(shown.clone());
                let origin = match by {
                    Who::Request(n) => Origin::Request(rid(*n)),
                    Who::Server => Origin::Server,
                };
                let mut command =
                    StoreCommand::save_verse(shown.clone(), WORDS[usize::from(*words)].to_owned());
                if let Origin::Request(id) = origin {
                    command = command.requested_by(id);
                }
                (
                    Sent {
                        key,
                        origin,
                        named: Some(shown.heads().to_vec()),
                    },
                    command,
                )
            }
            Leaf::Draft { verse, words } => {
                let key = tit(1, *verse);
                let command = StoreCommand::draft(
                    DraftField::Words,
                    vec![token(self.handle.as_ref(), &key)],
                    format!("{} (typed)", WORDS[usize::from(*words)]),
                    tc_journal::WallTime(0),
                );
                (
                    Sent {
                        key,
                        origin: Origin::Server,
                        named: None,
                    },
                    command,
                )
            }
        }
    }

    /// Submits the leaves one after the other, then awaits their replies, and checks each
    /// save's heads against the heads before it (STORAGE I4): a save replaces exactly the
    /// heads its token named and adds its change.
    async fn submit(&mut self, leaves: &[&Leaf]) -> Result<(), TestCaseError> {
        let mut heads: BTreeMap<RecordKey, BTreeSet<ChangeId>> = BTreeMap::new();
        let mut pending = Vec::new();
        for leaf in leaves {
            let (sent, command) = self.command(leaf);
            heads.entry(sent.key.clone()).or_insert_with(|| {
                self.handle
                    .snapshot()
                    .fold()
                    .heads(&sent.key)
                    .into_iter()
                    .collect()
            });
            pending.push((sent, self.handle.submit(command)));
        }
        for (sent, future) in pending {
            let reply = future.await;
            if let (Ok(reply), Some(named)) = (&reply, &sent.named) {
                let expected = heads.get_mut(&sent.key).unwrap();
                if let Some(new) = reply.changes.iter().find(|id| {
                    self.handle
                        .snapshot()
                        .fold()
                        .body(id)
                        .is_some_and(|b| b.record.key() == sent.key)
                }) {
                    for head in named {
                        expected.remove(head);
                    }
                    expected.insert(*new);
                }
            }
            self.replies.push((sent, reply));
        }
        for (key, expected) in heads {
            let now: BTreeSet<ChangeId> = self
                .handle
                .snapshot()
                .fold()
                .heads(&key)
                .into_iter()
                .collect();
            prop_assert_eq!(now, expected, "{:?}", key);
        }
        Ok(())
    }
}

fn check_run(ops: Vec<Op>) -> Result<(), TestCaseError> {
    let world = store_world();
    let rt = runtime();
    rt.block_on(async {
        let handle = world.open(&world.bible);
        let first = handle.snapshot().seq();
        let mut events = handle.subscribe();
        let mut run = Run {
            handle: handle.clone(),
            minted: Vec::new(),
            replies: Vec::new(),
        };
        for op in &ops {
            match op {
                Op::One(leaf) => run.submit(&[leaf]).await?,
                Op::Together(a, b) => run.submit(&[a, b]).await?,
                Op::Checkpoint => {
                    let outcome = handle
                        .checkpoint(CheckpointReason::Other("prop".into()))
                        .await;
                    prop_assert!(matches!(
                        outcome,
                        CheckpointOutcome::Committed | CheckpointOutcome::Unchanged
                    ));
                }
            }
        }
        let last = handle.snapshot().seq();
        let keys: BTreeSet<RecordKey> = (1..=6).map(|v| tit(1, v)).collect();
        let heads_before: BTreeMap<RecordKey, Vec<ChangeId>> = keys
            .iter()
            .map(|k| (k.clone(), sorted(handle.snapshot().fold().heads(k))))
            .collect();
        handle.clone().close().await.unwrap();
        let mut committed = Vec::new();
        while let Some(event) = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("the channel ends with the close")
        {
            if let StoreEvent::Committed { seq, delta, cause } = event {
                committed.push((seq, delta, cause));
            }
        }

        // 3: consecutive seqs ending at the newest snapshot.
        let seqs: Vec<u64> = committed.iter().map(|(seq, _, _)| *seq).collect();
        prop_assert_eq!(&seqs, &((first + 1)..=last).collect::<Vec<_>>());
        // 1 and 2: one reply each; one event per stored batch, with its origin.
        let mut by_seq: BTreeMap<u64, (tc_journal::fold::Delta, StoreCause)> = committed
            .into_iter()
            .map(|(seq, delta, cause)| (seq, (delta, cause)))
            .collect();
        let mut stored = Vec::new();
        for (sent, reply) in &run.replies {
            match reply {
                Ok(reply) if !reply.changes.is_empty() => {
                    let (delta, cause) = by_seq
                        .remove(&reply.seq)
                        .ok_or_else(|| TestCaseError::fail(format!("no event for {reply:?}")))?;
                    prop_assert_eq!(
                        cause,
                        StoreCause::Command {
                            origin: sent.origin.clone()
                        }
                    );
                    prop_assert!(delta.keys.contains(&sent.key));
                    stored.extend(reply.changes.iter().copied());
                }
                Ok(reply) => prop_assert_eq!(&reply.stored, &Stored::Unchanged),
                Err(error) => {
                    return Err(TestCaseError::fail(format!("refused: {error:?}")));
                }
            }
        }
        prop_assert!(
            by_seq.is_empty(),
            "events no reply names: {:?}",
            by_seq.keys()
        );

        // 5: after a close and a reopen on the files a killed process leaves, every change
        // a reply listed is there, and so is every head.
        let after = world.fs.crash(false);
        let (backend, _backend_watch) =
            backend_over(&after, APP, FakePlatform::default(), Arc::default());
        let reopened = backend.open(&world.bible, progress()).unwrap();
        for id in &stored {
            prop_assert!(reopened.snapshot().fold().body(id).is_some(), "{id:?} lost");
        }
        for (key, heads) in &heads_before {
            prop_assert_eq!(&sorted(reopened.snapshot().fold().heads(key)), heads);
        }
        reopened.close().await.unwrap();
        assert_valid_burrito(&after, &world.folder(&world.bible));
        Ok(())
    })
}

fn sorted(mut ids: Vec<ChangeId>) -> Vec<ChangeId> {
    ids.sort();
    ids
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        max_shrink_iters: 64,
        .. ProptestConfig::default()
    })]

    /// Every command submitted through the seam gets exactly one reply; every batch the store
    /// keeps reaches the seam as one `Committed` with the origin of the command that wrote it,
    /// in consecutive seqs; no save removes a head its token did not name; and a close and a
    /// reopen keep every change a reply listed.
    #[test]
    fn the_seam_answers_every_command_once_and_loses_nothing(
        ops in prop::collection::vec(op(), 1..12)
    ) {
        check_run(ops)?;
    }
}

// ---- The crash test ---------------------------------------------------------------------

/// The words of every verse of Titus, by key.
fn titus(snapshot: &tc_app::backend::Snapshot) -> BTreeMap<RecordKey, Option<String>> {
    let fold = snapshot.fold();
    fold.chapters_of(BookId::Titus)
        .into_iter()
        .flat_map(|c| fold.chapter_keys(BookId::Titus, c).verses)
        .map(|key| {
            let words = words_of(snapshot, &key);
            (key, words)
        })
        .collect()
}

/// PLAN P4 through the adapter: the writer killed at every write point of a save, and at the
/// first write points of the close that follows it, with a power cut after it. The project
/// always opens and validates; a save the seam reported durable is there; a save it reported
/// not written, or written but not durable, is there or not, and nothing else changed.
#[test]
fn a_crash_at_any_write_point_of_a_save_through_the_seam_keeps_every_durable_save() {
    let world: StoreWorld = store_world();
    let base = world.fs.crash(false);
    let key = tit(1, 3);
    let new_words = "kept through a crash";
    let rt = runtime();

    let (old, points) = {
        let fs = base.copy();
        let (backend, _backend_watch) =
            backend_over(&fs, APP, FakePlatform::default(), Arc::default());
        let handle = backend.open(&world.bible, progress()).unwrap();
        handle.wait_idle();
        let old = titus(&handle.snapshot());
        let start = fs.ops();
        rt.block_on(save(handle.as_ref(), &key, new_words)).unwrap();
        let points = fs.ops() - start;
        rt.block_on(handle.close()).unwrap();
        (old, points)
    };
    assert!(points >= 3, "a save has write points: {points}");

    let mut outcomes = BTreeMap::new();
    for k in 0..points + 3 {
        for torn in [false, true] {
            let fs = base.copy();
            let (backend, _backend_watch) =
                backend_over(&fs, APP, FakePlatform::default(), Arc::default());
            let handle = backend.open(&world.bible, progress()).unwrap();
            handle.wait_idle();
            fs.kill_before(k, torn);
            let result = rt.block_on(save(handle.as_ref(), &key, new_words));
            *outcomes
                .entry(match &result {
                    Ok(reply) => format!("{:?}", reply.stored).chars().take(18).collect(),
                    Err(error) => format!("{error:?}").chars().take(10).collect::<String>(),
                })
                .or_insert(0) += 1;
            drop(handle);
            let after = fs.crash(true);
            let (backend, _backend_watch) =
                backend_over(&after, APP, FakePlatform::default(), Arc::default());
            let reopened = backend
                .open(&world.bible, progress())
                .unwrap_or_else(|e| panic!("k {k} torn {torn}: {e:?}"));
            let now = titus(&reopened.snapshot());
            let heads = reopened.snapshot().fold().heads(&key);
            let held: Vec<Option<String>> = heads
                .iter()
                .map(
                    |h| match reopened.snapshot().fold().body(h).map(|b| b.record) {
                        Some(tc_journal::Record::Verse {
                            value: tc_journal::record::VerseValue::Text { words, .. },
                            ..
                        }) => Some(words),
                        _ => None,
                    },
                )
                .collect();
            match &result {
                Ok(reply) if reply.stored == Stored::Durable => {
                    assert!(
                        held.iter().any(|w| w.as_deref() == Some(new_words)),
                        "k {k} torn {torn}: a durable save was lost: {held:?}"
                    );
                }
                Ok(_) | Err(CommandError::NotWritten(_)) => {
                    let shown = now[&key].as_deref();
                    assert!(
                        shown == Some(new_words) || shown == old[&key].as_deref(),
                        "k {k} torn {torn}: {shown:?}"
                    );
                }
                Err(other) => panic!("k {k} torn {torn}: {other:?}"),
            }
            for (verse, words) in &old {
                if *verse != key {
                    assert_eq!(&now[verse], words, "k {k} torn {torn}: {verse:?}");
                }
            }
            rt.block_on(reopened.close()).unwrap();
            assert_valid_burrito(&after, &world.folder(&world.bible));
        }
    }
    // Every kind of outcome was met: killed before the file was in place, after it was in
    // place but before its folder was flushed, and after the save was durable.
    let kinds: Vec<&String> = outcomes.keys().collect();
    assert!(
        kinds.iter().any(|k| k.starts_with("NotWritten")),
        "{outcomes:?}"
    );
    assert!(
        kinds.iter().any(|k| k.starts_with("WrittenNotDurable")),
        "{outcomes:?}"
    );
    assert!(
        kinds.iter().any(|k| k.starts_with("Durable")),
        "{outcomes:?}"
    );
}
