//! Golden JSON (APP-DESIGN §10, unit A1): one file per endpoint under `tests/golden/`, with
//! an example request and reply, plus the wire forms of errors, events and refusals. A
//! change to a wire shape changes a file, so it shows in review. Run with `UPDATE_GOLDEN=1`
//! to write the files again after a deliberate change.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tc_api::account::{
    AccountNotice, AccountView, CancelSignIn, HelpLink, KeepState, LinkView, ProfileLink, Resume,
    ResumeOutcome, ResumeView, SignIn, SignOut, State as AccountState, WebUrl,
};
use tc_api::align::AlignSave;
use tc_api::app::{
    AppPrefsView, BuildInfo, ClientError, Hello, HelloView, OpenSession, Quit, SessionRole,
};
use tc_api::check::{
    Category, CheckListRef, Dot, NoteCategory, RailRow, RailRowPatch, RowFlags, Tool,
};
use tc_api::error::{ApiError, Feature, Flow, SignInReason};
use tc_api::event::{
    Aspect, ChangeCause, ChangeParts, ChangeScope, ChapterRef, CheckpointOutcome, Event,
    EventFrame, RecordChange, SessionEvent, SessionMode, StartupProgress, StartupState,
};
use tc_api::feedback::{Compose, FeedbackDraft};
use tc_api::ids::{
    AlignerId, ConflictRef, Epoch, JobId, LocaleId, ProjectId, ProjectRef, RecordRef, ReportId,
    RequestId, Scope, ServerInstance, ViewSeq, WallMs,
};
use tc_api::jobs::{
    Cancel, Get, JobKind, JobOutcome, JobState, JobView, OpenPhase, Phase, Progress,
};
use tc_api::net::{NetView, Set, State as NetState};
use tc_api::prefs::{
    DraftUnit, PlaceChange, PlaceMode, PlaceUpdate, PlaceVerse, RecordPlace, SetAlignSuggestions,
    SetDraftUnit, SetLocale,
};
use tc_api::refusal::Message;
use tc_api::shown::{ConflictMarker, PersonLabel, Shown, Who};
use tc_api::test::{
    Crash, CrashPoint, FieldLog, FieldLogView, FieldStore, Hold, HoldPoint, Quiet, QuietView,
    Release, Reset, StoredText,
};
use tc_api::write::{
    Ack, ConflictBrief, NotSaved, ReadOnlyReason, SaveRefusal, SavedValue, Stored, TokenUpdate,
};
use tc_api::{ALL, Endpoint, Kind, ShownToken};
use tc_core::BookId;

/// An example value for a golden file.
trait Example {
    fn example() -> Self;
}

