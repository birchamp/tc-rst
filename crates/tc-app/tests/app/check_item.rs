//! The Check screen's item detail and marks (unit B5b) through `App::request`, on the parity
//! scenarios with their resources installed: what a check shows (the note, the panes, the draft
//! words, the decision), that reading writes nothing, and the marks on the real store and on
//! scripted outcomes.

use std::collections::BTreeSet;
use std::sync::Arc;

use proptest::prelude::*;
use tc_api::check::{
    CheckArticleRead, CheckItemView, CheckListRef, CheckSessionView, CheckSources, CheckUnit,
    Decide, DecideEdit, DecisionStatus, GatewayText, GatewayView, Item, Open, OriginalView,
    RailRow, RailRows, RailSince, Rows, RowsView, Testament, Tool,
};
use tc_api::editor::{Draft, DraftField, SaveVerse};
use tc_api::helps::ArticleView;
use tc_api::ids::{Scope, WallMs};
use tc_api::model::selection::drag_range;
use tc_api::session::{DraftKind, Drafts, KeepDraft, OfferedDrafts};
use tc_api::write::Stored;
use tc_api::{ApiError, NotSaved, Reply};
use tc_app::App;
use tc_app::StoreChoice;
use tc_app::backend::memory::{MemoryBackend, Scripted};
use tc_app::backend::scenario::install_resources_into;
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::config::ProjectsFolder;
use tc_check::selection::{TargetText, from_indices, indices, target_words};
use tc_core::BookId;
use tc_store::FakePlatform;
use tc_store::fault::FaultFs;

use super::common::{Parts, TestApp, folder, open_progress, post, rid, tit};
use super::scenario::scenario;
use super::understand::{ok, opened, start};
use crate::store_world::{APP, backend_over};

fn list(tool: Tool) -> CheckListRef {
    CheckListRef {
        tool,
        unit: CheckUnit::Book(BookId::Titus),
    }
}

async fn session(app: &App, at: &Scope, tool: Tool) -> CheckSessionView {
    ok(
        app,
        &Open {
            at: at.clone(),
            list: list(tool),
            since: None,
        },
    )
    .await
}

fn rows(session: &CheckSessionView) -> &[RailRow] {
    match &session.rows {
        RailRows::Full(rows) => rows,
        other => panic!("{other:?}"),
    }
}

async fn item(app: &App, at: &Scope, tool: Tool, index: u32) -> CheckItemView {
    ok(
        app,
        &Item {
            at: at.clone(),
            list: list(tool),
            index,
        },
    )
    .await
}

fn the_first(rows: &[RailRow], title: &str) -> u32 {
    rows.iter()
        .find(|r| r.title.starts_with(title))
        .unwrap_or_else(|| panic!("no check titled {title}"))
        .index
}

/// `en-check-tn-session`: item 1 shows its quote as the ULT renders it, the note, the Academy
/// article, the Greek verse, the ULT verse with the quote's words lit, and the es-419 draft.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_note_shows_its_panes_its_article_and_the_draft_words() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(view.place.to_string(), "TIT 1:1");
    assert!(view.title.starts_with("for the faith of the chosen people"));
    let note: String = view.note.iter().map(|b| b.text()).collect();
    assert!(note.contains("abstract nouns"), "{note}");
    assert_eq!(view.article.title.as_deref(), Some("Abstract Nouns"));
    let CheckSources::Verse { original, gateway } = &view.sources else {
        panic!("{:?}", view.sources);
    };
    match original {
        OriginalView::Ready {
            testament: Testament::Nt,
            verse: Some(verse),
        } => {
            assert!(verse.tokens.concat().starts_with("Παῦλος"), "{verse:?}");
            // Owner ruling Q39 (tC4 #840): the Greek row marks the note's quote.
            let marked: Vec<&str> = verse
                .lit
                .iter()
                .map(|&i| verse.tokens[i as usize].as_str())
                .collect();
            assert_eq!(
                marked,
                [
                    "κατὰ",
                    "πίστιν",
                    "ἐκλεκτῶν",
                    "Θεοῦ",
                    "καὶ",
                    "ἐπίγνωσιν",
                    "ἀληθείας"
                ]
            );
        }
        other => panic!("{other:?}"),
    }
    let GatewayView::Pane {
        pane,
        text: GatewayText::Verse { tokens, lit },
    } = gateway
    else {
        panic!("{gateway:?}");
    };
    assert_eq!(pane, "ult");
    let lit_words: String = lit.iter().map(|&i| tokens[i as usize].as_str()).collect();
    assert!(lit_words.starts_with("forthefaith"), "{lit_words}");
    assert_eq!(view.target.words[..3], ["Pablo", "siervo", "de"]);
    assert!(view.target.selected.is_empty());
    assert!(view.target.verse.is_some());
    assert_eq!(view.decision.value.status, None);
    assert!(view.notices.is_empty(), "{:?}", view.notices);
}

