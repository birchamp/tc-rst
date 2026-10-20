//! The Align screen's server side (unit B6) through `App::request`, on the parity scenarios
//! with their resources installed: the rail as tC4's baseline shows it, a verse opened with
//! its bank and its cards, the edits, and the save of the links on the memory backend's
//! scripted outcomes.

use std::sync::Arc;
use std::time::Duration;

use tc_api::align::{
    AlignEdit, AlignSave, AlignerView, BankClick, BankSelection, Blocked, Close, Edit, Flush,
    Freshness, Index, IndexView, KeepReflow, Open, OpenVerse, Opened, RailState, Word,
};
use tc_api::event::{Event, SessionEvent};
use tc_api::ids::Scope;
use tc_api::session::Close as CloseProject;
use tc_api::write::NotSaved;
use tc_api::{ApiError, Reply};
use tc_app::backend::memory::{MemoryBackend, Scripted};
use tc_app::backend::scenario::install_resources_into;
use tc_app::events::Next;
use tc_app::{App, StoreChoice};
use tc_core::BookId;
use tc_journal::command::Command;
use tc_journal::{ChapterNo, RecordKey, VerseNo};

use super::common::{Parts, TestApp, folder};
use super::scenario::scenario;
use super::understand::{ok, opened};

const SAMPLE: &str = "sample_burrito";

/// An app over a parity scenario, keeping a handle on its memory backend so a test can script
/// what the store answers to a save.
async fn start(name: &str) -> (tempfile::TempDir, TestApp, MemoryBackend) {
    let state = scenario(name);
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let kept = backend.clone();
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(Arc::new(backend));
    })
    .await;
    (dir, t, kept)
}

/// A request id no earlier request of this test binary used: a retry under the same id is
/// answered with the first reply.
fn request(_label: u8) -> tc_api::ids::RequestId {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&n.to_le_bytes());
    tc_api::ids::RequestId::from_bytes(bytes)
}

async fn index(app: &App, at: &Scope) -> IndexView {
    ok(
        app,
        &Index {
            at: at.clone(),
            book: BookId::Titus,
        },
    )
    .await
}

async fn open_verse(app: &App, at: &Scope, chapter: u16, verse: u16) -> Opened {
    ok(
        app,
        &Open {
            at: at.clone(),
            request: request(60),
            book: BookId::Titus,
            verse: Some(OpenVerse { chapter, verse }),
        },
    )
    .await
}

async fn ready(app: &App, at: &Scope, chapter: u16, verse: u16) -> AlignerView {
    match open_verse(app, at, chapter, verse).await {
        Opened::Ready(view) => *view,
        other => panic!("{other:?}"),
    }
}

async fn edit(
    app: &App,
    at: &Scope,
    view: &AlignerView,
    n: u8,
    edit: AlignEdit,
) -> Result<AlignerView, ApiError> {
    match app
        .request(&Edit {
            at: at.clone(),
            request: request(n),
            aligner: view.aligner,
            seq: view.seq,
            edit,
        })
        .await
        .unwrap()
    {
        Reply::Ok(view) => Ok(view),
        Reply::Err(error) => Err(error),
    }
}

async fn flush(app: &App, at: &Scope, n: u8) -> AlignSave {
    ok(
        app,
        &Flush {
            at: at.clone(),
            request: request(n),
        },
    )
    .await
}

/// The bank chip of `word`, by its place in the bank.
fn chip(view: &AlignerView, word: &str) -> u32 {
    view.bank
        .iter()
        .position(|w| w.word == word)
        .unwrap_or_else(|| panic!("{word} is not in the bank: {:?}", view.bank)) as u32
}

/// The card whose original word is `word`.
fn card(view: &AlignerView, word: &str) -> u32 {
    view.cards
        .iter()
        .position(|c| c.top.iter().any(|t| t.word == word))
        .unwrap_or_else(|| panic!("no card for {word}")) as u32
}