fn request(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

fn scope() -> Scope {
    Scope {
        project: ProjectRef::parse("kr_ulb").unwrap(),
        epoch: Epoch::new(3),
    }
}

fn token(body: &str) -> ShownToken {
    ShownToken::from_body(body).unwrap()
}

fn record(json: &str) -> RecordRef {
    RecordRef::parse(json).unwrap()
}

fn account() -> AccountView {
    AccountView {
        user: Some("jdoe".into()),
        kept: KeepState::Kept,
        notice: None,
    }
}

fn ack() -> Ack {
    let verse = record(r#"{"book":"TIT","chapter":1,"kind":"verse","verse":4}"#);
    Ack {
        view: ViewSeq::new(42),
        stored: Stored::Written,
        tokens: vec![TokenUpdate {
            record: verse.clone(),
            token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
        }],
        new_conflicts: vec![ConflictBrief {
            conflict: ConflictRef::parse(&"c".repeat(64)).unwrap(),
            record: verse.clone(),
            by: PersonLabel {
                who: Who::Username("juan".into()),
                computer: None,
            },
        }],
        saved: vec![SavedValue {
            record: verse,
            text: "Paul, a servant of God".into(),
        }],
        rows: vec![RailRowPatch {
            row: RailRow {
                index: 7,
                reference: "TIT 1:4".parse().unwrap(),
                title: "faith".into(),
                flags: RowFlags {
                    dot: Dot::Valid,
                    decided: true,
                    bookmarked: false,
                    commented: true,
                },
                term: Some(2),
                category: Some(Category::Note(NoteCategory::Figures)),
                marker: Some(ConflictMarker {
                    conflict: ConflictRef::parse(&"d".repeat(64)).unwrap(),
                    versions: 2,
                }),
            },
        }],
        disk_warning: None,
    }
}

impl Example for () {
    fn example() -> Self {}
}

impl<T: Example> Example for Option<T> {
    fn example() -> Self {
        Some(T::example())
    }
}

impl Example for Hello {
    fn example() -> Self {
        Hello {}
    }
}

impl Example for HelloView {
    fn example() -> Self {
        HelloView {
            api: tc_api::API_VERSION,
            build: BuildInfo {
                version: "0.1.0".into(),
                commit: "b3b965f".into(),
            },
            instance: ServerInstance::from_bytes([0xab; 16]),
            internet: false,
            account: account(),
            sessions: vec![OpenSession {
                at: scope(),
                id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
                mode: SessionMode::ReadOnly(ReadOnlyReason::CloudSynced),
                role: SessionRole::Screen,
            }],
            prefs: AppPrefsView {
                app_locale: LocaleId::parse("ar").unwrap(),
            },
            startup: StartupState::Preparing(StartupProgress {
                done: 1200,
                total: 3739,
            }),
        }
    }
}

impl Example for Quit {
    fn example() -> Self {
        Quit {
            request: request(1),
        }
    }
}

impl Example for ClientError {
    fn example() -> Self {
        ClientError {
            request: request(2),
            view: Some("draft".into()),
            project: Some(ProjectRef::parse("kr_ulb").unwrap()),
            message: "index out of bounds".into(),
            at: WallMs(1_792_000_000_000),
        }
    }
}

impl Example for SetLocale {
    fn example() -> Self {
        SetLocale {
            request: request(3),
            locale: LocaleId::parse("pt-BR").unwrap(),
        }
    }
}

impl Example for SetDraftUnit {
    fn example() -> Self {
        SetDraftUnit {
            at: scope(),
            request: request(4),
            unit: DraftUnit::Verse,
        }
    }
}

impl Example for SetAlignSuggestions {
    fn example() -> Self {
        SetAlignSuggestions {
            at: scope(),
            request: request(5),
            on: true,
        }
    }
}

impl Example for RecordPlace {
    fn example() -> Self {
        RecordPlace {
            at: scope(),
            request: request(6),
            // A translation note on the chapter's introduction (`TIT 1:intro`).
            place: PlaceUpdate::Book {
                book: BookId::Titus,
                mode: Some(PlaceMode::Check),
                chapter: Some(1),
                verse: PlaceChange::Set(PlaceVerse::parse("intro").unwrap()),
                tool: Some(Tool::TranslationNotes),
            },
        }
    }
}

impl Example for NetState {
    fn example() -> Self {
        NetState {}
    }
}

impl Example for Set {
    fn example() -> Self {
        Set {
            request: request(7),
            on: true,
        }
    }
}

impl Example for NetView {
    fn example() -> Self {
        NetView { on: true }
    }
}

impl Example for Get {
    fn example() -> Self {
        Get { job: JobId::new(9) }
    }
}

impl Example for JobView {
    fn example() -> Self {
        JobView {
            id: JobId::new(9),
            kind: JobKind::Open,
            project: Some(ProjectRef::parse("kr_ulb").unwrap()),
            state: JobState::Running(Progress {
                phase: Phase::Open(OpenPhase::ReadingChanges),
                done: 312,
                total: Some(1890),
                item: None,
            }),
        }
    }
}

impl Example for Cancel {
    fn example() -> Self {
        Cancel {
            request: request(8),
            job: JobId::new(9),
        }
    }
}

impl Example for AccountState {
    fn example() -> Self {
        AccountState {}
    }
}

impl Example for AccountView {
    fn example() -> Self {
        account()
    }
}

impl Example for SignIn {
    fn example() -> Self {
        SignIn {
            request: request(10),
            login: "jdoe".into(),
            password: "correct horse".into(),
            stay: true,
        }
    }
}

impl Example for CancelSignIn {
    fn example() -> Self {
        CancelSignIn {
            request: request(11),
        }
    }
}

impl Example for Resume {
    fn example() -> Self {
        Resume {
            request: request(12),
        }
    }
}

impl Example for ResumeView {
    fn example() -> Self {
        ResumeView {
            outcome: ResumeOutcome::Refused,
            account: AccountView {
                user: None,
                kept: KeepState::NotKept,
                notice: Some(AccountNotice::SavedRefused),
            },
        }
    }
}

impl Example for SignOut {
    fn example() -> Self {
        SignOut {
            request: request(13),
        }
    }
}

impl Example for ProfileLink {
    fn example() -> Self {
        ProfileLink {}
    }
}

impl Example for HelpLink {
    fn example() -> Self {
        HelpLink {
            url: WebUrl::parse("https://help.door43.org/en/knowledgebase").unwrap(),
        }
    }
}

impl Example for LinkView {
    fn example() -> Self {
        LinkView {
            url: WebUrl::parse("https://git.door43.org/jdoe").unwrap(),
        }
    }
}

impl Example for Compose {
    fn example() -> Self {
        Compose {
            report: Some(ReportId::new(17)),
        }
    }
}

impl Example for FeedbackDraft {
    fn example() -> Self {
        FeedbackDraft {
            attachment: "translationCore 0.1.0 · windows\nReport: sync, 2026-10-17T15:00:00Z\n{}\n"
                .into(),
        }
    }
}

impl Example for Quiet {
    fn example() -> Self {
        Quiet {}
    }
}

impl Example for QuietView {
    fn example() -> Self {
        QuietView { busy: false }
    }
}

impl Example for Crash {
    fn example() -> Self {
        Crash {
            request: request(14),
            at: CrashPoint::BeforeReply,
            path: Some("test/fieldStore".into()),
        }
    }
}

impl Example for Hold {
    fn example() -> Self {
        Hold {
            request: request(15),
            at: HoldPoint::Save,
        }
    }
}

impl Example for Release {
    fn example() -> Self {
        Release {
            request: request(16),
            at: HoldPoint::Save,
        }
    }
}

impl Example for FieldStore {
    fn example() -> Self {
        FieldStore {
            request: request(17),
            field: "verse-1".into(),
            shown: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            text: "Paul, a servant of God".into(),
        }
    }
}

impl Example for Ack {
    fn example() -> Self {
        ack()
    }
}

impl Example for FieldLog {
    fn example() -> Self {
        FieldLog {
            field: "verse-1".into(),
        }
    }
}

impl Example for FieldLogView {
    fn example() -> Self {
        FieldLogView {
            current: Shown {
                value: "Paul, a servant of God".into(),
                token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
                marker: None,
            },
            stores: vec![StoredText {
                request: request(17),
                text: "Paul, a servant of God".into(),
            }],
        }
    }
}

impl Example for Reset {
    fn example() -> Self {
        Reset {
            request: request(18),
        }
    }
}

fn golden_dir() -> PathBuf {
    common::crate_dir().join("tests/golden")
}

fn updating() -> bool {
    std::env::var_os("UPDATE_GOLDEN").is_some_and(|v| v == "1")
}

/// Compares `value` with the golden file, or writes it under `UPDATE_GOLDEN=1`. Returns the
/// problem, if any.
fn check_golden(relative: &str, value: &Value) -> Option<String> {
    let path = golden_dir().join(relative);
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    if updating() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return None;
    }
    match std::fs::read_to_string(&path) {
        Ok(on_disk) if on_disk == text => None,
        Ok(_) => Some(format!(
            "{relative} differs; run with UPDATE_GOLDEN=1 if that is meant"
        )),
        Err(_) => Some(format!("{relative} is missing; run with UPDATE_GOLDEN=1")),
    }
}

/// The golden of one endpoint, after checking that its JSON reads back as the same values.
fn endpoint_golden<E>() -> (String, Value)
where
    E: Endpoint + Example + PartialEq + std::fmt::Debug,
    E::Ok: Example + PartialEq + std::fmt::Debug,
{
    let request = serde_json::to_value(E::example()).unwrap();
    let ok = serde_json::to_value(E::Ok::example()).unwrap();
    let back: E = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(back, E::example(), "{} request reads back", E::PATH);
    let back: E::Ok = serde_json::from_value(ok.clone()).unwrap();
    assert_eq!(back, E::Ok::example(), "{} reply reads back", E::PATH);
    let golden = json!({
        "path": E::PATH,
        "kind": E::KIND,
        "net": E::NET,
        "request": request,
        "ok": ok,
    });
    (format!("endpoints/{}.json", E::PATH), golden)
}

macro_rules! endpoint_goldens {
    ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        vec![$( endpoint_golden::<$req>() ),*]
    };
}