/// `en-check-tw-article`: a decided word check shows its selected word, and its article is
/// read from the resource the list derives from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decided_word_check_marks_its_selected_word_and_serves_its_article() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationWords).await;
    let decided = rows(&open)
        .iter()
        .find(|r| r.title == "of God" && r.flags.decided)
        .expect("the decided “of God”")
        .index;
    let view = item(&t.app, &at, Tool::TranslationWords, decided).await;
    assert_eq!(view.decision.value.status, Some(DecisionStatus::Valid));
    let words: Vec<&str> = view
        .target
        .selected
        .iter()
        .map(|&i| view.target.words[i as usize].as_str())
        .collect();
    assert_eq!(words, ["Dios"]);
    assert_eq!(view.article.title.as_deref(), Some("God"));
    let article = ok(
        &t.app,
        &CheckArticleRead {
            at,
            list: list(Tool::TranslationWords),
            index: decided,
        },
    )
    .await;
    let ArticleView::Found { title, blocks, .. } = article else {
        panic!("{article:?}");
    };
    assert_eq!(title, "God");
    assert!(blocks.len() > 5);
}

/// tC4 #619: the Check drawer draws the blocks the helps panel draws: `check/article` answers
/// what `helps/article` answers for the same link, and the check's own title names it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_checks_article_is_the_helps_blocks() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    for (tool, title) in [
        (Tool::TranslationNotes, "Abstract Nouns"),
        (Tool::TranslationWords, "Paul, Saul"),
    ] {
        let open = session(&t.app, &at, tool).await;
        let first = rows(&open)[0].index;
        let view = item(&t.app, &at, tool, first).await;
        let read = ok(
            &t.app,
            &CheckArticleRead {
                at: at.clone(),
                list: list(tool),
                index: first,
            },
        )
        .await;
        let helps = ok(
            &t.app,
            &tc_api::helps::Article {
                at: at.clone(),
                book: BookId::Titus,
                link: view.article.link.clone(),
            },
        )
        .await;
        assert_eq!(read, helps);
        let ArticleView::Found {
            title: found,
            path,
            blocks,
            ..
        } = read
        else {
            panic!("{read:?}");
        };
        assert_eq!(found, title);
        assert_eq!(view.article.title.as_deref(), Some(title));
        assert!(!path.is_empty() && !blocks.is_empty());
        let text: String = blocks.iter().map(|b| b.text()).collect();
        assert!(
            !text.contains("__") && !text.contains("\n>"),
            "no raw marks: {text:.80}"
        );
    }
}

/// tC4 #501: showing every check of a list writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn showing_checks_writes_nothing() {
    let state = scenario("en-pins");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let backend = Arc::new(backend);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let store = backend.clone();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(store);
    })
    .await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    for row in rows(&open).iter().take(30) {
        item(&t.app, &at, Tool::TranslationNotes, row.index).await;
    }
    assert!(backend.submitted(&folder("sample_burrito")).is_empty());
}

/// A check the list does not have is a bad request, never another check.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_index_past_the_list_is_refused() {
    let (_dir, t) = start("en-pins").await;
    let at = opened(&t.app).await.session.at;
    let reply = t
        .app
        .request(&Item {
            at,
            list: list(Tool::TranslationNotes),
            index: 9_999,
        })
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::BadRequest { .. })),
        "{reply:?}"
    );
}

// ---- marks, on the real store ---------------------------------------------------------------

const SAMPLE: &str = "sample_burrito";

/// The app over the `en-pins` scenario written through tc-store, with its resources installed.
async fn on_the_store() -> (tempfile::TempDir, TestApp, FaultFs) {
    let fs = FaultFs::new();
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let (store, _) = ScenarioStore::write(backend, scenario("en-pins")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(
        &dir.path().join("app-data"),
        &scenario("en-pins").resources().unwrap(),
    )
    .unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(Arc::new(store));
    })
    .await;
    (dir, t, fs)
}

async fn open_on_store(t: &TestApp) -> Scope {
    t.app
        .sessions()
        .open_screen(&folder(SAMPLE), open_progress())
        .await
        .unwrap()
        .at
}

fn decide(at: &Scope, request: u8, view: &CheckItemView, edit: DecideEdit) -> Decide {
    Decide {
        at: at.clone(),
        request: rid(request),
        list: list(Tool::TranslationNotes),
        place: view.place.clone(),
        check_id: view.check_id.clone(),
        decision: view.decision.token.clone(),
        verse: view.target.verse.clone(),
        edit,
    }
}

