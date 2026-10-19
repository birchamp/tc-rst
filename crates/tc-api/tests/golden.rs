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
    AccountNotice, AccountView, CancelSignIn, HelpLink, KeepState, KeychainState, LinkView,
    OrgChoice, Orgs, ProfileLink, RepoLink, Resume, ResumeOutcome, ResumeView, ShareTargets,
    SignIn, SignOut, State as AccountState, WebUrl,
};
use tc_api::align::AlignSave;
use tc_api::app::{
    AppPrefsView, BuildInfo, ClientError, Hello, HelloView, OpenSession, Quit, SessionRole,
};
use tc_api::buffer::{
    BufferEntry, BufferKey, BufferLine, DraftCall, FieldKey, FieldKind, FieldPlace, SentSave,
};
use tc_api::check::{
    Category, CheckListRef, CheckPlace, CheckUnit, Dot, NoteCategory, RailRow, RailRowPatch,
    RowFlags, Tool,
};
use tc_api::door43::{OpenDoor43, ProjectName, RepoSource, SyncProject};
use tc_api::error::{ApiError, Feature, Flow, SignInReason};
use tc_api::event::{
    Aspect, ChangeCause, ChangeParts, ChangeScope, ChapterRef, CheckpointOutcome, Event,
    EventFrame, RecordChange, SessionEvent, SessionMode, StartupProgress, StartupState,
};
use tc_api::feedback::{Compose, FeedbackDraft};
use tc_api::home::{
    BookTileView, Card, Door43Card, HomeCard, HomeView, LanguageSuggestion, Languages,
    LanguagesView, LastEditView, List, ProjectKind, RecentStory, RepoPlace, ResumePlace,
    StoriesView, StoryTileView, UnusableCard, UnusableReason, Versifications, VersificationsView,
};
use tc_api::ids::{
    AlignerId, ConflictRef, Epoch, JobId, LocaleId, ProjectId, ProjectRef, RecordRef, RefPart,
    ReportId, RequestId, Scope, ServerInstance, StoryNo, UploadId, ViewSeq, WallMs,
};
use tc_api::jobs::{
    Cancel, Get, JobKind, JobOutcome, JobState, JobView, OpenPhase, Phase, Progress, SyncResultView,
};
use tc_api::net::{NetView, Set, State as NetState};
use tc_api::prefs::{
    DraftUnit, PlaceChange, PlaceMode, PlaceUpdate, RecordPlace, SetAlignSuggestions, SetDraftUnit,
    SetLocale,
};
use tc_api::refusal::Message;
use tc_api::shown::{ConflictMarker, Direction, Known, PersonLabel, Shown, Who};
use tc_api::test::{
    Crash, CrashPoint, Echo, EchoView, EchoWord, FieldDraft, FieldLog, FieldLogView, FieldStore,
    Hold, HoldPoint, Keychain, KeychainHolds, KeychainMode, KeychainView, LastEdit, Quiet,
    QuietView, Release, Reset, SaveVerse, Script, Scripted, ScriptedSync, StoredDraft, StoredText,
    TeammatesEdit, TokenBase,
};
use tc_api::upload::{UploadFlow, UploadView};
use tc_api::write::{
    Ack, ConflictBrief, NotSaved, ReadOnlyReason, SaveRefusal, SavedValue, Stored, TokenUpdate,
};
use tc_api::{ALL, Endpoint, Kind, Reply, ReplyOf, ShownToken};
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
        server: "git.door43.org".into(),
        keychain: KeychainState::Works,
    }
}

fn verse_place(book: BookId, chapter: &str, verse: &str) -> CheckPlace {
    CheckPlace::Verse {
        book,
        chapter: RefPart::parse(chapter).unwrap(),
        verse: RefPart::parse(verse).unwrap(),
    }
}

fn frame_place(story: u8, frame: u16) -> CheckPlace {
    CheckPlace::Frame {
        story: StoryNo::new(story).unwrap(),
        frame,
    }
}