fn all_endpoint_goldens() -> Vec<(String, Value)> {
    let mut all = tc_api::endpoint_table!(endpoint_goldens);
    all.extend(tc_api::test_hook_table!(endpoint_goldens));
    all
}

#[test]
fn every_endpoint_has_golden_json() {
    let goldens = all_endpoint_goldens();
    let covered: BTreeSet<String> = goldens
        .iter()
        .map(|(file, _)| {
            file.trim_start_matches("endpoints/")
                .trim_end_matches(".json")
                .to_owned()
        })
        .collect();
    let routed: BTreeSet<String> = ALL.iter().map(|e| e.path.to_owned()).collect();
    assert_eq!(
        covered, routed,
        "every routed endpoint has a golden and no other does"
    );
    let problems: Vec<String> = goldens
        .iter()
        .filter_map(|(file, value)| check_golden(file, value))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_request_refuses_a_field_it_does_not_know() {
    macro_rules! refuses_unknown {
        ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
            $( refuses_unknown_field::<$req>(); )*
        };
    }
    fn refuses_unknown_field<E: Endpoint + Example>() {
        let mut value = serde_json::to_value(E::example()).unwrap();
        value
            .as_object_mut()
            .unwrap_or_else(|| panic!("{} request is an object", E::PATH))
            .insert("sneaky".into(), json!(1));
        assert!(
            serde_json::from_value::<E>(value).is_err(),
            "{} accepted an unknown field",
            E::PATH
        );
    }
    tc_api::endpoint_table!(refuses_unknown);
    tc_api::test_hook_table!(refuses_unknown);
}

