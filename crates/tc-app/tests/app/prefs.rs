//! The prefs writer: what a document keeps, and when each file is written.
//!
//! Timing tests run on a paused clock moved by hand, and read the time of each write from the
//! in-memory files.

use std::sync::Arc;
use std::time::Duration;

use proptest::prelude::*;
use serde_json::{Map, Value, json};
use tc_api::check::Tool;
use tc_api::ids::{LocaleId, ProjectId, ProjectRef, RefPart, StoryNo, WallMs};
use tc_api::prefs::{DraftUnit, PlaceChange, PlaceMode, PlaceUpdate};
use tc_app::prefs::{
    AppDataPath, EditUnit, LastEdit, MemoryFiles, PlaceRecord, PlaceVerse, PrefsWriter,
};
use tc_app::reports::ReportOp;
use tc_core::BookId;
use tokio::time::Instant;

use super::common::{Prefs, prefs, prefs_over, settle, step};

fn project(n: u8) -> ProjectId {
    ProjectId::parse(&format!("{n:08x}-0000-4000-8000-000000000000")).unwrap()
}

fn folder(name: &str) -> ProjectRef {
    ProjectRef::parse(name).unwrap()
}

fn tit() -> BookId {
    "TIT".parse().unwrap()
}

fn verse(text: &str) -> RefPart {
    RefPart::parse(text).unwrap()
}

fn tit_at(chapter: u16, v: &str) -> PlaceUpdate {
    PlaceUpdate::Book {
        book: tit(),
        mode: Some(PlaceMode::Draft),
        chapter: Some(chapter),
        verse: PlaceChange::Set(verse(v)),
        tool: None,
    }
}

fn edit(snippet: &str) -> LastEdit {
    LastEdit {
        project: folder("titus"),
        unit: EditUnit::Book {
            book: tit(),
            chapter: 1,
            verse: verse("4"),
        },
        mode: PlaceMode::Draft,
        tool: None,
        snippet: snippet.to_owned(),
        at: WallMs(5),
    }
}

fn writes_of(env: &Prefs, path: &AppDataPath) -> Vec<Instant> {
    env.files
        .writes()
        .into_iter()
        .filter(|(written, _)| written == path)
        .map(|(_, at)| at)
        .collect()
}

fn json_of(env: &Prefs, path: &AppDataPath) -> Value {
    serde_json::from_slice(&env.files.get(path).expect("the file exists")).unwrap()
}