/// Every form of place a check has: numbers, a Psalms superscription, a verse list and a
/// span, a note on an introduction, and the frames of a story, its title included.
fn check_places() -> Vec<CheckPlace> {
    vec![
        verse_place(BookId::Titus, "1", "4"),
        verse_place(BookId::Psalms, "60", "front"),
        verse_place(BookId::Psalms, "5", "1,3,8,12"),
        verse_place(BookId::Psalms, "5", "2-3"),
        verse_place(BookId::Titus, "1", "intro"),
        verse_place(BookId::Titus, "front", "intro"),
        frame_place(3, 4),
        frame_place(3, 0),
    ]
}

fn check_units() -> Vec<CheckUnit> {
    vec![CheckUnit::Book(BookId::Psalms), CheckUnit::Stories]
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
                place: verse_place(BookId::Titus, "1", "4"),
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
                verse: PlaceChange::Set(RefPart::parse("intro").unwrap()),
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
            sign_in: request(10),
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
                server: "git.door43.org".into(),
                keychain: KeychainState::Works,
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

impl Example for JobId {
    fn example() -> Self {
        JobId::new(21)
    }
}

impl Example for RepoLink {
    fn example() -> Self {
        RepoLink {
            url: WebUrl::parse("https://git.door43.org/Equipo-Ejemplo/sample_burrito").unwrap(),
        }
    }
}

impl Example for Orgs {
    fn example() -> Self {
        Orgs {
            language: "es-419".into(),
        }
    }
}

impl Example for ShareTargets {
    fn example() -> Self {
        ShareTargets {
            user: "facilitator-zq".into(),
            orgs: vec![
                OrgChoice {
                    name: "Equipo-Ejemplo".into(),
                    full_name: "Equipo Ejemplo".into(),
                    can_create: true,
                    recommended: true,
                },
                OrgChoice {
                    name: "es-419_gl".into(),
                    full_name: "Latin American Spanish Gateway".into(),
                    can_create: false,
                    recommended: false,
                },
            ],
        }
    }
}

impl Example for OpenDoor43 {
    fn example() -> Self {
        OpenDoor43 {
            request: request(20),
            repo: RepoSource::new("facilitator-zq", "es-419_tit_equipo").unwrap(),
            name: ProjectName::parse("es-419_tit_equipo").unwrap(),
        }
    }
}

impl Example for SyncProject {
    fn example() -> Self {
        SyncProject {
            request: request(21),
            project: ProjectRef::parse("sample_burrito").unwrap(),
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

impl Example for Echo {
    fn example() -> Self {
        Echo { size: 20_480 }
    }
}

/// Short and fixed, so the golden file stays readable; `EchoView::of_size` makes the long ones.
impl Example for EchoView {
    fn example() -> Self {
        EchoView {
            words: vec![
                EchoWord {
                    text: "בְּרֵאשִׁ֖ית".into(),
                    occurrence: 1,
                    occurrences: 1,
                    index: 0,
                },
                EchoWord {
                    text: "בָּרָ֣א".into(),
                    occurrence: 1,
                    occurrences: 2,
                    index: 1,
                },
            ],
        }
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

impl Example for SaveVerse {
    fn example() -> Self {
        SaveVerse {
            at: scope(),
            request: request(18),
            shown: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            words: "Paul, a servant of God".into(),
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
                base: TokenBase::Current,
            }],
            draft: Some(StoredDraft {
                request: request(19),
                text: "Paul, a servant of God and".into(),
                base: TokenBase::Current,
            }),
            drafts: 2,
        }
    }
}

impl Example for FieldDraft {
    fn example() -> Self {
        FieldDraft {
            request: request(19),
            field: "verse-1".into(),
            shown: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            text: "Paul, a servant of God and".into(),
        }
    }
}

impl Example for Keychain {
    fn example() -> Self {
        Keychain {
            request: request(19),
            mode: Some(KeychainMode::FailingForget),
            holds: Some(KeychainHolds::Token("kept-token".into())),
            relaunch: true,
        }
    }
}

impl Example for KeychainView {
    fn example() -> Self {
        KeychainView {
            mode: KeychainMode::FailingForget,
            holds: true,
            keeps: 0,
            reads: 1,
            forgets: 0,
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

impl Example for List {
    fn example() -> Self {
        List {}
    }
}

fn bible_card() -> HomeCard {
    HomeCard {
        project: ProjectRef::parse("sample_burrito").unwrap(),
        name: "Español Biblia".into(),
        tag: Known::Known("es-419".into()),
        dir: Direction::Ltr,
        kind: ProjectKind::Bible,
        door43: Door43Card::Shared {
            place: RepoPlace {
                owner: "jdoe".into(),
                name: "es-419_tit_text_reg".into(),
                server: Some("qa.door43.org".into()),
            },
        },
        books: Known::Known(vec![
            BookTileView {
                book: BookId::Titus,
                drafted: Known::Known(11),
                conflicts: 1,
            },
            BookTileView {
                book: BookId::Jonah,
                drafted: Known::Unknown,
                conflicts: 0,
            },
        ]),
        stories: None,
        conflicts: Known::Known(1),
    }
}

fn story_card() -> HomeCard {
    HomeCard {
        project: ProjectRef::parse("sample_burrito_obs").unwrap(),
        name: "Historias".into(),
        tag: Known::Unknown,
        dir: Direction::Rtl,
        kind: ProjectKind::Stories,
        door43: Door43Card::Local,
        books: Known::Known(Vec::new()),
        stories: Some(StoriesView {
            drafted: Known::Known(1),
            stories: vec![
                StoryTileView {
                    story: StoryNo::new(1).unwrap(),
                    title: "La Creación".into(),
                    drafted: Known::Known(13),
                },
                StoryTileView {
                    story: StoryNo::new(2).unwrap(),
                    title: String::new(),
                    drafted: Known::Unknown,
                },
            ],
            recent: vec![RecentStory {
                story: StoryNo::new(1).unwrap(),
                at: WallMs(1_792_378_282_100),
            }],
        }),
        conflicts: Known::Unknown,
    }
}

impl Example for HomeView {
    fn example() -> Self {
        HomeView {
            cards: vec![story_card(), bible_card()],
            unusable: vec![
                UnusableCard {
                    name: "broken".into(),
                    reason: UnusableReason::NoProjectFile,
                },
                UnusableCard {
                    name: "damaged".into(),
                    reason: UnusableReason::ProjectFileDamaged {
                        reason: "projectId is not a UUID".into(),
                    },
                },
            ],
            others: vec!["Notes".into(), "afile".into()],
            resume: Some(LastEditView {
                project: ProjectRef::parse("sample_burrito").unwrap(),
                place: ResumePlace::Book {
                    book: BookId::Titus,
                    chapter: 1,
                    verse: RefPart::parse("1").unwrap(),
                },
                mode: PlaceMode::Draft,
                tool: None,
                snippet: "Pablo, siervo de Dios".into(),
            }),
        }
    }
}

impl Example for Card {
    fn example() -> Self {
        Card {
            project: ProjectRef::parse("sample_burrito").unwrap(),
        }
    }
}

impl Example for HomeCard {
    fn example() -> Self {
        bible_card()
    }
}

impl Example for Versifications {
    fn example() -> Self {
        Versifications {}
    }
}

impl Example for VersificationsView {
    fn example() -> Self {
        VersificationsView {
            names: vec!["eng".into(), "org".into(), "lxx".into()],
        }
    }
}

impl Example for Languages {
    fn example() -> Self {
        Languages {
            query: "Lebanese Arabic".into(),
        }
    }
}

impl Example for LanguagesView {
    fn example() -> Self {
        LanguagesView {
            suggestions: vec![
                LanguageSuggestion {
                    name: "Lebanese Arabic".into(),
                    own: Some("عربي لبناني".into()),
                    code: Some("acm-x-lebarabi-c".into()),
                    door43: Some("acm-x-lebarabic".into()),
                    dir: Direction::Rtl,
                },
                LanguageSuggestion {
                    name: "Hausa".into(),
                    own: None,
                    code: Some("ha".into()),
                    door43: None,
                    dir: Direction::Ltr,
                },
                LanguageSuggestion {
                    name: "Unnamed".into(),
                    own: None,
                    code: None,
                    door43: Some("-x-".into()),
                    dir: Direction::Ltr,
                },
            ],
        }
    }
}

impl Example for LastEdit {
    fn example() -> Self {
        LastEdit {
            request: request(19),
            project: ProjectRef::parse("sample_burrito").unwrap(),
            place: ResumePlace::Story {
                story: StoryNo::new(3).unwrap(),
                frame: 2,
            },
            mode: PlaceMode::Draft,
            snippet: "Pablo,  xapóstol".into(),
            at: WallMs(1_792_378_282_100),
        }
    }
}

impl Example for Script {
    fn example() -> Self {
        Script {
            request: request(22),
            next: Scripted::Sync {
                project: ProjectRef::parse("sample_burrito").unwrap(),
                outcome: ScriptedSync::Synced {
                    teammates: Some(TeammatesEdit {
                        book: BookId::Titus,
                        chapter: 1,
                        verse: 1,
                        texts: vec!["Pablo, siervo".into(), "Pablo, esclavo".into()],
                    }),
                    restored: 0,
                },
            },
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

/// An error the endpoint can answer: the internet question's for an endpoint that asks it,
/// else a bug's report, which any endpoint can give.
fn error_example<E: Endpoint>() -> ApiError {
    match E::NET {
        Some(kind) => ApiError::NeedsInternet(kind),
        None => ApiError::Internal {
            report: ReportId::new(9),
        },
    }
}

/// The golden of one endpoint: the request and the whole HTTP 200 body of a reply and of an
/// error, after checking that each reads back as the same value.
fn endpoint_golden<E>() -> (String, Value)
where
    E: Endpoint + Example + PartialEq + std::fmt::Debug,
    E::Ok: Example + PartialEq + std::fmt::Debug,
{
    let request = serde_json::to_value(E::example()).unwrap();
    let back: E = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(back, E::example(), "{} request reads back", E::PATH);
    let mut bodies = Vec::new();
    for body in [
        ReplyOf::<E>::Ok(E::Ok::example()),
        ReplyOf::<E>::Err(error_example::<E>()),
    ] {
        let value = serde_json::to_value(&body).unwrap();
        let back: ReplyOf<E> = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(back, body, "{} reply reads back", E::PATH);
        bodies.push(value);
    }
    let [reply, error] = <[Value; 2]>::try_from(bodies).unwrap();
    let golden = json!({
        "path": E::PATH,
        "kind": E::KIND,
        "net": E::NET,
        "request": request,
        "reply": reply,
        "error": error,
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

/// The scope a write's example names, and its path: `Some` exactly when the example's JSON
/// has a top-level `at` that reads as a [`Scope`]. Other writes have an `at` of another kind
/// (`app/clientError`'s time, `test/crash`'s point) or none.
fn write_scope<E: tc_api::Write + Example>() -> (&'static str, Option<Scope>, Option<Scope>) {
    let example = E::example();
    let json = serde_json::to_value(&example).unwrap();
    let in_json = json
        .get("at")
        .and_then(|at| serde_json::from_value::<Scope>(at.clone()).ok());
    (E::PATH, example.at().cloned(), in_json)
}

macro_rules! write_scopes {
    ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {{
        let mut out = Vec::new();
        $( write_scopes!(@row out $kind $req); )*
        out
    }};
    (@row $out:ident query $req:path) => {};
    (@row $out:ident command $req:path) => {
        $out.push(write_scope::<$req>());
    };
    (@row $out:ident job $req:path) => {
        $out.push(write_scope::<$req>());
    };
}

/// tc-app checks a write's scope before its reply cache (APP-DESIGN §4.3, §4.4), so every
/// write must name the project it applies to exactly when its request carries one.
#[test]
fn a_write_names_its_scope_exactly_when_its_json_has_at() {
    let mut rows = tc_api::endpoint_table!(write_scopes);
    rows.extend(tc_api::test_hook_table!(write_scopes));
    let writes = ALL
        .iter()
        .filter(|e| matches!(e.kind, Kind::Command | Kind::StartJob))
        .count();
    assert_eq!(rows.len(), writes, "every write row is checked");
    for (path, at, in_json) in &rows {
        assert_eq!(at, in_json, "{path}");
    }
    let scoped: BTreeSet<&str> = rows
        .iter()
        .filter(|(_, at, _)| at.is_some())
        .map(|(path, _, _)| *path)
        .collect();
    assert_eq!(
        scoped,
        BTreeSet::from([
            "prefs/recordPlace",
            "prefs/setAlignSuggestions",
            "prefs/setDraftUnit",
            "test/saveVerse",
        ])
    );
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
        let request = serde_json::to_value(E::example()).unwrap();
        let accepted = common::unknown_key_accepted_in::<E>(&request)
            .unwrap_or_else(|e| panic!("{}: {e}", E::PATH));
        assert!(
            accepted.is_empty(),
            "{} accepted an unknown field in {accepted:?} of {request}",
            E::PATH
        );
    }
    tc_api::endpoint_table!(refuses_unknown);
    tc_api::test_hook_table!(refuses_unknown);
}

#[test]
fn the_unknown_field_check_reaches_every_nested_object_and_catches_a_lax_one() {
    // The check must visit the objects a request nests: a scope, an enum's envelope and its
    // body, and the envelope of an enum inside that body.
    let place = serde_json::to_value(RecordPlace::example()).unwrap();
    assert_eq!(
        common::object_pointers(&place),
        ["", "/at", "/place", "/place/value", "/place/value/verse"]
    );

    #[derive(Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Strict {
        inner: Lax,
    }
    #[derive(Serialize, serde::Deserialize)]
    struct Lax {
        n: u8,
    }
    let request = json!({"inner": {"n": 1}});
    assert_eq!(
        common::unknown_key_accepted_in::<Strict>(&request),
        Ok(vec!["/inner".to_owned()]),
        "a nested struct that reads an unknown key is found, and only it"
    );
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
    plain_samples(samples)
}

/// The wire forms of `samples`, after checking that each reads back as the same value. For a
/// type that is not a tagged enum, such as a structure or a value written as text.
fn plain_samples<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    samples: &[T],
) -> Value {
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

/// The flows an upload names, in the header's words, and the view the server answers with.
fn uploads() -> Value {
    json!({
        "flows": wire_samples(&UploadFlow::ALL),
        "view": plain_samples(&[UploadView {
            id: UploadId::parse("00112233445566778899aabbccddeeff").unwrap(),
            name: "Tito – 1.usfm".into(),
            size: 53_248,
        }]),
    })
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
        Event::Job(JobView {
            id: JobId::new(10),
            kind: JobKind::Sync,
            project: Some(ProjectRef::parse("sample_burrito").unwrap()),
            state: JobState::Done(JobOutcome::Synced(SyncResultView {
                received: 2,
                from: vec![
                    PersonLabel {
                        who: Who::Teammate(1),
                        computer: None,
                    },
                    PersonLabel {
                        who: Who::Username("maria".into()),
                        computer: None,
                    },
                ],
                kept_newest: 0,
                verse_conflicts: 1,
                alignments: 0,
                restored: 0,
                missing_pins: 5,
            })),
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
                stories: vec![StoryNo::new(12).unwrap()],
                checks: vec![
                    CheckListRef {
                        tool: Tool::TranslationNotes,
                        unit: CheckUnit::Book(BookId::Titus),
                    },
                    CheckListRef {
                        tool: Tool::TranslationWords,
                        unit: CheckUnit::Stories,
                    },
                ],
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

/// The draft buffer's item keys, entries and log lines (§6.6): the forms the UI, `boot.js`,
/// the shell and the server read.
fn buffer() -> Value {
    let verse = BufferKey {
        project: ProjectRef::parse("en_ult | draft \"2\"").unwrap(),
        id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse("TIT 1:4").unwrap(),
        },
    };
    let comment = BufferKey {
        field: FieldKey {
            kind: FieldKind::CheckComment,
            place: FieldPlace::parse("tn TIT 1:4 abc1").unwrap(),
        },
        ..verse.clone()
    };
    let saving = BufferEntry {
        api: 1,
        instance: ServerInstance::from_bytes([0xab; 16]),
        base: vec![token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")],
        text: "Paul, a servant of God and".into(),
        sent: Some(SentSave {
            request: request(20),
            text: "Paul, a servant of God".into(),
            scope: Some(Scope {
                project: verse.project.clone(),
                epoch: Epoch::new(3),
            }),
        }),
        at: WallMs(1_760_000_000_123),
        draft: Some(DraftCall {
            path: "test/fieldDraft".into(),
            body: r#"{"field":"verse-1","shown":"st1.eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"}"#.into(),
        }),
    };
    let typed = BufferEntry {
        sent: None,
        draft: None,
        text: "\u{5d1}\u{5bc}\u{5b0}".into(),
        ..saving.clone()
    };
    let item = |key: &BufferKey| {
        format!(
            "{}{}",
            tc_api::buffer::BUFFER_ITEM_PREFIX,
            serde_json::to_string(key).unwrap()
        )
    };
    json!({
        "itemKeys": [item(&verse), item(&comment)],
        "keys": plain_samples(&[verse.clone(), comment.clone()]),
        "entries": plain_samples(&[saving.clone(), typed.clone()]),
        "lines": wire_samples(&[
            BufferLine::Changed { key: verse.clone(), entry: saving },
            BufferLine::Changed { key: comment.clone(), entry: typed },
            BufferLine::Removed { key: verse, instance: ServerInstance::from_bytes([0xab; 16]) },
        ]),
    })
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
        ("types/uploads.json", uploads()),
        ("types/buffer.json", buffer()),
        ("types/check-places.json", plain_samples(&check_places())),
        ("types/check-units.json", wire_samples(&check_units())),
        (
            "types/check-lists.json",
            plain_samples(
                &[Tool::TranslationNotes, Tool::TranslationWords]
                    .into_iter()
                    .flat_map(|tool| {
                        check_units()
                            .into_iter()
                            .map(move |unit| CheckListRef { tool, unit })
                    })
                    .collect::<Vec<_>>(),
            ),
        ),
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
        "uploads",
        "check-places",
        "check-units",
        "check-lists",
        "buffer",
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

#[test]
fn a_reply_is_tagged_like_every_other_enum() {
    use tc_api::net::NetView;
    let ok: Reply<NetView> = Reply::Ok(NetView { on: true });
    assert_eq!(
        serde_json::to_value(&ok).unwrap(),
        json!({"type": "ok", "value": {"on": true}})
    );
    let err: Reply<NetView> = Reply::Err(ApiError::RequestReused);
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({"type": "err", "value": {"type": "requestReused"}})
    );
    assert_eq!(
        serde_json::to_value(Reply::<()>::Ok(())).unwrap(),
        json!({"type": "ok", "value": null})
    );
    // Serde's own form of `Result` is not the wire form.
    assert!(serde_json::from_value::<Reply<NetView>>(json!({"Ok": {"on": true}})).is_err());
    assert_eq!(ok.into_result(), Ok(NetView { on: true }));
    assert_eq!(
        Reply::from(Err::<(), _>(ApiError::StaleSession)),
        Reply::Err(ApiError::StaleSession)
    );
}

/// Every object in `value` with where it is, for messages.
fn objects<'a>(
    value: &'a Value,
    at: String,
    out: &mut Vec<(String, &'a serde_json::Map<String, Value>)>,
) {
    match value {
        Value::Object(map) => {
            out.push((at.clone(), map));
            for (key, inner) in map {
                objects(inner, format!("{at}.{key}"), out);
            }
        }
        Value::Array(items) => {
            for (i, inner) in items.iter().enumerate() {
                objects(inner, format!("{at}[{i}]"), out);
            }
        }
        _ => {}
    }
}

fn is_camel_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric())
}

#[test]
fn every_golden_file_follows_the_wire_rules() {
    // APP-DESIGN §3.1: camelCase fields, and every enum that carries data, the reply
    // envelope included, as {"type": <camelCase variant>, "value": <data>}.
    let mut files = BTreeSet::new();
    files_under(&golden_dir(), &mut files);
    assert!(files.len() > 20, "{} golden files", files.len());
    let mut problems = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let name = path
            .strip_prefix(golden_dir())
            .unwrap()
            .display()
            .to_string();
        let mut found = Vec::new();
        objects(&value, name, &mut found);
        for (at, map) in found {
            for key in map.keys().filter(|key| !is_camel_case(key)) {
                problems.push(format!("{at}: key {key:?} is not camelCase"));
            }
            if let Some(tag) = map.get("type") {
                if !tag.as_str().is_some_and(is_camel_case) {
                    problems.push(format!("{at}: tag {tag} is not a camelCase name"));
                }
                if map.keys().any(|key| key != "type" && key != "value") {
                    problems.push(format!("{at}: a tagged enum holds other keys"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
