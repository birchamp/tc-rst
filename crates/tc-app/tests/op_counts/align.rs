//! The count rows of B6 (§8.2), on Psalms and on the corpus ten times larger: opening one
//! verse in the aligner and editing it touch that verse, whatever the size of the project.

use std::time::Duration;

use tc_align::Alignment;
use tc_api::Reply;
use tc_api::align::{AlignEdit, AlignerView, BankClick, Edit, Open, OpenVerse, Opened};
use tc_api::event::{ChangeScope, Event, SessionEvent};
use tc_api::ids::{JobId, RequestId};
use tc_api::jobs::{JobKind, JobState};
use tc_api::prefs::SetAlignSuggestions;
use tc_api::test::{Hold, HoldPoint, Release};
use tc_app::App;
use tc_app::events::Next;
use tc_app::jobs::JobRegistry;
use tc_app::wire::scope::record_ref;
use tc_journal::{ChapterNo, Record, RecordKey, VerseNo};

use super::corpus::{PSALMS, corpora, store_corpora};
use super::harness::{Backing, OnStore, Scale, cost, costs_what_it_touches, growth, opened, scale};

async fn ok<E: tc_api::Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

fn request(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

/// Verse 1 of a chapter whose records are the same in both corpora.
fn first_verse() -> OpenVerse {
    OpenVerse {
        chapter: corpora().chapter,
        verse: 1,
    }
}

async fn open_verse(s: &Scale, at: OpenVerse, n: u8) -> AlignerView {
    match ok(
        &s.app,
        &Open {
            at: s.scope(),
            request: request(n),
            book: PSALMS,
            verse: Some(at),
        },
    )
    .await
    {
        Opened::Ready(view) => *view,
        other => panic!("{other:?}"),
    }
}

/// Opening a verse reads that verse's record and its alignment, the pins and the numbering,
/// and loads the book's resources once; the project's size changes none of it. The first
/// verse costs the cut of the Hebrew Bible, the second does not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_open_touches_one_verse() {
    let ((small, _), _) = costs_what_it_touches(
        "opening one verse in the aligner",
        OnStore::AppCounts,
        opened,
        async |s: &Scale| {
            let at = first_verse();
            let view = open_verse(s, at, 1).await;
            assert!(!view.cards.is_empty());
        },
    )
    .await;
    assert_eq!(small.app.book_resource_loads, 1, "{small:?}");
    // The verse, its alignment, the pins (five groups), the versification and the two
    // tokens: a handful of records, not the book's.
    assert!(small.app.records_read <= 24, "{small:?}");
}

/// A click, a placement and a Mark valid read nothing of the project: the session holds the
/// verse.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_edit_touches_one_verse() {
    let ((small, large), _) = costs_what_it_touches(
        "editing one verse in the aligner",
        OnStore::Everything,
        async |s: &Scale| {
            opened(s).await;
            let at = first_verse();
            open_verse(s, at, 1).await;
        },
        async |s: &Scale| {
            let at = first_verse();
            let view = open_verse(s, at, 1).await;
            let clicked = ok(
                &s.app,
                &Edit {
                    at: s.scope(),
                    request: request(2),
                    aligner: view.aligner,
                    seq: view.seq,
                    edit: AlignEdit::ClickBank {
                        word: view.bank[0].clone(),
                        click: BankClick::Plain,
                    },
                },
            )
            .await;
            assert_eq!(clicked.seq, view.seq + 1);
        },
    )
    .await;
    // The session holds the verse: a click reads nothing of the project.
    assert_eq!(small.app.records_read, 0, "{small:?}");
    assert_eq!(small.app.book_resource_loads, 0, "{small:?}");
    assert_eq!(large.app.records_read, 0, "{large:?}");
}

/// The rail reads each verse of the book and its alignment once, and nothing of the rest of the
/// project: its cost is the book's, whatever the history behind it (PERF rule 1).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_index_costs_the_book() {
    let ((small, large), _) = costs_what_it_touches(
        "the aligner's rail",
        OnStore::AppCounts,
        opened,
        async |s: &Scale| {
            let rail = ok(
                &s.app,
                &tc_api::align::Index {
                    at: s.scope(),
                    book: PSALMS,
                },
            )
            .await;
            assert!(rail.items.len() > 2_000, "{}", rail.items.len());
        },
    )
    .await;
    assert_eq!(small.app.book_resource_loads, 0, "{small:?}");
    // Two records for each of the book's verses (its text, its alignment), and no more.
    assert!(small.app.records_read <= 2 * 2_461 + 8, "{small:?}");
    assert_eq!(small.app.records_read, large.app.records_read);
}