#[tokio::test(start_paused = true)]
async fn place_is_written_500ms_after_the_last_move() {
    let env = prefs();
    let path = AppDataPath::project_prefs(&project(1));
    let t0 = Instant::now();
    env.prefs.record_place(project(1), tit_at(1, "1"));
    settle().await;
    step(200).await;
    env.prefs.record_place(project(1), tit_at(1, "2"));
    settle().await;
    step(200).await;
    env.prefs.record_place(project(1), tit_at(1, "3"));
    settle().await;
    // Each move starts the 500 ms again: nothing at 500 or 700 ms, one at 900.
    step(490).await;
    assert!(writes_of(&env, &path).is_empty());
    step(10).await;
    let writes = writes_of(&env, &path);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0] - t0, Duration::from_millis(900));
    assert_eq!(
        json_of(&env, &path)["places"]["TIT"]["verse"],
        json!("3"),
        "the write holds the newest move"
    );
    step(5000).await;
    assert_eq!(writes_of(&env, &path).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn record_place_follows_tc4() {
    // Ported from tC4's test/place.test.ts, with typed observations.
    let env = prefs();
    let a = project(1);
    let b = project(2);
    let book = |mode, chapter, verse: PlaceChange<RefPart>, tool| PlaceUpdate::Book {
        book: tit(),
        mode,
        chapter,
        verse,
        tool,
    };

    env.prefs.record_place(
        a.clone(),
        book(
            Some(PlaceMode::Draft),
            Some(2),
            PlaceChange::Set(verse("5")),
            None,
        ),
    );
    env.prefs.leave_project().await;
    let first = env.prefs.place(&a, "TIT").unwrap();
    assert_eq!(
        (first.mode, first.chapter, first.verse.clone()),
        (PlaceMode::Draft, 2, Some(PlaceVerse::Text("5".to_owned())))
    );

    // A mode switch keeps the verse.
    env.prefs.record_place(
        a.clone(),
        book(Some(PlaceMode::Read), None, PlaceChange::Keep, None),
    );
    env.prefs.leave_project().await;
    let second = env.prefs.place(&a, "TIT").unwrap();
    assert_eq!(
        (second.mode, second.chapter, second.verse.clone()),
        (PlaceMode::Read, 2, Some(PlaceVerse::Text("5".to_owned())))
    );

    // A frame click in a story with no record starts at Draft, with the story as its chapter
    // (tC4's raw recordPlace says chapter 1, but rememberPlace always passes the story).
    env.prefs.record_place(
        a.clone(),
        PlaceUpdate::Story {
            story: StoryNo::new(3).unwrap(),
            mode: None,
            frame: PlaceChange::Set(4),
            tool: None,
        },
    );
    env.prefs.leave_project().await;
    let story = env.prefs.place(&a, "story:3").unwrap();
    assert_eq!(
        (story.mode, story.chapter, story.verse),
        (PlaceMode::Draft, 3, Some(PlaceVerse::Frame(4)))
    );

    // A chapter click clears the verse and keeps the mode.
    env.prefs
        .record_place(a.clone(), book(None, Some(3), PlaceChange::Clear, None));
    env.prefs.leave_project().await;
    let third = env.prefs.place(&a, "TIT").unwrap();
    assert_eq!(
        (third.mode, third.chapter, third.verse),
        (PlaceMode::Read, 3, None)
    );

    // The tool is kept while the mode stays Check, and dropped when it leaves.
    let jon = |mode, chapter, tool| PlaceUpdate::Book {
        book: "JON".parse().unwrap(),
        mode,
        chapter,
        verse: PlaceChange::Keep,
        tool,
    };
    env.prefs.record_place(
        a.clone(),
        jon(
            Some(PlaceMode::Check),
            Some(1),
            Some(Tool::TranslationNotes),
        ),
    );
    env.prefs.leave_project().await;
    assert_eq!(
        env.prefs.place(&a, "JON").unwrap().tool,
        Some(Tool::TranslationNotes)
    );
    env.prefs
        .record_place(a.clone(), jon(Some(PlaceMode::Check), None, None));
    env.prefs.leave_project().await;
    assert_eq!(
        env.prefs.place(&a, "JON").unwrap().tool,
        Some(Tool::TranslationNotes)
    );
    env.prefs
        .record_place(a.clone(), jon(Some(PlaceMode::Draft), None, None));
    env.prefs.leave_project().await;
    assert_eq!(env.prefs.place(&a, "JON").unwrap().tool, None);

    // Records of two units of one project and of two projects stay apart, in the order
    // they were first made.
    env.prefs.record_place(
        b.clone(),
        PlaceUpdate::Story {
            story: StoryNo::new(1).unwrap(),
            mode: Some(PlaceMode::Read),
            frame: PlaceChange::Set(2),
            tool: None,
        },
    );
    env.prefs.leave_project().await;
    let file_a = json_of(&env, &AppDataPath::project_prefs(&a));
    let keys: Vec<&String> = file_a["places"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["TIT", "story:3", "JON"]);
    assert_eq!(
        env.prefs.place(&b, "story:1").unwrap().verse,
        Some(PlaceVerse::Frame(2))
    );
    assert!(env.prefs.place(&b, "TIT").is_none());
    assert_eq!(env.prefs.place(&a, "TIT").unwrap().mode, PlaceMode::Read);
}

#[tokio::test(start_paused = true)]
async fn a_visit_to_the_place_already_recorded_starts_no_write() {
    let env = prefs();
    let path = AppDataPath::project_prefs(&project(1));
    env.prefs.record_place(project(1), tit_at(2, "5"));
    settle().await;
    step(600).await;
    assert_eq!(writes_of(&env, &path).len(), 1);
    // The same place again changes only the time.
    env.clock.advance(60_000);
    env.prefs.record_place(project(1), tit_at(2, "5"));
    settle().await;
    step(5000).await;
    assert_eq!(writes_of(&env, &path).len(), 1);
    // A move writes again, with the newer time.
    env.prefs.record_place(project(1), tit_at(2, "6"));
    settle().await;
    step(500).await;
    assert_eq!(writes_of(&env, &path).len(), 2);
}

#[tokio::test(start_paused = true)]
async fn last_edit_is_written_5s_after_the_last_change() {
    let env = prefs();
    let app = AppDataPath::app_prefs();
    let t0 = Instant::now();
    for n in 0..5 {
        env.prefs
            .record_last_edit(project(1), edit(&format!("s{n}")));
        settle().await;
        if n < 4 {
            step(1000).await;
        }
    }
    // The last change was at 4 s. Nothing is written for the app document before 9 s.
    step(4990).await;
    assert!(writes_of(&env, &app).is_empty());
    step(10).await;
    let writes = writes_of(&env, &app);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0] - t0, Duration::from_secs(9));
    assert_eq!(json_of(&env, &app)["lastEdit"]["snippet"], json!("s4"));
    assert_eq!(env.prefs.last_edit().unwrap().snippet, "s4");
}

