//! Open and Understand (unit B3) through `App::request`: the open job and its outcome, a
//! refused open, the close at Home, the open book, the source panes and the helps, on the
//! parity scenarios with their resources installed.

use std::sync::Arc;
use std::time::Duration;

use tc_api::ApiError;
use tc_api::app::Hello;
use tc_api::editor::{Book, FrameRow, PaneText, RowSource, SourceFrame, Sources};
use tc_api::helps::{Article, ArticleLink, ArticleView, Passage, Slot};
use tc_api::home::ProjectKind;
use tc_api::ids::{JobId, Scope};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::prefs::DraftUnit;
use tc_api::refusal::OpenRefusal;
use tc_api::session::{Close, Open, OpenUnit, OpenedProject};
use tc_api::shown::{Direction, Known};
use tc_api::{Refusal, Reply};
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::scenario::{ScenarioState, install_resources_into};
use tc_app::{App, StoreChoice};
use tc_core::BookId;
use tc_versification::StandardScheme;

use super::common::{Parts, TestApp, folder, rid};
use super::scenario::scenario;

/// An app over a parity scenario's projects with its resources installed, as `tc4 serve
/// --scenario` starts.
pub async fn start(name: &str) -> (tempfile::TempDir, TestApp) {
    start_over(scenario(name)).await
}

/// The same, its Bible project created in another versification (New Bible offers six).
pub(super) async fn start_in(name: &str, scheme: StandardScheme) -> (tempfile::TempDir, TestApp) {
    let files = scenario(name).files().iter().map(|(path, bytes)| {
        let vrs = path.ends_with("sample_burrito/ingredients/vrs.json");
        let bytes = if vrs {
            scheme.json().as_bytes().to_vec()
        } else {
            bytes.clone()
        };
        (path.clone(), bytes)
    });
    start_over(&ScenarioState::from_files(files).unwrap()).await
}

pub(super) async fn start_over(state: &ScenarioState) -> (tempfile::TempDir, TestApp) {
    let (backend, report) = MemoryBackend::from_scenario(state).unwrap();
    assert!(report.refused.len() <= 1, "{:?}", report.refused);
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = StoreChoice::Given(Arc::new(backend));
    })
    .await;
    (dir, t)
}

pub async fn ok<E: tc_api::Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

/// The job's end, within five seconds.
async fn ended(app: &App, job: JobId) -> JobState {
    let mut watch = app.jobs().watch(job).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        watch
            .wait_for(tc_app::jobs::JobRegistry::is_ended)
            .await
            .unwrap()
            .clone()
    })
    .await
    .expect("the open ends")
}

async fn open(app: &App, project: &str, unit: Option<OpenUnit>) -> JobState {
    let job = ok(
        app,
        &Open {
            request: rid(40),
            project: folder(project),
            unit,
        },
    )
    .await;
    ended(app, job).await
}