/// Places `word` of the bank on the card of the original word `on`, by a click and a click.
async fn place(
    app: &App,
    at: &Scope,
    view: &AlignerView,
    n: u8,
    word: &str,
    on: &str,
) -> AlignerView {
    let armed = edit(
        app,
        at,
        view,
        n,
        AlignEdit::ClickBank {
            index: chip(view, word),
            click: BankClick::Plain,
        },
    )
    .await
    .unwrap();
    edit(
        app,
        at,
        &armed,
        n + 1,
        AlignEdit::PlaceSelection {
            card: card(&armed, on),
        },
    )
    .await
    .unwrap()
}

/// The save states the app has announced for any aligner so far.
fn saves(app: &App) -> Vec<AlignSave> {
    let mut reader = app.events().subscribe(0);
    let mut out = Vec::new();
    while let Some(next) = reader.try_next() {
        if let Next::Frame(frame) = next
            && let Event::Session {
                event: SessionEvent::AlignSave { state, .. },
                ..
            } = frame.event
        {
            out.push(state);
        }
    }
    out
}

fn saved_links(backend: &MemoryBackend) -> Vec<tc_journal::command::AlignmentEdit> {
    backend
        .submitted(&folder(SAMPLE))
        .into_iter()
        .filter_map(|c| match c {
            Command::SaveAlignment { value, .. } => Some(value),
            _ => None,
        })
        .collect()
}

/// `align-workspace`: the rail lists all 46 verses of Titus, 5 of them drafted; Titus 1:1 has
/// the sample's six links and 27 words.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_sample_rail_is_tc4s() {
    let (_dir, t, _) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let rail = index(&t.app, &at).await;
    assert_eq!(rail.items.len(), 46);
    let drafted: Vec<_> = rail
        .items
        .iter()
        .filter(|i| i.status.state != RailState::Undrafted)
        .collect();
    assert_eq!(drafted.len(), 5);
    let first = drafted[0];
    assert_eq!(first.place.label(), "1:1");
    assert_eq!(first.status.state, RailState::Todo);
    assert_eq!((first.status.placed, first.status.total), (6, 27));
    assert!(first.text.starts_with("Pablo"), "{}", first.text);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn titus_1_1_opens_as_tc4s_workspace() {
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    assert_eq!(view.place.label(), "1:1");
    assert_eq!((view.placed, view.total), (6, 27));
    assert_eq!(view.bank.len(), 21);
    // Ten cards hold eleven Greek words and six more cards the rest: 17 words on 16 cards.
    assert_eq!(view.cards.len(), 16);
    assert_eq!(view.cards.iter().map(|c| c.top.len()).sum::<usize>(), 17);
    assert_eq!(view.selection, BankSelection::None);
    assert_eq!(view.save, AlignSave::Saved);
    assert_eq!(view.freshness, Freshness::Current);
    assert!(view.dropped.is_empty());
    assert!(!view.done);
    assert!(view.gateway.is_some(), "the ULT pane carries the verse");
    assert_eq!(view.cards[0].gateway.as_deref(), Some("Paul"));
    assert!(view.original.starts_with("Παῦλος"), "{}", view.original);
    // Opening, and a click, write nothing (tC4 #787: nothing is written at open).
    let selected = edit(
        &t.app,
        &at,
        &view,
        61,
        AlignEdit::ClickBank {
            index: 0,
            click: BankClick::Plain,
        },
    )
    .await
    .unwrap();
    assert_eq!(selected.selection, BankSelection::Armed(vec![0]));
    assert_eq!(selected.save, AlignSave::Saved, "a selection is not a link");
    assert!(backend.submitted(&folder(SAMPLE)).is_empty());
}