/// Mark valid with two tapped words stores them; the rail's row and the progress follow, and
/// the item read again shows the words, the status and the new tokens.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mark_valid_stores_the_tapped_words_and_the_rail_follows() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let tapped: Vec<u32> = vec![11, 12, 13];
    let ack = post(
        &t.app,
        &decide(
            &at,
            1,
            &view,
            DecideEdit::Valid {
                words: tapped.clone(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
    assert_eq!(ack.rows.len(), 1, "the check's row");
    assert!(ack.rows[0].row.flags.decided);
    assert_eq!(ack.rows[0].row.index, first);
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(now.decision.value.status, Some(DecisionStatus::Valid));
    assert_eq!(now.target.selected, tapped);
    assert_ne!(now.decision.token, view.decision.token, "a new version");
    let after = session(&t.app, &at, Tool::TranslationNotes).await;
    assert_eq!(after.progress.decided, open.progress.decided + 1);
}

/// Mark valid with no word is "nothing to select"; Invalid clears the words; To do keeps the
/// status apart from the words, as tC4's buttons do.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_triage_buttons_follow_tc4s_rules() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let read = || item(&t.app, &at, Tool::TranslationNotes, first);
    let view = read().await;
    post(
        &t.app,
        &decide(&at, 2, &view, DecideEdit::Valid { words: vec![] }),
    )
    .await
    .unwrap();
    let valid = read().await;
    assert_eq!(valid.decision.value.status, Some(DecisionStatus::Valid));
    assert!(valid.decision.value.nothing_to_select);
    assert!(valid.target.selected.is_empty());
    post(&t.app, &decide(&at, 3, &valid, DecideEdit::Invalid))
        .await
        .unwrap();
    let invalid = read().await;
    assert_eq!(invalid.decision.value.status, Some(DecisionStatus::Invalid));
    assert!(!invalid.decision.value.nothing_to_select);
    post(&t.app, &decide(&at, 4, &invalid, DecideEdit::Todo))
        .await
        .unwrap();
    assert_eq!(
        read().await.decision.value.status,
        Some(DecisionStatus::Todo)
    );
}

/// The server's half of `check_comment_saved_on_leaving_the_item` (the journey in
/// `parity/app/journeys/check-detail.test.mjs` types a comment, goes to the next check and finds
/// it stored on the first): a comment saved with the token of the item shown comes back with the
/// item, a bookmark and the comment change nothing of the decision, and an empty comment removes
/// it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_comment_and_a_bookmark_leave_the_decision_as_it_was() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    post(
        &t.app,
        &decide(&at, 5, &view, DecideEdit::Valid { words: vec![0] }),
    )
    .await
    .unwrap();
    let valid = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let ack = post(
        &t.app,
        &decide(
            &at,
            6,
            &valid,
            DecideEdit::Comment {
                text: "ask Pablo".into(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(ack.saved[0].text, "ask Pablo");
    let commented = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(
        commented.decision.value.comment.as_deref(),
        Some("ask Pablo")
    );
    assert_eq!(commented.decision.value.status, Some(DecisionStatus::Valid));
    assert_eq!(commented.target.selected, [0]);
    post(
        &t.app,
        &decide(&at, 7, &commented, DecideEdit::Bookmark { on: true }),
    )
    .await
    .unwrap();
    let marked = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert!(marked.decision.value.bookmarked);
    assert_eq!(marked.decision.value.comment.as_deref(), Some("ask Pablo"));
    post(
        &t.app,
        &decide(
            &at,
            8,
            &marked,
            DecideEdit::Comment {
                text: String::new(),
            },
        ),
    )
    .await
    .unwrap();
    let cleared = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(cleared.decision.value.comment, None);
    assert!(cleared.decision.value.bookmarked);
    // A bookmark or a comment is not a decision: progress is what the one valid mark made.
    let after = session(&t.app, &at, Tool::TranslationNotes).await;
    assert_eq!(after.progress.decided, open.progress.decided + 1);
}

/// The verse changed after the detail showed it: the words the person tapped are not those
/// words any more, so the mark is refused and nothing is written on the wrong text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn words_tapped_on_a_verse_that_changed_are_not_saved() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let minted = t
        .app
        .sessions()
        .reader(&at)
        .unwrap()
        .read()
        .mint(&tit(1, 1), None)
        .unwrap();
    post(
        &t.app,
        &SaveVerse {
            at: at.clone(),
            request: rid(9),
            shown: tc_app::wire::token::shown(&minted).unwrap(),
            words: "Otras palabras del todo.".into(),
        },
    )
    .await
    .unwrap();
    let reply = t
        .app
        .request(&decide(
            &at,
            10,
            &view,
            DecideEdit::Valid { words: vec![0, 1] },
        ))
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::NotSaved(NotSaved::LookAgain))),
        "{reply:?}"
    );
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(now.decision.value.status, None, "nothing was recorded");
}