#[test]
fn commands_and_jobs_carry_a_request_id_and_queries_do_not() {
    for (file, golden) in all_endpoint_goldens() {
        let kind: Kind = serde_json::from_value(golden["kind"].clone()).unwrap();
        let has_request = golden["request"].get("request").is_some();
        assert_eq!(
            has_request,
            kind != Kind::Query,
            "{file}: a {kind:?} {} a request id",
            if has_request { "carries" } else { "lacks" }
        );
    }
}

fn wire_samples<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    samples: &[T],
) -> Value {
    common::assert_every_variant(samples);
    Value::Array(
        samples
            .iter()
            .map(|sample| {
                let value = serde_json::to_value(sample).unwrap();
                let back: T = serde_json::from_value(value.clone()).unwrap();
                assert_eq!(&back, sample);
                value
            })
            .collect(),
    )
}

fn api_errors() -> Vec<ApiError> {
    vec![
        ApiError::Version { server: 2 },
        ApiError::BadRequest {
            field: "locale".into(),
            reason: "not one of the interface languages".into(),
        },
        ApiError::StaleScope {
            current: Some(Epoch::new(4)),
        },
        ApiError::StaleSession,
        ApiError::RequestReused,
        ApiError::NotSaved(NotSaved::StaleScope),
        ApiError::Refused(tc_api::Refusal::Prefs(
            tc_api::refusal::PrefsRefusal::LocaleNotSaved,
        )),
        ApiError::NeedsInternet(tc_api::NetKind::Sync),
        ApiError::NeedsSignIn(SignInReason::PrivateRepository),
        ApiError::Busy(Flow::Sync),
        ApiError::Unavailable(Feature::Writes),
        ApiError::Internal {
            report: ReportId::new(17),
        },
    ]
}

fn not_saved() -> Vec<NotSaved> {
    vec![
        NotSaved::Refused(SaveRefusal::IncoherentView),
        NotSaved::Disk("The disk is full.".into()),
        NotSaved::ReadOnly(ReadOnlyReason::SecondWriter),
        NotSaved::LookAgain,
        NotSaved::PlanStale,
        NotSaved::StaleScope,
        NotSaved::BadToken,
    ]
}