/// A verse with no text, and one that only a span's first verse can open, say so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_verse_with_nothing_to_align_says_why() {
    let (_dir, t, _) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    assert_eq!(
        open_verse(&t.app, &at, 1, 6).await,
        Opened::Blocked(Blocked::Undrafted)
    );
    let reply = t
        .app
        .request(&Open {
            at,
            request: request(62),
            book: BookId::Titus,
            verse: Some(OpenVerse {
                chapter: 9,
                verse: 9,
            }),
        })
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::BadRequest { .. })),
        "{reply:?}"
    );
}

/// J5: click a bank word, click its card, and the word is placed; the selection clears, the
/// bank loses the word and the view says the links are not saved yet.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_click_on_a_word_and_a_click_on_its_card_places_it() {
    let (_dir, t, _) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    let placed = place(&t.app, &at, &view, 61, "fe", "πίστιν").await;
    assert_eq!(placed.placed, view.placed + 1);
    assert_eq!(placed.bank.len(), view.bank.len() - 1);
    assert_eq!(placed.selection, BankSelection::None);
    assert_eq!(placed.save, AlignSave::Unsaved);
    let on = &placed.cards[card(&placed, "πίστιν") as usize];
    assert!(on.placed.iter().any(|w| w.word == "fe"), "{on:?}");
    assert!(placed.seq > view.seq);
    // And the word goes back with a click on it.
    let word = Word {
        word: "fe".into(),
        occurrence: 1,
        occurrences: 1,
    };
    let back = edit(
        &t.app,
        &at,
        &placed,
        65,
        AlignEdit::Unplace {
            card: card(&placed, "πίστιν"),
            word,
        },
    )
    .await
    .unwrap();
    assert_eq!(back.bank.len(), view.bank.len());
    assert_eq!(back.placed, view.placed);
}

/// Ctrl and Shift select several bank words, and one click on a card places them all (#736).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn several_selected_words_are_placed_in_one_edit() {
    let (_dir, t, _) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    let (a, b) = (chip(&view, "conocimiento"), chip(&view, "piedad"));
    let one = edit(
        &t.app,
        &at,
        &view,
        61,
        AlignEdit::ClickBank {
            index: a,
            click: BankClick::Plain,
        },
    )
    .await
    .unwrap();
    let two = edit(
        &t.app,
        &at,
        &one,
        62,
        AlignEdit::ClickBank {
            index: b,
            click: BankClick::Toggle,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        two.selection,
        BankSelection::Armed(vec![a.min(b), a.max(b)])
    );
    let placed = edit(
        &t.app,
        &at,
        &two,
        63,
        AlignEdit::PlaceSelection {
            card: card(&two, "ἐπίγνωσιν"),
        },
    )
    .await
    .unwrap();
    assert_eq!(placed.placed, view.placed + 2);
    assert_eq!(placed.selection, BankSelection::None);
}

/// A click made on a view that is not the newest is refused, never applied to a verse the
/// person no longer sees; and an aligner that was replaced answers the same.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_edit_of_an_old_view_is_stale_not_applied() {
    let (_dir, t, _) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    let after = edit(
        &t.app,
        &at,
        &view,
        61,
        AlignEdit::ClickBank {
            index: 0,
            click: BankClick::Plain,
        },
    )
    .await
    .unwrap();
    let old = edit(&t.app, &at, &view, 62, AlignEdit::RejectAll).await;
    assert_eq!(old.unwrap_err(), ApiError::StaleSession);
    let second = ready(&t.app, &at, 1, 2).await;
    let replaced = edit(&t.app, &at, &after, 63, AlignEdit::RejectAll).await;
    assert_eq!(replaced.unwrap_err(), ApiError::StaleSession);
    assert_ne!(second.aligner, view.aligner);
}

/// `align/open` on the verse whose session is live returns that session, edits and all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_the_open_verse_returns_the_session() {
    let (_dir, t, _) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    let placed = place(&t.app, &at, &view, 61, "fe", "πίστιν").await;
    let again = ready(&t.app, &at, 1, 1).await;
    assert_eq!(again, placed);
}