/// A mark for a check the list no longer has at that place is `LookAgain`, not another check.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mark_for_another_check_is_look_again() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let mut mark = decide(&at, 11, &view, DecideEdit::Todo);
    mark.check_id = "zzzz".into();
    let reply = t.app.request(&mark).await.unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::NotSaved(NotSaved::LookAgain))),
        "{reply:?}"
    );
}

// ---- the check comment's server draft (C4, A-10) --------------------------------------------

/// The next launch over the files an ended app left: a new app on `fs`'s crash image, with the
/// same app data (the installed resources).
async fn relaunch(dir: &tempfile::TempDir, fs: &FaultFs) -> TestApp {
    let backend = backend_over(
        &fs.crash(false),
        APP,
        FakePlatform::default(),
        Arc::default(),
    );
    let projects = dir.path().join("projects");
    TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(Arc::new(backend));
    })
    .await
}

/// A check comment's draft, as the comment editor sends it between saves (`editor/draft`).
fn comment_draft(t: &TestApp, at: &Scope, over: &tc_api::ShownToken, text: &str) -> Draft {
    Draft {
        at: at.clone(),
        request: rid(40),
        instance: t.app.instance(),
        field: DraftField::CheckComment,
        shown: vec![over.clone()],
        text: text.to_owned(),
        changed: WallMs(1_792_000_000_000),
    }
}

async fn offers(t: &TestApp, at: &Scope, request: u8) -> OfferedDrafts {
    post(
        &t.app,
        &Drafts {
            at: at.clone(),
            request: rid(request),
            buffered: Vec::new(),
            mirror: false,
        },
    )
    .await
    .unwrap()
}

/// RT-10, Q-25: the checker marks a check valid, types a comment and the app ends before the
/// comment is saved (a crash, a power cut, a kill). The next open offers the comment, and
/// *Keep* stores it on the decision the person made, beside the mark, with no conflict
/// against the person's own mark.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_comment_typed_before_the_app_ended_is_offered_and_kept_beside_the_mark() {
    let (dir, t, fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let ack = post(
        &t.app,
        &decide(&at, 1, &view, DecideEdit::Valid { words: vec![0] }),
    )
    .await
    .unwrap();
    let marked = ack.tokens[0].token.clone();
    post(
        &t.app,
        &comment_draft(&t, &at, &marked, "ask Pablo about faith"),
    )
    .await
    .unwrap();
    drop(t);

    let t = relaunch(&dir, &fs).await;
    let at = open_on_store(&t).await;
    let offered = offers(&t, &at, 41).await;
    let offer = offered
        .offers
        .iter()
        .find(|o| o.kind == DraftKind::CheckComment)
        .unwrap_or_else(|| panic!("the comment is offered: {offered:?}"));
    assert_eq!(offer.text, "ask Pablo about faith");
    assert_eq!(offer.place.as_str(), "TIT 1:1");
    let kept = post(
        &t.app,
        &KeepDraft {
            at: at.clone(),
            request: rid(42),
            kind: offer.kind,
            shown: offer.shown.clone(),
            text: offer.text.clone(),
            discard: offer.settles.stored.clone(),
        },
    )
    .await
    .unwrap();
    assert!(kept.new_conflicts.is_empty(), "{kept:?}");
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(
        now.decision.value.comment.as_deref(),
        Some("ask Pablo about faith")
    );
    assert_eq!(now.decision.value.status, Some(DecisionStatus::Valid));
    assert_eq!(now.target.selected, [0]);
    let again = offers(&t, &at, 43).await;
    assert!(
        again.offers.is_empty(),
        "answered once: {again:?} after {offered:?}"
    );
}