/// Opening the cached aligner, changing a placement and flushing cost one verse's records.
/// The edit stays inside the measured action so its timer cannot save during setup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_save_costs_one_verse() {
    let (_, (stored, _)) = costs_what_it_touches(
        "opening, editing and saving one verse's cached aligner",
        OnStore::Everything,
        async |s: &Scale| {
            opened(s).await;
            open_verse(s, first_verse(), 1).await;
            if let Some(memory) = s.memory() {
                memory
                    .script(&s.folder, tc_app::backend::memory::Scripted::Written)
                    .unwrap();
            }
        },
        async |s: &Scale| {
            let view = open_verse(s, first_verse(), 2).await;
            let (card, word) = view
                .cards
                .iter()
                .find_map(|card| card.placed.first().map(|word| (card.index, word.clone())))
                .expect("the real Psalms verse has placed words");
            let edited: AlignerView = ok(
                &s.app,
                &Edit {
                    at: s.scope(),
                    request: request(3),
                    aligner: view.aligner,
                    seq: view.seq,
                    edit: AlignEdit::Unplace { card, word },
                },
            )
            .await;
            assert_eq!(edited.placed + 1, view.placed);
            assert_eq!(edited.save, tc_api::align::AlignSave::Unsaved);
            let saved: tc_api::align::AlignSave = ok(
                &s.app,
                &tc_api::align::Flush {
                    at: s.scope(),
                    request: request(4),
                },
            )
            .await;
            assert_eq!(saved, tc_api::align::AlignSave::Saved);
        },
    )
    .await;
    assert!(stored.fold.keys_updated > 0, "{stored:?}");
    assert!(stored.store.commands > 0, "{stored:?}");
}

fn first_alignment() -> RecordKey {
    RecordKey::Alignment(
        PSALMS,
        ChapterNo::new(first_verse().chapter).unwrap(),
        VerseNo::new(1).unwrap(),
    )
}

/// Waits for the session's frame that names `key` among the changed records.
async fn changed(frames: &mut tc_app::events::Subscription, key: &RecordKey) {
    let record = record_ref(key).unwrap();
    loop {
        match frames.next().await {
            Next::Frame(frame) => {
                if let Event::Session {
                    event:
                        SessionEvent::Changed {
                            scope: ChangeScope::Parts(parts),
                            ..
                        },
                    ..
                } = frame.event
                    && parts.records.iter().any(|r| r.record == record)
                {
                    return;
                }
            }
            other => panic!("the session's event stream ended: {other:?}"),
        }
    }
}

/// Cancelling also frees Train's wait, so a failed assertion cannot strand a blocking job.
struct CancelTrainingOnDrop {
    jobs: JobRegistry,
    id: JobId,
}

impl Drop for CancelTrainingOnDrop {
    fn drop(&mut self) {
        self.jobs.cancel(self.id);
    }
}

async fn training_held(s: &Scale) -> CancelTrainingOnDrop {
    let mut events = s.app.events().subscribe(s.app.events().head());
    ok(
        &s.app,
        &Hold {
            request: request(40),
            at: HoldPoint::Train,
        },
    )
    .await;
    ok(
        &s.app,
        &SetAlignSuggestions {
            at: s.scope(),
            request: request(41),
            on: true,
        },
    )
    .await;
    let running = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.next().await {
                Next::Frame(frame) => {
                    if let Event::Job(job) = frame.event
                        && job.kind == JobKind::Train
                        && matches!(job.state, JobState::Running(_))
                    {
                        break job.id;
                    }
                }
                other => panic!("training's event stream ended: {other:?}"),
            }
        }
    })
    .await;
    // Clean up even if the progress reporter failed to announce the held job.
    if running.is_err() {
        ok(
            &s.app,
            &SetAlignSuggestions {
                at: s.scope(),
                request: request(42),
                on: false,
            },
        )
        .await;
        ok(
            &s.app,
            &Release {
                request: request(43),
                at: HoldPoint::Train,
            },
        )
        .await;
    }
    let guard = CancelTrainingOnDrop {
        jobs: s.app.jobs().clone(),
        id: running.expect("the held Train job reports Running"),
    };
    assert_eq!(
        s.app.holding(),
        1,
        "training reached the gate before any corpus reads"
    );
    guard
}