/// The links are saved by the server 2 s after the last change, as one command; the state goes
/// Unsaved, Saving, Saved on the event stream and in the view.
#[tokio::test(start_paused = false)]
async fn the_links_are_saved_two_seconds_after_the_last_change() {
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    backend.script(&folder(SAMPLE), Scripted::Written).unwrap();
    let placed = place(&t.app, &at, &view, 61, "fe", "πίστιν").await;
    assert_eq!(placed.save, AlignSave::Unsaved);
    tokio::time::pause();
    // Another change inside the two seconds restarts them.
    tokio::time::advance(Duration::from_millis(1500)).await;
    assert!(saved_links(&backend).is_empty(), "not yet");
    let more = place(&t.app, &at, &placed, 63, "verdad", "ἀληθείας").await;
    tokio::time::advance(Duration::from_millis(1500)).await;
    assert!(saved_links(&backend).is_empty(), "the timer restarted");
    tokio::time::advance(Duration::from_millis(600)).await;
    // Let the actor run.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    let sent = saved_links(&backend);
    assert_eq!(sent.len(), 1, "one command for both changes");
    let on = sent[0]
        .alignments
        .iter()
        .find(|g| g.top_words.iter().any(|w| w.word == "πίστιν"))
        .unwrap();
    assert!(on.bottom_words.iter().any(|w| w.word == "fe"));
    assert_eq!(
        saves(&t.app),
        [AlignSave::Unsaved, AlignSave::Saving, AlignSave::Saved]
    );
    let now = ready(&t.app, &at, 1, 1).await;
    assert_eq!(now.save, AlignSave::Saved);
    assert_eq!(now.cards, more.cards);
}

/// A save that fails leaves the links where they are, unsaved; the next flush tries again and
/// the links are stored as the person made them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_save_that_fails_keeps_the_links_and_is_tried_again() {
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    let placed = place(&t.app, &at, &view, 61, "fe", "πίστιν").await;
    backend
        .script(
            &folder(SAMPLE),
            Scripted::NotWritten("The disk is full.".into()),
        )
        .unwrap();
    let failed = flush(&t.app, &at, 70).await;
    assert_eq!(
        failed,
        AlignSave::Failed(NotSaved::Disk("The disk is full.".into()))
    );
    let still = ready(&t.app, &at, 1, 1).await;
    assert_eq!(still.save, failed, "the view says so after a gap too");
    assert_eq!(still.cards, placed.cards);
    backend.script(&folder(SAMPLE), Scripted::Written).unwrap();
    assert_eq!(flush(&t.app, &at, 71).await, AlignSave::Saved);
    let links = saved_links(&backend);
    assert_eq!(links.len(), 2, "the second try sent the links again");
    assert_eq!(links[0], links[1]);
    assert_eq!(ready(&t.app, &at, 1, 1).await.save, AlignSave::Saved);
}

/// Another verse replaces the open one after its links are saved; links that cannot be saved
/// keep the person on the verse.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn leaving_a_verse_saves_it_first_and_a_failure_keeps_the_person_there() {
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    place(&t.app, &at, &view, 61, "fe", "πίστιν").await;
    backend
        .script(
            &folder(SAMPLE),
            Scripted::NotWritten("The disk is full.".into()),
        )
        .unwrap();
    let refused = t
        .app
        .request(&Open {
            at: at.clone(),
            request: request(80),
            book: BookId::Titus,
            verse: Some(OpenVerse {
                chapter: 1,
                verse: 2,
            }),
        })
        .await
        .unwrap();
    assert!(
        matches!(refused, Reply::Err(ApiError::NotSaved(NotSaved::Disk(_)))),
        "{refused:?}"
    );
    assert_eq!(ready(&t.app, &at, 1, 1).await.cards.len(), view.cards.len());
    backend.script(&folder(SAMPLE), Scripted::Written).unwrap();
    let second = ready(&t.app, &at, 1, 2).await;
    assert_eq!(second.place.label(), "1:2");
    assert_eq!(saved_links(&backend).len(), 2);
}