/// The other order: the checker types a comment, then bookmarks the check, and the app ends
/// a while later, before the comment is saved. The bookmark saves no comment, so the comment's
/// draft outlives it; the next open offers the comment and *Keep* stores it beside the mark and
/// the bookmark, with no conflict.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_comment_typed_before_a_bookmark_outlives_it_and_is_kept_beside_it() {
    let (dir, t, fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    post(
        &t.app,
        &decide(&at, 1, &view, DecideEdit::Valid { words: vec![0] }),
    )
    .await
    .unwrap();
    let marked = item(&t.app, &at, Tool::TranslationNotes, first).await;
    post(
        &t.app,
        &comment_draft(&t, &at, &marked.decision.token, "ask Pablo about faith"),
    )
    .await
    .unwrap();
    post(
        &t.app,
        &decide(&at, 2, &marked, DecideEdit::Bookmark { on: true }),
    )
    .await
    .unwrap();
    // Past the draft file's next write.
    tokio::time::sleep(std::time::Duration::from_millis(1_500)).await;
    drop(t);

    let t = relaunch(&dir, &fs).await;
    let at = open_on_store(&t).await;
    let offered = offers(&t, &at, 41).await;
    let offer = offered
        .offers
        .iter()
        .find(|o| o.kind == DraftKind::CheckComment)
        .unwrap_or_else(|| panic!("the comment is offered: {offered:?}"));
    assert_eq!(offer.text, "ask Pablo about faith");
    let kept = post(
        &t.app,
        &KeepDraft {
            at: at.clone(),
            request: rid(42),
            kind: offer.kind,
            shown: offer.shown.clone(),
            text: offer.text.clone(),
            discard: offer.settles.stored.clone(),
        },
    )
    .await
    .unwrap();
    assert!(kept.new_conflicts.is_empty(), "{kept:?}");
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(
        now.decision.value.comment.as_deref(),
        Some("ask Pablo about faith")
    );
    assert_eq!(now.decision.value.status, Some(DecisionStatus::Valid));
    assert!(now.decision.value.bookmarked);
    assert_eq!(now.decision.marker, None, "no conflict");
    assert!(offers(&t, &at, 43).await.offers.is_empty(), "answered once");
}

/// A comment typed on a check not marked yet, and the app ended before it was saved: *Keep*
/// stores it as the check's first decision, as Done would have, with no conflict, once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_comment_typed_on_an_undecided_check_is_kept_as_its_first_decision() {
    let (dir, t, fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(view.decision.value.status, None);
    post(
        &t.app,
        &comment_draft(&t, &at, &view.decision.token, "ask Pablo about faith"),
    )
    .await
    .unwrap();
    drop(t);

    let t = relaunch(&dir, &fs).await;
    let at = open_on_store(&t).await;
    let offered = offers(&t, &at, 41).await;
    let offer = offered
        .offers
        .iter()
        .find(|o| o.kind == DraftKind::CheckComment)
        .unwrap_or_else(|| panic!("the comment is offered: {offered:?}"));
    let kept = post(
        &t.app,
        &KeepDraft {
            at: at.clone(),
            request: rid(42),
            kind: offer.kind,
            shown: offer.shown.clone(),
            text: offer.text.clone(),
            discard: offer.settles.stored.clone(),
        },
    )
    .await
    .unwrap();
    assert!(kept.new_conflicts.is_empty(), "{kept:?}");
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(
        now.decision.value.comment.as_deref(),
        Some("ask Pablo about faith")
    );
    assert_eq!(now.decision.value.status, None, "a comment is not a mark");
    assert_eq!(now.decision.marker, None);
    let after = session(&t.app, &at, Tool::TranslationNotes).await;
    assert_eq!(after.progress.decided, open.progress.decided);
    assert!(offers(&t, &at, 43).await.offers.is_empty(), "answered once");
}

/// A comment typed on a check not marked yet, then a mark, and the app ended before the page
/// drafted the comment again over the mark: the offer still names the check with no decision,
/// and *Keep* stores the comment beside the person's own mark, with no conflict, once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_comment_typed_before_the_first_mark_is_kept_beside_it() {
    let (dir, t, fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(view.decision.value.status, None);
    post(
        &t.app,
        &comment_draft(&t, &at, &view.decision.token, "ask Pablo about faith"),
    )
    .await
    .unwrap();
    post(
        &t.app,
        &decide(&at, 1, &view, DecideEdit::Valid { words: vec![0] }),
    )
    .await
    .unwrap();
    drop(t);

    let t = relaunch(&dir, &fs).await;
    let at = open_on_store(&t).await;
    let offered = offers(&t, &at, 41).await;
    let offer = offered
        .offers
        .iter()
        .find(|o| o.kind == DraftKind::CheckComment)
        .unwrap_or_else(|| panic!("the comment is offered: {offered:?}"));
    let kept = post(
        &t.app,
        &KeepDraft {
            at: at.clone(),
            request: rid(42),
            kind: offer.kind,
            shown: offer.shown.clone(),
            text: offer.text.clone(),
            discard: offer.settles.stored.clone(),
        },
    )
    .await
    .unwrap();
    assert!(kept.new_conflicts.is_empty(), "{kept:?}");
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(
        now.decision.value.comment.as_deref(),
        Some("ask Pablo about faith")
    );
    assert_eq!(now.decision.value.status, Some(DecisionStatus::Valid));
    assert_eq!(now.decision.marker, None, "no conflict");
    let after = session(&t.app, &at, Tool::TranslationNotes).await;
    let row = &rows(&after)[first as usize];
    assert!(row.flags.commented, "{row:?}");
    assert_eq!(row.marker, None);
    assert!(offers(&t, &at, 43).await.offers.is_empty(), "answered once");
}