/// The model's captured snapshot stays held throughout the edited save. No background
/// corpus read enters the counters, and a save that waited for training could not finish.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_save_costs_one_verse_while_model_training_is_held() {
    let (memory, disk) = (corpora(), store_corpora());
    for backing in [Backing::Memory, Backing::Store] {
        let mut costs = Vec::new();
        for (project, on_disk) in [
            (&memory.psalms, &disk.psalms),
            (&memory.larger, &disk.larger),
        ] {
            let s = scale(backing, project, on_disk).await;
            opened(&s).await;
            let before = open_verse(&s, first_verse(), 1).await;
            let train = training_held(&s).await;
            if let Some(memory) = s.memory() {
                memory
                    .script(&s.folder, tc_app::backend::memory::Scripted::Written)
                    .unwrap();
            }
            let mut frames = s.app.events().subscribe(s.app.events().head());
            let measured = tokio::time::timeout(
                Duration::from_secs(5),
                cost(&s, async |s| {
                    let view = open_verse(s, first_verse(), 2).await;
                    assert_eq!(view.suggest.status, tc_api::align::SuggestStatus::Reading);
                    let (card, word) = view
                        .cards
                        .iter()
                        .find_map(|card| card.placed.first().map(|word| (card.index, word.clone())))
                        .expect("the real Psalms verse has placed words");
                    let edited: AlignerView = ok(
                        &s.app,
                        &Edit {
                            at: s.scope(),
                            request: request(3),
                            aligner: view.aligner,
                            seq: view.seq,
                            edit: AlignEdit::Unplace { card, word },
                        },
                    )
                    .await;
                    assert_eq!(edited.placed + 1, view.placed);
                    assert_eq!(edited.save, tc_api::align::AlignSave::Unsaved);
                    let saved = ok(
                        &s.app,
                        &tc_api::align::Flush {
                            at: s.scope(),
                            request: request(4),
                        },
                    )
                    .await;
                    assert_eq!(saved, tc_api::align::AlignSave::Saved);
                    if backing == Backing::Store {
                        // The session tells the screens who wrote the saved record, a read of
                        // its own; the store being idle does not mean that frame has gone.
                        changed(&mut frames, &first_alignment()).await;
                    }
                }),
            )
            .await
            .expect("the edited save finishes before training is released");
            assert_eq!(s.app.holding(), 1, "the save did not release training");
            assert!(matches!(
                s.app.jobs().view(train.id).unwrap().state,
                JobState::Running(_)
            ));
            if backing == Backing::Store {
                // MemoryBackend's scripted Written reply does not change its fold. Check
                // durable projection only where the store actually applied this save.
                let Some(Record::Alignment { value, .. }) = s
                    .app
                    .read(&s.scope())
                    .unwrap()
                    .read()
                    .projected(&first_alignment())
                else {
                    panic!("the saved verse lost its alignment");
                };
                assert_eq!(value.placed() + 1, usize::try_from(before.placed).unwrap());
                assert!(measured.fold.keys_updated > 0, "{measured:?}");
                assert!(measured.store.commands > 0, "{measured:?}");
            }
            costs.push(measured);

            // Releasing corpus reads before cost() took its final snapshot would mix the
            // whole background model's work into the one-verse count.
            let mut ended = s.app.jobs().watch(train.id).unwrap();
            ok(
                &s.app,
                &SetAlignSuggestions {
                    at: s.scope(),
                    request: request(44),
                    on: false,
                },
            )
            .await;
            ok(
                &s.app,
                &Release {
                    request: request(45),
                    at: HoldPoint::Train,
                },
            )
            .await;
            let terminal = tokio::time::timeout(
                Duration::from_secs(5),
                ended.wait_for(JobRegistry::is_ended),
            )
            .await
            .expect("the held training cancels")
            .unwrap()
            .clone();
            assert_eq!(terminal, JobState::Cancelled);
            assert_eq!(s.app.holding(), 0);
            drop(train);
            s.app.quit().await;
        }
        let grew = growth(&costs[0], &costs[1]);
        assert!(
            grew.is_empty(),
            "saving with training held grows on {backing:?}:\n{}",
            grew.join("\n")
        );
    }
}