fn events() -> Vec<Event> {
    vec![
        Event::Resync,
        Event::Internet { on: true },
        Event::Account(account()),
        Event::Job(JobView {
            id: JobId::new(9),
            kind: JobKind::Seed,
            project: None,
            state: JobState::Done(JobOutcome::Finished),
        }),
        Event::ProjectsChanged,
        Event::HomeCard {
            project: ProjectRef::parse("kr_ulb").unwrap(),
        },
        Event::Session {
            at: scope(),
            event: SessionEvent::Opened,
        },
        Event::Startup(StartupState::Ready),
    ]
}

fn session_events() -> Vec<SessionEvent> {
    let verse = record(r#"{"book":"TIT","chapter":1,"kind":"verse","verse":4}"#);
    vec![
        SessionEvent::Opened,
        SessionEvent::Closed,
        SessionEvent::Mode(SessionMode::Writable),
        SessionEvent::Changed {
            view: ViewSeq::new(43),
            scope: ChangeScope::Parts(ChangeParts {
                chapters: vec![ChapterRef {
                    book: BookId::Titus,
                    chapter: 1,
                }],
                books: vec![],
                stories: vec![],
                checks: vec![CheckListRef {
                    tool: Tool::TranslationNotes,
                    book: BookId::Titus,
                }],
                records: vec![RecordChange {
                    record: verse,
                    by: Some(PersonLabel {
                        who: Who::Teammate(2),
                        computer: Some("LAPTOP-7".into()),
                    }),
                }],
                aspects: vec![Aspect::Conflicts],
            }),
            cause: ChangeCause::Intake {
                from: vec![PersonLabel {
                    who: Who::Username("juan".into()),
                    computer: None,
                }],
            },
        },
        SessionEvent::Checkpoint(CheckpointOutcome::Failed {
            reason: "the index is locked".into(),
        }),
        SessionEvent::MaterializeBlocked {
            paths: vec!["ingredients/TIT.usfm".into()],
        },
        SessionEvent::AlignSave {
            aligner: AlignerId::new(5),
            state: AlignSave::Failed(NotSaved::Disk("The disk is full.".into())),
        },
    ]
}

#[test]
fn shared_types_have_golden_json() {
    let refusals: Vec<Value> = common::samples::refusals()
        .iter()
        .map(|refusal| {
            let vars: serde_json::Map<String, Value> = refusal
                .vars()
                .into_iter()
                .map(|(name, value)| (name.to_owned(), Value::String(value)))
                .collect();
            json!({
                "wire": refusal,
                "code": refusal.code(),
                "key": refusal.key(),
                "vars": vars,
                "detail": refusal.detail(),
            })
        })
        .collect();
    let frame = EventFrame {
        seq: 1001,
        instance: ServerInstance::from_bytes([0xab; 16]),
        event: Event::Session {
            at: scope(),
            event: SessionEvent::Changed {
                view: ViewSeq::new(44),
                scope: ChangeScope::Everything,
                cause: ChangeCause::LocalSave {
                    request: request(1),
                },
            },
        },
    };
    let files = [
        ("types/api-error.json", wire_samples(&api_errors())),
        ("types/not-saved.json", wire_samples(&not_saved())),
        ("types/events.json", wire_samples(&events())),
        ("types/session-events.json", wire_samples(&session_events())),
        (
            "types/event-frame.json",
            serde_json::to_value(frame).unwrap(),
        ),
        ("types/refusals.json", Value::Array(refusals)),
    ];
    let problems: Vec<String> = files
        .iter()
        .filter_map(|(file, value)| check_golden(file, value))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn files_under(dir: &Path, out: &mut BTreeSet<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.insert(path);
        }
    }
}

#[test]
fn no_golden_file_is_left_without_its_endpoint_or_type() {
    let mut expected: BTreeSet<PathBuf> = all_endpoint_goldens()
        .into_iter()
        .map(|(file, _)| golden_dir().join(file))
        .collect();
    for file in [
        "api-error",
        "not-saved",
        "events",
        "session-events",
        "event-frame",
        "refusals",
    ] {
        expected.insert(golden_dir().join(format!("types/{file}.json")));
    }
    let mut on_disk = BTreeSet::new();
    if golden_dir().exists() {
        files_under(&golden_dir(), &mut on_disk);
    }
    let stale: Vec<_> = on_disk.difference(&expected).collect();
    if updating() {
        for path in &stale {
            std::fs::remove_file(path).unwrap();
        }
        return;
    }
    assert!(stale.is_empty(), "golden files nothing writes: {stale:?}");
}