/// A comment whose save landed leaves no draft behind: the next open after the app ended
/// offers nothing, so a saved comment is never asked about again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_saved_check_comment_leaves_no_draft_to_offer() {
    let (dir, t, fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    post(
        &t.app,
        &comment_draft(&t, &at, &view.decision.token, "ask Pablo"),
    )
    .await
    .unwrap();
    post(
        &t.app,
        &decide(
            &at,
            2,
            &view,
            DecideEdit::Comment {
                text: "ask Pablo".into(),
            },
        ),
    )
    .await
    .unwrap();
    drop(t);

    let t = relaunch(&dir, &fs).await;
    let at = open_on_store(&t).await;
    assert!(offers(&t, &at, 41).await.offers.is_empty());
    let now = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert_eq!(now.decision.value.comment.as_deref(), Some("ask Pablo"));
}

/// A check comment's draft names its decision's record; any other token is refused, as for
/// every editor draft.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_check_comment_draft_over_another_record_is_refused() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    let verse = view.target.verse.clone().expect("the verse is drafted");
    let reply = t
        .app
        .request(&comment_draft(&t, &at, &verse, "on the verse"))
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::NotSaved(NotSaved::BadToken))),
        "{reply:?}"
    );
}

// ---- re-assessment after changes (C4) --------------------------------------------------------

/// The rail a page holds: the first open's rows, patched with each `check/rows { since }`.
struct HeldRail {
    tool: Tool,
    rows: Vec<RailRow>,
    since: RailSince,
}

impl HeldRail {
    async fn open(app: &App, at: &Scope, tool: Tool) -> HeldRail {
        let view = session(app, at, tool).await;
        HeldRail {
            tool,
            rows: rows(&view).to_vec(),
            since: RailSince {
                session: view.session,
                view: view.view,
            },
        }
    }

    /// What the page asks after a `Changed` that names the list.
    async fn refresh(&mut self, app: &App, at: &Scope) {
        match ok(
            app,
            &Rows {
                at: at.clone(),
                list: list(self.tool),
                since: self.since,
            },
        )
        .await
        {
            RowsView::Changed { view, rows, .. } => {
                assert!(tc_api::model::rail::patch(&mut self.rows, &rows));
                self.since.view = view;
            }
            RowsView::Reopen => panic!("the list was not derived again"),
        }
    }

    fn by_index(&self) -> std::collections::BTreeMap<u32, RailRow> {
        self.rows.iter().map(|r| (r.index, r.clone())).collect()
    }
}

/// The rails of both tools as a fresh open of the project derives and assesses them.
async fn fresh_rails(t: &TestApp, at: &Scope) -> (Scope, Vec<HeldRail>) {
    t.app.sessions().leave_screen(at).unwrap();
    t.app.sessions().settled(&folder(SAMPLE)).await;
    let at = open_on_store(t).await;
    let mut rails = Vec::new();
    for tool in [Tool::TranslationNotes, Tool::TranslationWords] {
        rails.push(HeldRail::open(&t.app, &at, tool).await);
    }
    (at, rails)
}

/// A verse saved with other words, as Translate saves it.
async fn save_verse(t: &TestApp, at: &Scope, request: u8, verse: &tc_api::ShownToken, words: &str) {
    post(
        &t.app,
        &SaveVerse {
            at: at.clone(),
            request: rid(request),
            shown: verse.clone(),
            words: words.to_owned(),
        },
    )
    .await
    .unwrap();
}