/// Going Home saves the links before the leave checkpoint (and so does the quit, which closes
/// every session the same way).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn going_home_saves_the_links_first() {
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    place(&t.app, &at, &view, 61, "fe", "πίστιν").await;
    backend.script(&folder(SAMPLE), Scripted::Written).unwrap();
    ok(
        &t.app,
        &CloseProject {
            request: request(90),
            at,
        },
    )
    .await;
    t.app.quit().await;
    assert_eq!(saved_links(&backend).len(), 1);
}

/// A closed aligner is saved first, and a failing close leaves it open, unsaved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_aligner_saves_and_a_failure_leaves_it_open() {
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    let placed = place(&t.app, &at, &view, 61, "fe", "πίστιν").await;
    backend
        .script(
            &folder(SAMPLE),
            Scripted::NotWritten("The disk is full.".into()),
        )
        .unwrap();
    let close = Close {
        at: at.clone(),
        request: request(91),
        aligner: view.aligner,
    };
    let failed = ok(&t.app, &close).await;
    assert!(matches!(failed, AlignSave::Failed(_)));
    assert_eq!(ready(&t.app, &at, 1, 1).await.cards, placed.cards);
    backend.script(&folder(SAMPLE), Scripted::Written).unwrap();
    let again = Close {
        request: request(92),
        ..close
    };
    assert_eq!(ok(&t.app, &again).await, AlignSave::Saved);
    // The aligner is gone: its edits are stale.
    let late = edit(&t.app, &at, &placed, 93, AlignEdit::RejectAll).await;
    assert_eq!(late.unwrap_err(), ApiError::StaleSession);
}

/// A teammate's change to the verse reaches the open project: the verse then opens with the
/// notice, the links that followed their words kept, and the person's first edit clears it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_verse_changed_by_a_teammate_opens_with_the_notice() {
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let key = RecordKey::Verse(
        BookId::Titus,
        ChapterNo::new(1).unwrap(),
        VerseNo::new(1).unwrap(),
    );
    let changes = backend
        .teammates_edit_verse(
            &folder(SAMPLE),
            &key,
            &["Pablo, siervo de Dios y apóstol de Jesucristo, por la fe"],
        )
        .unwrap();
    backend.teammate_edits(&folder(SAMPLE), changes).unwrap();
    let view = ready(&t.app, &at, 1, 1).await;
    assert_eq!(view.freshness, Freshness::Stale);
    assert_eq!(view.status.state, RailState::Invalid);
    let kept = edit(
        &t.app,
        &at,
        &view,
        61,
        AlignEdit::ClickBank {
            index: 0,
            click: BankClick::Plain,
        },
    )
    .await
    .unwrap();
    assert_eq!(kept.freshness, Freshness::Stale, "a selection is no edit");
    let kept = t
        .app
        .request(&KeepReflow {
            at,
            request: request(62),
            aligner: view.aligner,
            seq: kept.seq,
        })
        .await
        .unwrap();
    let Reply::Ok(kept) = kept else {
        panic!("{kept:?}")
    };
    assert_eq!(kept.freshness, Freshness::Current);
    assert_eq!(kept.save, AlignSave::Unsaved);
}

/// Mark valid is refused while suggestions stand and lights the button otherwise; any later
/// edit takes it back (D73).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mark_valid_lights_the_button_and_an_edit_takes_it_back() {
    let (_dir, t, _) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    let view = ready(&t.app, &at, 1, 1).await;
    let marked = edit(&t.app, &at, &view, 61, AlignEdit::MarkValid)
        .await
        .unwrap();
    assert!(marked.done);
    assert_eq!(marked.status.state, RailState::Valid);
    assert_eq!(marked.save, AlignSave::Unsaved);
    let edited = place(&t.app, &at, &marked, 62, "fe", "πίστιν").await;
    assert!(!edited.done);
}