#[tokio::test(start_paused = true)]
async fn typing_writes_no_app_data_until_it_stops() {
    // A-20: typing costs no fsynced app-data write each second.
    let env = prefs();
    let app = AppDataPath::app_prefs();
    let place = AppDataPath::project_prefs(&project(1));
    let t0 = Instant::now();
    for n in 0..60 {
        env.prefs
            .record_last_edit(project(1), edit(&format!("s{n}")));
        settle().await;
        step(500).await;
    }
    // 30 s of an edit every half second: the app document is not written at all, and the
    // place file once, for the first visit to the verse.
    assert!(writes_of(&env, &app).is_empty());
    assert_eq!(writes_of(&env, &place).len(), 1);
    step(4500).await;
    let writes = writes_of(&env, &app);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0] - t0, Duration::from_millis(34_500));
}

#[tokio::test(start_paused = true)]
async fn last_edit_is_written_at_once_when_the_person_leaves() {
    let env = prefs();
    let app = AppDataPath::app_prefs();
    let t0 = Instant::now();
    env.prefs.record_last_edit(project(1), edit("Paul"));
    settle().await;
    step(1000).await;
    assert!(writes_of(&env, &app).is_empty());
    env.prefs.leave_project().await;
    let writes = writes_of(&env, &app);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0] - t0, Duration::from_secs(1));
    // Nothing is left to write later.
    step(60_000).await;
    assert_eq!(writes_of(&env, &app).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn flush_writes_everything_pending() {
    let env = prefs();
    env.prefs.record_place(project(1), tit_at(1, "1"));
    env.prefs.record_place(project(2), tit_at(2, "2"));
    env.prefs.record_last_edit(project(1), edit("x"));
    env.prefs.flush().await;
    let written: Vec<String> = env
        .files
        .writes()
        .iter()
        .map(|(path, _)| path.as_str().to_owned())
        .collect();
    assert_eq!(written.len(), 3, "{written:?}");
    assert!(written.contains(&"prefs/app.json".to_owned()));
    assert!(
        env.files
            .get(&AppDataPath::project_prefs(&project(2)))
            .is_some()
    );
}

#[tokio::test(start_paused = true)]
async fn pending_writes_are_flushed_when_the_writer_is_dropped() {
    let env = prefs();
    env.prefs.record_last_edit(project(1), edit("kept"));
    settle().await;
    let Prefs { prefs, files, .. } = env;
    drop(prefs);
    settle().await;
    let written = files
        .get(&AppDataPath::app_prefs())
        .expect("written at the end");
    assert!(String::from_utf8(written).unwrap().contains("kept"));
}

#[tokio::test(start_paused = true)]
async fn the_app_language_is_saved_at_once_and_waited_for() {
    let env = prefs();
    assert_eq!(env.prefs.app_locale(), LocaleId::ENGLISH);
    env.prefs
        .set_locale(LocaleId::parse("ar").unwrap())
        .await
        .unwrap();
    assert_eq!(env.prefs.app_locale().as_str(), "ar");
    assert_eq!(
        json_of(&env, &AppDataPath::app_prefs())["appLocale"],
        json!("ar")
    );
    assert_eq!(
        json_of(&env, &AppDataPath::app_prefs())["schemaVersion"],
        json!(1)
    );
}

#[tokio::test(start_paused = true)]
async fn the_other_at_once_writes_happen_at_once() {
    let env = prefs();
    env.prefs.record_last_used(folder("titus"), WallMs(77));
    env.prefs.set_align_suggestions(folder("titus"), true);
    env.prefs.set_draft_unit(project(1), DraftUnit::Section);
    settle().await;
    assert_eq!(env.prefs.last_used(&folder("titus")), Some(WallMs(77)));
    assert_eq!(env.prefs.align_suggestions(&folder("titus")), Some(true));
    assert_eq!(env.prefs.draft_unit(&project(1)), Some(DraftUnit::Section));
    let app = json_of(&env, &AppDataPath::app_prefs());
    assert_eq!(app["lastUsed"]["titus"], json!(77));
    assert_eq!(app["alignSuggestions"]["titus"], json!(true));
    let project_file = json_of(&env, &AppDataPath::project_prefs(&project(1)));
    assert_eq!(project_file["draftUnit"], json!("section"));
}

#[tokio::test(start_paused = true)]
async fn the_suggestion_switch_is_visible_before_its_writer_runs() {
    let env = prefs();
    env.prefs.set_align_suggestions(folder("titus"), true);
    assert_eq!(env.prefs.align_suggestions(&folder("titus")), Some(true));
    env.prefs.set_align_suggestions(folder("titus"), false);
    assert_eq!(env.prefs.align_suggestions(&folder("titus")), Some(false));
    settle().await;
    assert_eq!(env.prefs.align_suggestions(&folder("titus")), Some(false));
    assert_eq!(
        json_of(&env, &AppDataPath::app_prefs())["alignSuggestions"]["titus"],
        json!(false)
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_locale_save_is_refused_and_changes_nothing() {
    let files = Arc::new(MemoryFiles::new());
    let seeded = br#"{"schemaVersion":1,"appLocale":"es-419"}"#.to_vec();
    files.seed(AppDataPath::app_prefs(), seeded.clone());
    let env = prefs_over(files);
    assert_eq!(env.prefs.app_locale().as_str(), "es-419");

    env.files.fail_writes(true);
    let fr = LocaleId::parse("fr").unwrap();
    assert!(env.prefs.set_locale(fr).await.is_err());
    assert_eq!(
        env.prefs.app_locale().as_str(),
        "es-419",
        "the language did not change"
    );
    assert_eq!(
        env.files.get(&AppDataPath::app_prefs()),
        Some(seeded),
        "the file did not change"
    );
    assert_eq!(
        env.reports.all().last().map(|r| r.op),
        Some(ReportOp::PrefsWrite)
    );

    // A later write that carries other changes does not slip the refused language in.
    env.files.fail_writes(false);
    env.prefs.record_last_used(folder("titus"), WallMs(1));
    settle().await;
    assert_eq!(
        json_of(&env, &AppDataPath::app_prefs())["appLocale"],
        json!("es-419")
    );

    // The retry succeeds.
    env.prefs.set_locale(fr).await.unwrap();
    assert_eq!(env.prefs.app_locale().as_str(), "fr");
    assert_eq!(
        json_of(&env, &AppDataPath::app_prefs())["appLocale"],
        json!("fr")
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_place_write_keeps_the_value_and_adds_a_report() {
    let env = prefs();
    let path = AppDataPath::project_prefs(&project(1));
    env.files.fail_writes(true);
    env.prefs.record_place(project(1), tit_at(2, "5"));
    settle().await;
    step(600).await;
    assert!(env.files.get(&path).is_none());
    // The value is kept for the session.
    assert_eq!(env.prefs.place(&project(1), "TIT").unwrap().chapter, 2);
    let reports = env.reports.all();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].op, ReportOp::PrefsWrite);
    // The next change of the file writes it again, with both.
    env.files.fail_writes(false);
    env.prefs.record_place(
        project(1),
        PlaceUpdate::Story {
            story: StoryNo::new(2).unwrap(),
            mode: None,
            frame: PlaceChange::Set(1),
            tool: None,
        },
    );
    settle().await;
    step(500).await;
    let file = json_of(&env, &path);
    assert!(file["places"]["TIT"].is_object());
    assert!(file["places"]["story:2"].is_object());
}

#[tokio::test(start_paused = true)]
async fn unknown_and_malformed_members_are_kept_on_rewrite() {
    let files = Arc::new(MemoryFiles::new());
    files.seed(
        AppDataPath::app_prefs(),
        br#"{"appLocale": {"id": "fr"}, "future": [1, 2], "lastUsed": {"old": 9}}"#.to_vec(),
    );
    let env = prefs_over(files);
    assert_eq!(env.prefs.app_locale(), LocaleId::ENGLISH);
    env.prefs.record_last_used(folder("titus"), WallMs(5));
    settle().await;
    let app = json_of(&env, &AppDataPath::app_prefs());
    assert_eq!(app["appLocale"], json!({"id": "fr"}));
    assert_eq!(app["future"], json!([1, 2]));
    assert_eq!(app["lastUsed"], json!({"old": 9, "titus": 5}));
    // Members stay in their order, and the version is added at the end.
    let keys: Vec<&String> = app.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["appLocale", "future", "lastUsed", "schemaVersion"]);
}

#[tokio::test(start_paused = true)]
async fn each_bad_saved_locale_reads_as_english_and_stays() {
    for bad in [json!("de"), json!("EN"), json!(42), json!({"id": "fr"})] {
        let files = Arc::new(MemoryFiles::new());
        let seeded = serde_json::to_vec(&json!({ "appLocale": bad })).unwrap();
        files.seed(AppDataPath::app_prefs(), seeded.clone());
        let env = prefs_over(files);
        assert_eq!(env.prefs.app_locale(), LocaleId::ENGLISH, "{bad}");
        env.prefs.flush().await;
        assert_eq!(
            env.files.get(&AppDataPath::app_prefs()),
            Some(seeded),
            "{bad}: the document is left as it is"
        );
        // Another write keeps the value too; only choosing a language replaces it.
        env.prefs.record_last_used(folder("titus"), WallMs(1));
        settle().await;
        assert_eq!(json_of(&env, &AppDataPath::app_prefs())["appLocale"], bad);
        env.prefs
            .set_locale(LocaleId::parse("hi").unwrap())
            .await
            .unwrap();
        assert_eq!(
            json_of(&env, &AppDataPath::app_prefs())["appLocale"],
            json!("hi")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_file_is_kept_aside_and_reported() {
    for bytes in [&b"{not json"[..], b"[1, 2]", b"\"text\"", b""] {
        let files = Arc::new(MemoryFiles::new());
        files.seed(AppDataPath::app_prefs(), bytes.to_vec());
        let env = prefs_over(files);
        assert_eq!(env.prefs.app_locale(), LocaleId::ENGLISH);
        assert_eq!(
            env.files
                .get(&AppDataPath::app_prefs().unreadable_copy())
                .as_deref(),
            Some(bytes),
            "the old bytes are kept"
        );
        let reports = env.reports.all();
        assert_eq!(reports.len(), 1);
        assert!(matches!(reports[0].op, ReportOp::PrefsWrite));
        // The next write replaces the file with a readable one.
        env.prefs.record_last_used(folder("titus"), WallMs(1));
        settle().await;
        assert_eq!(
            json_of(&env, &AppDataPath::app_prefs())["lastUsed"]["titus"],
            json!(1)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_place_record_keeps_the_members_it_does_not_know() {
    let files = Arc::new(MemoryFiles::new());
    let path = AppDataPath::project_prefs(&project(1));
    files.seed(
        path.clone(),
        serde_json::to_vec(&json!({
            "places": { "TIT": { "mode": "read", "chapter": 1, "verse": null, "at": 1, "later": true } },
            "other": 1
        }))
        .unwrap(),
    );
    let env = prefs_over(files);
    env.prefs.record_place(project(1), tit_at(2, "3"));
    env.prefs.flush().await;
    let file = json_of(&env, &path);
    assert_eq!(file["places"]["TIT"]["later"], json!(true));
    assert_eq!(file["places"]["TIT"]["chapter"], json!(2));
    assert_eq!(file["other"], json!(1));
}

#[tokio::test(start_paused = true)]
async fn reset_puts_the_defaults_back_and_writes_them() {
    let env = prefs();
    env.prefs
        .set_locale(LocaleId::parse("ar").unwrap())
        .await
        .unwrap();
    env.prefs.record_place(project(1), tit_at(2, "3"));
    env.prefs.record_last_edit(project(1), edit("x"));
    settle().await;
    env.prefs.reset().await;
    assert_eq!(env.prefs.app_locale(), LocaleId::ENGLISH);
    assert!(env.prefs.place(&project(1), "TIT").is_none());
    assert!(env.prefs.last_edit().is_none());
    let before = env.files.writes().len();
    step(60_000).await;
    assert_eq!(env.files.writes().len(), before, "nothing was left pending");
    assert!(
        json_of(&env, &AppDataPath::app_prefs())
            .get("appLocale")
            .is_none()
    );
}

fn json_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i32>().prop_map(Value::from),
        "[a-z ]{0,8}".prop_map(Value::from),
        proptest::collection::vec(any::<i32>().prop_map(Value::from), 0..4).prop_map(Value::Array),
        proptest::collection::btree_map("[a-z]{1,3}", any::<i32>().prop_map(Value::from), 0..3)
            .prop_map(|m| Value::Object(m.into_iter().collect())),
    ]
}

#[derive(Debug, Clone)]
enum Update {
    LastUsed(u8, i64),
    Align(u8, bool),
    LastEdit(u8),
    Place(u16),
    Locale(u8),
    Draft(bool),
    Wait(u64),
    Flush,
}

fn updates() -> impl Strategy<Value = Vec<Update>> {
    proptest::collection::vec(
        prop_oneof![
            (0u8..3, any::<i64>()).prop_map(|(f, ms)| Update::LastUsed(f, ms)),
            (0u8..3, any::<bool>()).prop_map(|(f, on)| Update::Align(f, on)),
            (0u8..5).prop_map(Update::LastEdit),
            (1u16..20).prop_map(Update::Place),
            (0u8..10).prop_map(Update::Locale),
            any::<bool>().prop_map(Update::Draft),
            (1u64..3000).prop_map(Update::Wait),
            Just(Update::Flush),
        ],
        0..40,
    )
}

proptest! {
    /// However the app updates its documents, every member it does not know is still there,
    /// unchanged and in its place, and so is every unknown entry of a member it does know.
    #[test]
    fn documents_survive_any_sequence_of_updates(
        unknown_app in proptest::collection::vec(json_value(), 0..6),
        unknown_project in proptest::collection::vec(json_value(), 0..6),
        updates in updates(),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        let (app_file, project_file) = runtime.block_on(async {
            let mut app: Map<String, Value> = unknown_app
                .iter()
                .enumerate()
                .map(|(n, v)| (format!("x{n}"), v.clone()))
                .collect();
            app.insert("lastUsed".to_owned(), json!({ "elsewhere": 7 }));
            let mut project_doc: Map<String, Value> = unknown_project
                .iter()
                .enumerate()
                .map(|(n, v)| (format!("y{n}"), v.clone()))
                .collect();
            project_doc.insert("places".to_owned(), json!({ "GEN": { "mode": "read", "chapter": 1, "verse": null, "at": 1 } }));
            let files = Arc::new(MemoryFiles::new());
            files.seed(AppDataPath::app_prefs(), serde_json::to_vec(&app).unwrap());
            files.seed(AppDataPath::project_prefs(&project(1)), serde_json::to_vec(&project_doc).unwrap());
            let env = prefs_over(files);
            let writer: &PrefsWriter = &env.prefs;
            for update in updates {
                match update {
                    Update::LastUsed(f, ms) => writer.record_last_used(folder(&format!("f{f}")), WallMs(ms)),
                    Update::Align(f, on) => writer.set_align_suggestions(folder(&format!("f{f}")), on),
                    Update::LastEdit(n) => writer.record_last_edit(project(1), edit(&format!("s{n}"))),
                    Update::Place(n) => writer.record_place(project(1), tit_at(n, "1")),
                    Update::Locale(n) => {
                        let id = tc_api::ids::LOCALE_IDS[usize::from(n)];
                        writer.set_locale(LocaleId::parse(id).unwrap()).await.unwrap();
                    }
                    Update::Draft(section) => writer.set_draft_unit(
                        project(1),
                        if section { DraftUnit::Section } else { DraftUnit::Verse },
                    ),
                    Update::Wait(ms) => step(ms).await,
                    Update::Flush => writer.flush().await,
                }
            }
            writer.flush().await;
            (
                json_of(&env, &AppDataPath::app_prefs()),
                json_of(&env, &AppDataPath::project_prefs(&project(1))),
            )
        });
        let app_keys: Vec<&String> = app_file.as_object().unwrap().keys().collect();
        for (n, original) in unknown_app.iter().enumerate() {
            prop_assert_eq!(app_keys[n], &format!("x{n}"));
            prop_assert_eq!(&app_file[format!("x{n}")], original);
        }
        prop_assert_eq!(&app_file["lastUsed"]["elsewhere"], &json!(7));
        let project_keys: Vec<&String> = project_file.as_object().unwrap().keys().collect();
        for (n, original) in unknown_project.iter().enumerate() {
            prop_assert_eq!(project_keys[n], &format!("y{n}"));
            prop_assert_eq!(&project_file[format!("y{n}")], original);
        }
        prop_assert_eq!(&project_file["places"]["GEN"]["chapter"], &json!(1));
    }
}

#[tokio::test(start_paused = true)]
async fn place_records_round_trip_through_the_file() {
    let env = prefs();
    env.prefs.record_place(project(1), tit_at(2, "4-5"));
    env.prefs.leave_project().await;
    let record: PlaceRecord = env.prefs.place(&project(1), "TIT").unwrap();
    assert_eq!(record.verse, Some(PlaceVerse::Text("4-5".to_owned())));
    // A new writer over the same files reads the same record.
    let again = prefs_over(Arc::clone(&env.files));
    assert_eq!(again.prefs.place(&project(1), "TIT"), Some(record));
}

/// REF1c, tC4 #769: a Check rail's folded groups are kept per computer, an unfold as `false`
/// as tC4 writes it, and read back after a restart for their own tool only.
#[tokio::test(start_paused = true)]
async fn fold_survives_a_restart() {
    let files = Arc::new(MemoryFiles::new());
    let env = prefs_over(files.clone());
    let groups = ["figs-metaphor".to_owned(), "figs-idiom".to_owned()];
    env.prefs
        .set_rail_folded(Tool::TranslationNotes, &groups, true);
    env.prefs
        .set_rail_folded(Tool::TranslationNotes, &groups[1..], false);
    assert_eq!(
        env.prefs.rail_folded(Tool::TranslationNotes),
        ["figs-metaphor"],
        "readers see it before the write"
    );
    settle().await;
    assert_eq!(
        json_of(&env, &AppDataPath::app_prefs())["railFolded"],
        json!({"translationNotes|figs-metaphor": true, "translationNotes|figs-idiom": false})
    );
    drop(env);
    let again = prefs_over(files);
    assert_eq!(
        again.prefs.rail_folded(Tool::TranslationNotes),
        ["figs-metaphor"]
    );
    assert!(again.prefs.rail_folded(Tool::TranslationWords).is_empty());
}