/// PERF:79-80, AD17: a mark re-assesses its own check and a verse edit the decided checks of
/// its chapter, and the rails the page holds, patched row by row, equal the rails a fresh open
/// derives and assesses: no row is left stale and none changes that should not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn marks_and_verse_edits_reassess_what_they_reach_and_the_rails_equal_a_fresh_open() {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let mut notes = HeldRail::open(&t.app, &at, Tool::TranslationNotes).await;
    let mut words = HeldRail::open(&t.app, &at, Tool::TranslationWords).await;
    let first = the_first(&notes.rows, "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;

    let before = t.app.counters();
    post(
        &t.app,
        &decide(
            &at,
            20,
            &view,
            DecideEdit::Valid {
                words: vec![11, 12],
            },
        ),
    )
    .await
    .unwrap();
    notes.refresh(&t.app, &at).await;
    let spent = t.app.counters().since(&before);
    assert_eq!(spent.checks_assessed, 1, "the mark's own check: {spent:?}");
    assert_eq!(spent.items_derived, 0);
    assert_eq!(spent.rows_serialized, 2, "the Ack's row and the refresh's");

    // More marks in the chapter: a comment, a bookmark, Invalid and To do on others.
    let undecided: Vec<u32> = notes
        .rows
        .iter()
        .filter(|r| !r.flags.decided && r.index != first)
        .map(|r| r.index)
        .take(3)
        .collect();
    let edits = [
        DecideEdit::Invalid,
        DecideEdit::Todo,
        DecideEdit::Comment {
            text: "ask Pablo".into(),
        },
    ];
    for (n, (index, edit)) in undecided.iter().zip(edits).enumerate() {
        let view = item(&t.app, &at, Tool::TranslationNotes, *index).await;
        post(&t.app, &decide(&at, 21 + n as u8, &view, edit))
            .await
            .unwrap();
        notes.refresh(&t.app, &at).await;
    }
    let marked = item(&t.app, &at, Tool::TranslationNotes, first).await;
    post(
        &t.app,
        &decide(&at, 25, &marked, DecideEdit::Bookmark { on: true }),
    )
    .await
    .unwrap();

    // The verse the first mark selected words in loses them: its decided checks are
    // re-assessed, and the mark shows invalidated.
    let verse = marked.target.verse.clone().expect("the verse is drafted");
    let before = t.app.counters();
    save_verse(&t, &at, 26, &verse, "Pablo, siervo de Dios.").await;
    notes.refresh(&t.app, &at).await;
    words.refresh(&t.app, &at).await;
    let spent = t.app.counters().since(&before);
    assert_eq!(spent.items_derived, 0);
    assert!(
        spent.checks_assessed > 0 && spent.checks_assessed < 100,
        "the decided checks of chapter 1 of both tools: {spent:?}"
    );
    let invalidated = item(&t.app, &at, Tool::TranslationNotes, first).await;
    assert!(invalidated.decision.value.invalidated);

    let (_, fresh) = fresh_rails(&t, &at).await;
    assert_eq!(notes.by_index(), fresh[0].by_index());
    assert_eq!(words.by_index(), fresh[1].by_index());
}

/// One act of a generated checking session.
#[derive(Debug, Clone)]
enum Act {
    /// A mark, a comment or a bookmark on the check at `pick` of a tool's rail.
    Mark { notes: bool, pick: usize, edit: u8 },
    /// The verse of the check at `pick` saved with other words, keeping or dropping the words
    /// the decisions there selected.
    Verse {
        notes: bool,
        pick: usize,
        keep: bool,
    },
    /// The page asks for the rows changed since its view.
    Refresh,
}

fn act() -> impl Strategy<Value = Act> {
    prop_oneof![
        (any::<bool>(), 0usize..400, 0u8..6).prop_map(|(notes, pick, edit)| Act::Mark {
            notes,
            pick,
            edit
        }),
        (any::<bool>(), 0usize..400, any::<bool>()).prop_map(|(notes, pick, keep)| Act::Verse {
            notes,
            pick,
            keep
        }),
        Just(Act::Refresh),
    ]
}