pub async fn opened(app: &App) -> OpenedProject {
    match open(app, "sample_burrito", Some(OpenUnit::Book(BookId::Titus))).await {
        JobState::Done(JobOutcome::Opened(opened)) => *opened,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_gives_the_session_and_the_projects_chip() {
    let (_dir, t) = start("sample").await;
    let opened = opened(&t.app).await;
    assert_eq!(opened.name, "Equipo Ejemplo — Tito y Jonás");
    assert_eq!(opened.tag, "es-419");
    assert_eq!(opened.dir, Direction::Ltr);
    assert_eq!(opened.kind, ProjectKind::Bible);
    assert_eq!(opened.books, vec![BookId::Jonah, BookId::Titus]);
    assert_eq!(opened.draft_unit, DraftUnit::Section);
    assert_eq!(opened.place, None, "nothing is remembered yet");
    let hello = ok(&t.app, &Hello {}).await;
    assert_eq!(hello.sessions, vec![opened.session.clone()]);
    assert_eq!(
        t.app
            .prefs()
            .last_used(&folder("sample_burrito"))
            .map(|_| ()),
        Some(()),
        "the open counts as a use, for Home's order"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_open_fails_its_job_in_tc4s_words() {
    let (_dir, t) = start("sample").await;
    let state = open(&t.app, "no_such_project", None).await;
    assert_eq!(
        state,
        JobState::Failed(ApiError::Refused(Refusal::Open(OpenRefusal::NotAProject)))
    );
    assert!(ok(&t.app, &Hello {}).await.sessions.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn going_home_closes_the_screen_session() {
    let (_dir, t) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    ok(
        &t.app,
        &Close {
            request: rid(41),
            at: at.clone(),
        },
    )
    .await;
    let read = t
        .app
        .request(&Book {
            at: at.clone(),
            book: BookId::Titus,
        })
        .await
        .unwrap();
    assert!(
        matches!(read, Reply::Err(ApiError::StaleScope { .. })),
        "{read:?}"
    );
    // Closing again is a stale scope, never a second leave.
    let again = t
        .app
        .request(&Close {
            request: rid(42),
            at,
        })
        .await
        .unwrap();
    assert!(
        matches!(again, Reply::Err(ApiError::NotSaved(_))),
        "{again:?}"
    );
}

pub(super) async fn at(t: &TestApp) -> Scope {
    opened(&t.app).await.session.at
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_book_has_its_chapters_and_its_drafted_share_in_the_rail() {
    let (_dir, t) = start("sample").await;
    let at = at(&t).await;
    let book = ok(
        &t.app,
        &Book {
            at,
            book: BookId::Titus,
        },
    )
    .await;
    assert_eq!(book.chapters, vec![1, 2, 3]);
    let rail: Vec<(BookId, Known<u8>)> = book
        .rail
        .iter()
        .map(|r| (r.book, r.drafted.clone()))
        .collect();
    // tC4's baseline shows Titus at 11%; Jonah's share comes from Home's summary, which no
    // Home visit computed here.
    assert_eq!(
        rail,
        vec![
            (BookId::Jonah, Known::Unknown),
            (BookId::Titus, Known::Known(11))
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chapters_panes_have_tc4s_sections_and_verse_text() {
    let (_dir, t) = start("en-pins").await;
    let at = at(&t).await;
    let sources = ok(
        &t.app,
        &Sources {
            at,
            book: BookId::Titus,
            chapter: 1,
        },
    )
    .await;
    let ids: Vec<&str> = sources.panes.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["ult", "ust"]);
    let PaneText::Ready(ult) = &sources.panes[0].text else {
        panic!("{:?}", sources.panes[0].text)
    };
    // The units of tC4's understand-tit1: Titus 1:1–3, 1:4–5, 1:6–7, 1:8–9, 1:10–11,
    // 1:12–14, 1:15–16.
    assert_eq!(ult.sections, vec![1, 4, 6, 8, 10, 12, 15]);
    assert_eq!(ult.verses.len(), 16);
    let first = &ult.verses[0];
    assert_eq!(first.key.as_str(), "1");
    assert_eq!(
        first.text,
        "Paul, a servant of God and an apostle of Jesus Christ, for the faith of the chosen people of God and knowledge of the truth that agrees with godliness,"
    );
    let joined: String = first.tokens.concat();
    assert_eq!(tc_usfm::text::normalize_whitespace(&joined), first.text);
    // Verse 5 starts a paragraph of its own in tC4's unit Titus 1:4–5.
    assert!(
        ult.verses[4].before.is_some() || ult.verses[4].lead.is_some(),
        "{:?}",
        ult.verses[4]
    );
}

/// tC4 #118: a project in another numbering reads one unit per project verse, each with the
/// source verse its numbering names, and every help at its place in the project, a chapter's
/// introduction kept. Titus is numbered alike in `rsc`, so each verse is its own and the
/// helps are the same project's in `eng` (the shift itself: `numbering`'s tests).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_in_another_numbering_reads_each_verse_where_the_sources_have_it() {
    let read = async |t: &TestApp| {
        let at = at(t).await;
        let sources = ok(
            &t.app,
            &Sources {
                at: at.clone(),
                book: BookId::Titus,
                chapter: 1,
            },
        )
        .await;
        let helps = ok(
            &t.app,
            &Passage {
                at,
                book: BookId::Titus,
                chapter: 1,
            },
        )
        .await;
        (sources, helps)
    };
    let (_eng_dir, eng) = start("en-pins").await;
    let (same, same_helps) = read(&eng).await;
    assert_eq!(same.frame, SourceFrame::Same);
    let (_dir, t) = start_in("en-pins", StandardScheme::Rsc).await;
    let (sources, helps) = read(&t).await;
    let SourceFrame::Mapped { rows } = &sources.frame else {
        panic!("{:?}", sources.frame)
    };
    assert_eq!(rows.len(), 16);
    assert_eq!(
        rows[3],
        FrameRow {
            verse: "4".to_owned().try_into().unwrap(),
            source: RowSource::Verse {
                chapter: 1,
                verse: "4".to_owned().try_into().unwrap(),
            },
        }
    );
    let (PaneText::Ready(ult), PaneText::Ready(eng_ult)) =
        (&sources.panes[0].text, &same.panes[0].text)
    else {
        panic!("{:?}", sources.panes[0].text)
    };
    assert!(ult.sections.is_empty(), "one unit per verse");
    assert_eq!(ult.verses, eng_ult.verses);
    assert_eq!(helps.notes, same_helps.notes, "the intro note included");
    assert_eq!(helps.words, same_helps.words);
    assert_eq!(helps.questions, same_helps.questions);
    assert_eq!(helps.other_book, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chapters_helps_are_tc4s_cards_with_gateway_titles() {
    let (_dir, t) = start("en-pins").await;
    let at = at(&t).await;
    let helps = ok(
        &t.app,
        &Passage {
            at,
            book: BookId::Titus,
            chapter: 1,
        },
    )
    .await;
    let Slot::Ready(notes) = &helps.notes else {
        panic!("{:?}", helps.notes)
    };
    assert_eq!(notes.dir, Some(Direction::Ltr));
    let intro = &notes.items[0];
    assert_eq!(
        (intro.chapter.as_str(), intro.verse.as_str()),
        ("1", "intro")
    );
    assert_eq!(intro.note[0].text(), "Titus 1 Chapter Introduction");
    let first = &notes.items[1];
    assert_eq!(first.verse.as_str(), "1");
    let ult = first.panes.iter().find(|p| p.pane == "ult").unwrap();
    assert_eq!(
        ult.title.as_deref(),
        Some("for the faith of the chosen people of God and knowledge of the truth")
    );
    assert_eq!(ult.lit.len(), 1);
    assert_eq!(first.academy.as_deref(), Some("figs-abstractnouns"));
    assert!(notes.items.iter().all(|n| n.chapter.as_str() == "1"));
    // tC4 vendors no en_tq v91: the tab says the resource is not on this computer yet.
    assert_eq!(helps.questions, Slot::Fetch);
    let Slot::Ready(words) = &helps.words else {
        panic!("{:?}", helps.words)
    };
    assert!(
        words
            .items
            .iter()
            .any(|w| w.slug == "god" && w.category.as_deref() == Some("kt"))
    );
    assert_eq!(helps.simplified_pin.as_deref(), Some("en_ust"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_article_comes_from_the_books_helps() {
    let (_dir, t) = start("en-pins").await;
    let at = at(&t).await;
    let article = ok(
        &t.app,
        &Article {
            at: at.clone(),
            book: BookId::Titus,
            link: ArticleLink::Words {
                category: Some("kt".into()),
                slug: "god".into(),
            },
        },
    )
    .await;
    let ArticleView::Found { title, blocks, .. } = article else {
        panic!("{article:?}")
    };
    assert_eq!(title, "God");
    // tC4's pane draws the whole file, its title line first.
    assert_eq!(blocks[0].kind, tc_api::model::BlockKind::H);
    assert_eq!(blocks[0].text(), "God");
    let missing = ok(
        &t.app,
        &Article {
            at,
            book: BookId::Titus,
            link: ArticleLink::Academy {
                slug: "no-such-article".into(),
            },
        },
    )
    .await;
    assert_eq!(missing, ArticleView::Missing);
}

// ---- take_spans: the UI's copy against tc-helps' (§3.8) -----------------------------------

fn api_spans(spans: &[tc_helps::blocks::Span]) -> Vec<tc_api::model::ArticleSpan> {
    spans
        .iter()
        .map(|s| tc_api::model::ArticleSpan {
            text: s.text.clone(),
            bold: s.bold,
            italic: s.italic,
        })
        .collect()
}

/// tc-api's `take_spans`, which the UI cuts a note's preview with, gives what tc-helps' port
/// of tC4's `takeSpans` gives, at every length of every block of the Titus notes and articles
/// the block vectors hold.
#[test]
fn take_spans_cuts_every_block_as_tc_helps_does() {
    let vectors: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            super::common::repo_root().join("fixtures/helps/article-block-vectors.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut cuts = 0;
    for case in vectors["vectors"].as_array().unwrap() {
        for block in tc_helps::blocks::render_article_blocks(case["input"].as_str().unwrap()) {
            let ours = api_spans(&block.spans);
            let len = tc_helps::blocks::js_len(&block.text());
            assert_eq!(tc_api::model::js_len(&block.text()), len);
            for n in 0..=len + 1 {
                assert_eq!(
                    tc_api::model::take_spans(&ours, n),
                    api_spans(&tc_helps::blocks::take_spans(&block.spans, n)),
                    "{n} of {:?}",
                    block.text()
                );
                cuts += 1;
            }
        }
    }
    assert!(cuts > 100_000, "{cuts} cuts");
}

proptest::proptest! {
    #[test]
    fn take_spans_agrees_on_any_spans(
        spans in proptest::collection::vec(("[a-zé𝔸 ]{0,6}", proptest::bool::ANY, proptest::bool::ANY), 0..6),
        n in 0usize..40,
    ) {
        let helps: Vec<tc_helps::blocks::Span> = spans
            .into_iter()
            .map(|(text, bold, italic)| tc_helps::blocks::Span { text, bold, italic })
            .collect();
        proptest::prop_assert_eq!(
            tc_api::model::take_spans(&api_spans(&helps), n),
            api_spans(&tc_helps::blocks::take_spans(&helps, n))
        );
    }
}

/// J15's server half: `test/hold` at `open` keeps the job running with the project unopened,
/// so the UI's overlay is what shows; the release lets it end with its outcome.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_open_runs_until_it_is_released() {
    use tc_api::test::{Hold, HoldPoint, Release};
    let (_dir, t) = start("sample").await;
    ok(
        &t.app,
        &Hold {
            request: rid(43),
            at: HoldPoint::Open,
        },
    )
    .await;
    let job = ok(
        &t.app,
        &Open {
            request: rid(44),
            project: folder("sample_burrito"),
            unit: None,
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while t.app.holding() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the open reaches its hold");
    let held = t.app.jobs().view(job).unwrap().state;
    assert!(!tc_app::jobs::JobRegistry::is_ended(&held), "{held:?}");
    assert!(
        ok(&t.app, &Hello {}).await.sessions.is_empty(),
        "nothing is open yet"
    );
    ok(
        &t.app,
        &Release {
            request: rid(45),
            at: HoldPoint::Open,
        },
    )
    .await;
    assert!(matches!(
        ended(&t.app, job).await,
        JobState::Done(JobOutcome::Opened(_))
    ));
}