async fn play(acts: Vec<Act>) {
    let (_dir, t, _fs) = on_the_store().await;
    let at = open_on_store(&t).await;
    let mut rails = vec![
        HeldRail::open(&t.app, &at, Tool::TranslationNotes).await,
        HeldRail::open(&t.app, &at, Tool::TranslationWords).await,
    ];
    for (n, act) in acts.into_iter().enumerate() {
        let request = 60 + n as u8;
        match act {
            Act::Mark { notes, pick, edit } => {
                let rail = &rails[usize::from(!notes)];
                let index = rail.rows[pick % rail.rows.len()].index;
                let view = ok(
                    &t.app,
                    &Item {
                        at: at.clone(),
                        list: list(rail.tool),
                        index,
                    },
                )
                .await;
                let edit = match edit {
                    0 => DecideEdit::Valid {
                        words: (!view.target.words.is_empty())
                            .then_some(0)
                            .into_iter()
                            .collect(),
                    },
                    1 => DecideEdit::Invalid,
                    2 => DecideEdit::Todo,
                    3 => DecideEdit::Comment {
                        text: format!("note {n}"),
                    },
                    4 => DecideEdit::Bookmark {
                        on: !view.decision.value.bookmarked,
                    },
                    _ => DecideEdit::Valid { words: vec![] },
                };
                let mut mark = decide(&at, request, &view, edit);
                mark.list = list(rail.tool);
                post(&t.app, &mark).await.unwrap();
            }
            Act::Verse { notes, pick, keep } => {
                let rail = &rails[usize::from(!notes)];
                let index = rail.rows[pick % rail.rows.len()].index;
                let view = ok(
                    &t.app,
                    &Item {
                        at: at.clone(),
                        list: list(rail.tool),
                        index,
                    },
                )
                .await;
                let Some(verse) = view.target.verse.clone() else {
                    continue;
                };
                let words = if keep {
                    format!("{} y más", view.target.words.join(" "))
                } else {
                    "Otras palabras del todo.".to_owned()
                };
                save_verse(&t, &at, request, &verse, &words).await;
            }
            Act::Refresh => {
                for rail in &mut rails {
                    rail.refresh(&t.app, &at).await;
                }
            }
        }
    }
    for rail in &mut rails {
        rail.refresh(&t.app, &at).await;
    }
    let (_, fresh) = fresh_rails(&t, &at).await;
    for (held, fresh) in rails.iter().zip(&fresh) {
        assert_eq!(held.by_index(), fresh.by_index(), "{:?}", held.tool);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// AD17 with C4's re-assessment: whatever marks, comments, bookmarks and verse edits come in
    /// whatever order, with the page asking for the changed rows at any point, the rails it holds
    /// equal the rails a fresh open derives and assesses.
    #[test]
    fn held_rails_equal_a_fresh_open_after_any_session(acts in proptest::collection::vec(act(), 1..10)) {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(play(acts));
    }
}

// ---- marks, on scripted outcomes ----------------------------------------------------------

/// A disk that refuses the write: the mark says so, keeps nothing, and the item reads as it
/// did (AD6's scripted outcome).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mark_the_disk_refuses_is_not_saved_and_changes_nothing() {
    let state = scenario("en-pins");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let backend = Arc::new(backend);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let store = backend.clone();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(store);
    })
    .await;
    let at = opened(&t.app).await.session.at;
    let open = session(&t.app, &at, Tool::TranslationNotes).await;
    let first = the_first(rows(&open), "for the faith of the chosen people of God");
    let view = item(&t.app, &at, Tool::TranslationNotes, first).await;
    backend
        .script(
            &folder(SAMPLE),
            Scripted::NotWritten("the disk is full".into()),
        )
        .unwrap();
    let reply = t
        .app
        .request(&decide(
            &at,
            12,
            &view,
            DecideEdit::Comment {
                text: "kept in the editor".into(),
            },
        ))
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::Err(ApiError::NotSaved(NotSaved::Disk(_)))),
        "{reply:?}"
    );
    assert_eq!(item(&t.app, &at, Tool::TranslationNotes, first).await, view);
    // The write was scripted to succeed once: the same request goes through.
    backend.script(&folder(SAMPLE), Scripted::Written).unwrap();
    let ack = post(
        &t.app,
        &decide(
            &at,
            13,
            &view,
            DecideEdit::Comment {
                text: "kept in the editor".into(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
}

// ---- the selection a drag makes, held to the server's ---------------------------------------

proptest! {
    /// `drag_range` against `tc_check::selection::from_indices`: whatever words a press and a
    /// drag select, the server records exactly those words, whole, and reads the same indices
    /// back (#737).
    #[test]
    fn a_drag_selects_whole_words_and_mark_valid_saves_them(
        base in proptest::collection::btree_set(0u32..16, 0..4),
        from in 0u32..16,
        to in 0u32..16,
    ) {
        const VERSE: &str = "Pablo siervo de Dios y apóstol de Jesucristo, según la fe de los escogidos de Dios";
        let words = target_words(VERSE);
        prop_assert_eq!(words.len(), 16);
        let picked = drag_range(&base, from, to);
        let expected: BTreeSet<u32> = base
            .iter()
            .copied()
            .chain(from.min(to)..=from.max(to))
            .collect();
        prop_assert_eq!(&picked, &expected);
        let chosen: Vec<usize> = picked.iter().map(|&i| i as usize).collect();
        let text = TargetText::new(VERSE.to_owned());
        let selections = from_indices(&text, &chosen).unwrap();
        prop_assert_eq!(selections.len(), picked.len());
        let read: BTreeSet<u32> = indices(VERSE, &selections).into_iter().map(|i| i as u32).collect();
        prop_assert_eq!(read, picked);
    }
}
