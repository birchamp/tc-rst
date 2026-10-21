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
use tc_api::align::{
    AlignEdit, AlignPlace, AlignSave, AlignerView, BankClick, BankSelection, Blocked,
    Card as AlignCard, Close as AlignClose, DroppedLinkView, Edit as AlignEditRequest, Flush,
    Freshness, GatewayLens, Index, IndexItem, IndexView, KeepReflow, Open as AlignOpen, OpenVerse,
    Opened, RailState, RailStatus, Suggest as AlignSuggest, SuggestStatus, SuggestView, Testament,
    Word,
};
use tc_api::app::{
    AppPrefsView, BuildInfo, ClientError, Hello, HelloView, OpenSession, Quit, SessionRole,
};
use tc_api::buffer::{
    BufferEntry, BufferKey, BufferLine, DraftCall, FieldKey, FieldKind, FieldPlace, SentSave,
};
use tc_api::check::{
    AlignProgress, CardCount, Category, CheckArticle, CheckArticleRead, CheckCollection,
    CheckEmpty, CheckItemView, CheckListRef, CheckNotice, CheckPlace, CheckProgress,
    CheckSessionView, CheckUnit, Decide, DecideEdit, DecisionStatus, DecisionView, Dot,
    DroppedNote, GatewayText, GatewayView, Item, NextCheck, NoteCategory, Open as CheckOpen,
    OriginalView, Picker, PickerView, PinCitation, RailProgress, RailRow, RailRowPatch, RailRows,
    RailSince, ResourceCitation, RowFlags, Rows, RowsView, Rung, StoryItem, StoryItemView,
    StoryPane, TargetWords, Testament as CheckTestament, Tool, ToolCard, ToolState,
};
use tc_api::create::{
    AddBlankBooks, AddUsfmBooks, Bible, BooksAdded, Created, Details, Obs, ReviewUsfm, UsfmAdded,
    UsfmFile, UsfmOutcome, UsfmProblem, UsfmRow, UsfmVerdict,
};
use tc_api::door43::{OpenDoor43, ProjectName, RepoSource, SyncProject};
use tc_api::editor::{
    AddComment, Book, BookView, ChapterText, Chapters, ChaptersView, CommentView,
    DiscardDraft as EditorDiscardDraft, Draft as EditorDraft, DraftField as EditorDraftField,
    FrameRow, PaneChapter, PaneText, PaneVerse, RailBook, RowSource, SaveLayout, SaveSection,
    SaveVerse, SourceFrame, SourcePane, Sources, SourcesView, VerseRow,
};
use tc_api::error::{ApiError, Feature, Flow, SignInReason};
use tc_api::event::{
    Aspect, ChangeCause, ChangeParts, ChangeScope, ChapterRef, CheckpointOutcome, Event,
    EventFrame, RecordChange, SessionEvent, SessionMode, StartupProgress, StartupState,
};
use tc_api::export;
use tc_api::feedback::{Compose, FeedbackDraft};
use tc_api::helps::{
    Article, ArticleLink, ArticleView, LitVerse, NoteCard, PaneFocus, Passage, PassageView,
    ReadySlot, SimplifiedVerse, Slot, Story as HelpsStory, StoryArticle, WordCard,
};
use tc_api::home::{
    BookTileView, Card, Door43Card, HomeCard, HomeView, LanguageSuggestion, Languages,
    LanguagesView, LastEditView, List, ProjectKind, RecentStory, RepoPlace, ResumePlace,
    StoriesView, StoryTileView, UnusableCard, UnusableReason, Versifications, VersificationsView,
};
use tc_api::ids::{
    AlignerId, ChangeRef, CheckSessionId, ConflictRef, Epoch, JobId, LocaleId, ProjectId,
    ProjectRef, RecordRef, RefPart, ReportId, RequestId, Scope, ServerInstance, StoryNo, UploadId,
    ViewSeq, WallMs,
};
use tc_api::jobs::{
    Cancel, Get, JobKind, JobOutcome, JobState, JobView, OpenPhase, Phase, Progress, SyncResultView,
};
use tc_api::model::section::ParaFormat;
use tc_api::model::{ArticleBlock, ArticleSpan, BlockKind};
use tc_api::net::{NetView, Set, State as NetState};
use tc_api::prefs::{
    DraftUnit, PlaceChange, PlaceMode, PlaceUpdate, RecordPlace, SetAlignSuggestions, SetDraftUnit,
    SetLocale,
};
use tc_api::refusal::Message;
use tc_api::resources::{
    ApplyPlan, BlockReason, BlockedUnit, Carry, Catalog, CatalogView, CheckUpdates, Download,
    FixFetch, FixRepin, FixSideload, FixState, FixView, FromInstalls, GatewayChange,
    GatewayPackage, GatewayRef, ImageChange, OfferRef, PackageRole, PackageRow, PinLabel,
    PlanApplied, PlanGateway, PlanRow, PlanUpgrade, PlanView, PlanWhat, RepoMove, Seen, SetOffer,
    SetRung, SourceGateway, Sources as SourceTexts, SourcesView as SourceTextsView, TextKind,
    TextOffer, UpdatesView,
};
use tc_api::session::{
    BufferSlot, Close, CopyProject, DiscardDraft, DraftKind, DraftOffer, Drafts, Keep, KeepDraft,
    KeptDraft, KeptPlace, OfferedDrafts, Open, OpenUnit, OpenedProject, OtherComputerClosed,
    ProjectCopy, Settles, StoredDraftId,
};
use tc_api::settings::{
    CurrentGateway, GatewayCard, Save, SettingsView, ShownValues, ViewFromHome,
};
use tc_api::shown::{ConflictMarker, Direction, Known, PersonLabel, Shown, Who};
use tc_api::story;
use tc_api::test::{
    Crash, CrashPoint, Echo, EchoView, EchoWord, FieldDraft, FieldLog, FieldLogView, FieldStore,
    Hold, HoldPoint, Keychain, KeychainHolds, KeychainMode, KeychainView, LastEdit, Quiet,
    QuietView, Release, Reset, Script, Scripted, ScriptedSync, StoredDraft, StoredText,
    TeammatesEdit, TokenBase,
};
use tc_api::upload::{UploadFlow, UploadView};
use tc_api::write::{
    Ack, ConflictBrief, NotSaved, ReadOnlyReason, SaveRefusal, SavedValue, Stored, TokenUpdate,
};
use tc_api::{ALL, Endpoint, Kind, Reply, ReplyOf, ReviewToken, ShownToken};
use tc_core::BookId;

use tc_api::conflicts::{
    ConflictEntry, ConflictKind, DetachedEntry, DetachedKind, History, HistoryVersion, HistoryView,
    List as ConflictList, MadeBy, NoticeEntry, NoticeView, Resolve, ReviewEntries, ReviewPlace,
    ReviewTab, ReviewView, TabCount, Version, VersionCause, VersionValue,
};

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
    vec![
        CheckUnit::Book(BookId::Psalms),
        CheckUnit::Story(StoryNo::new(7).unwrap()),
    ]
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
                check_id: "abcd".into(),
                place: verse_place(BookId::Titus, "1", "4"),
                title: "faith".into(),
                flags: RowFlags {
                    dot: Dot::Valid,
                    decided: true,
                    invalidated: false,
                    invalid: false,
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

impl Example for ViewFromHome {
    fn example() -> Self {
        ViewFromHome {
            project: ProjectRef::parse("es-419_tit_equipo").unwrap(),
        }
    }
}

impl Example for Save {
    fn example() -> Self {
        Save {
            request: request(40),
            project: ProjectRef::parse("es-419_tit_equipo").unwrap(),
            shown: ShownValues {
                direction: Direction::Ltr,
                font: None,
            },
            direction: Direction::Rtl,
            font: "Scheherazade New — Arabic script".into(),
        }
    }
}

fn details() -> Details {
    Details {
        name: "Kanuri Bible".into(),
        language_name: "Kanuri".into(),
        code: "acm-x-lebarabi-c".into(),
        door43_code: Some("acm-x-lebarabic".into()),
        direction: Direction::Rtl,
        font: "Noto Sans (default)".into(),
    }
}

impl Example for Bible {
    fn example() -> Self {
        Bible {
            request: request(41),
            details: details(),
            versification: "eng".into(),
        }
    }
}

impl Example for Obs {
    fn example() -> Self {
        Obs {
            request: request(42),
            details: details(),
        }
    }
}

impl Example for Created {
    fn example() -> Self {
        Created {
            project: ProjectRef::parse("kanuri_bible").unwrap(),
        }
    }
}

impl Example for AddBlankBooks {
    fn example() -> Self {
        AddBlankBooks {
            request: request(43),
            project: ProjectRef::parse("kanuri_bible").unwrap(),
            books: vec![BookId::Titus, BookId::Philemon],
        }
    }
}

impl Example for BooksAdded {
    fn example() -> Self {
        BooksAdded {
            added: vec![BookId::Titus],
        }
    }
}

fn usfm_file(name: &str) -> UsfmFile {
    UsfmFile {
        upload: UploadId::parse("00112233445566778899aabbccddeeff").unwrap(),
        name: name.into(),
    }
}

impl Example for ReviewUsfm {
    fn example() -> Self {
        ReviewUsfm {
            file: usfm_file("57-TIT.usfm"),
        }
    }
}

impl Example for UsfmVerdict {
    fn example() -> Self {
        UsfmVerdict::Book {
            book: BookId::Titus,
        }
    }
}

impl Example for AddUsfmBooks {
    fn example() -> Self {
        AddUsfmBooks {
            request: request(44),
            project: ProjectRef::parse("kanuri_bible").unwrap(),
            files: vec![usfm_file("57-TIT.usfm"), usfm_file("old-TIT.usfm")],
        }
    }
}

impl Example for UsfmAdded {
    fn example() -> Self {
        UsfmAdded {
            rows: vec![
                UsfmRow {
                    name: "57-TIT.usfm".into(),
                    outcome: UsfmOutcome::Added {
                        book: BookId::Titus,
                    },
                },
                UsfmRow {
                    name: "old-TIT.usfm".into(),
                    outcome: UsfmOutcome::Refused(UsfmProblem::TwoFiles {
                        book: BookId::Titus,
                        first: "57-TIT.usfm".into(),
                    }),
                },
            ],
        }
    }
}

fn spanish() -> GatewayRef {
    GatewayRef {
        language: "es-419".into(),
        owner: "es-419_gl".into(),
    }
}

fn english_package() -> GatewayPackage {
    GatewayPackage {
        gateway: GatewayRef {
            language: "en".into(),
            owner: "unfoldingWord".into(),
        },
        name: "English".into(),
        autonym: "English".into(),
        direction: Direction::Ltr,
        seen: Seen {
            tn: "v89".into(),
            tw: "v89".into(),
            ta: "v89".into(),
        },
    }
}

impl Example for SettingsView {
    fn example() -> Self {
        SettingsView {
            language_name: "Español".into(),
            direction: Direction::Ltr,
            font: Some("Charis SIL".into()),
            gateway: GatewayCard {
                current: Some(CurrentGateway {
                    gateway: spanish(),
                    autonym: "Español (Latinoamérica)".into(),
                }),
                options: FromInstalls::Listed(vec![english_package()]),
            },
        }
    }
}

impl Example for SourceTexts {
    fn example() -> Self {
        SourceTexts { at: scope() }
    }
}

impl Example for SourceTextsView {
    fn example() -> Self {
        SourceTextsView {
            gateways: vec![SourceGateway {
                package: english_package(),
                installed: 1,
            }],
            current: Some(spanish()),
            checkable: FromInstalls::Failed {
                reason: "resources/dcs: permission denied".into(),
            },
            pinned: true,
        }
    }
}

impl Example for Catalog {
    fn example() -> Self {
        Catalog {
            at: scope(),
            gateway: spanish(),
            book: BookId::Titus,
        }
    }
}

impl Example for CatalogView {
    fn example() -> Self {
        CatalogView {
            rows: vec![
                PackageRow {
                    role: PackageRole::Text,
                    repo: "es-419_glt".into(),
                    desc: "Texto Puente Literal".into(),
                },
                PackageRow {
                    role: PackageRole::WordLinks,
                    repo: "es-419_twl".into(),
                    desc: "Enlaces de palabras".into(),
                },
            ],
        }
    }
}

impl Example for Download {
    fn example() -> Self {
        Download {
            request: request(21),
            at: scope(),
            gateway: spanish(),
            book: BookId::Titus,
            repos: vec!["es-419_glt".into(), "es-419_tn".into()],
        }
    }
}

fn tn_move() -> RepoMove {
    RepoMove {
        repo: "es-419_tn".into(),
        from: "v66".into(),
        to: "v67".into(),
        published: Some("2026-10-02T14:03:11Z".into()),
    }
}

impl Example for CheckUpdates {
    fn example() -> Self {
        CheckUpdates { at: scope() }
    }
}

impl Example for UpdatesView {
    fn example() -> Self {
        UpdatesView {
            sets: vec![
                SetOffer {
                    rung: SetRung::Primary,
                    language: "es-419".into(),
                    moves: vec![tn_move()],
                },
                SetOffer {
                    rung: SetRung::Fallback,
                    language: "en".into(),
                    moves: Vec::new(),
                },
            ],
            texts: vec![TextOffer {
                kind: TextKind::Original,
                change: RepoMove {
                    repo: "el-x-koine_ugnt".into(),
                    from: "v0.34".into(),
                    to: "v0.35".into(),
                    published: None,
                },
            }],
            texts_error: Some("Door43 did not answer".into()),
        }
    }
}

impl Example for PlanUpgrade {
    fn example() -> Self {
        PlanUpgrade {
            request: request(22),
            at: scope(),
            offer: OfferRef::Set {
                rung: SetRung::Primary,
            },
        }
    }
}

impl Example for PlanGateway {
    fn example() -> Self {
        PlanGateway {
            request: request(23),
            at: scope(),
            gateway: GatewayRef {
                language: "en".into(),
                owner: "unfoldingWord".into(),
            },
        }
    }
}

impl Example for ApplyPlan {
    fn example() -> Self {
        ApplyPlan {
            request: request(24),
            at: scope(),
            plan: tc_api::ids::PlanId::new(3),
        }
    }
}

fn gateway_plan() -> PlanView {
    PlanView {
        plan: tc_api::ids::PlanId::new(4),
        what: PlanWhat::Gateway(Box::new(GatewayChange {
            from: Some(spanish_package()),
            to: english_package(),
            images: Some(ImageChange {
                from: None,
                to: Some("en_obs-images".into()),
            }),
        })),
        decisions_at_risk: 4,
        rows: vec![
            PlanRow {
                tool: Tool::TranslationWords,
                unit: CheckCollection::Book(BookId::Titus),
                decisions: 2,
                against: "es-419_twl".into(),
                carry: Some(Carry {
                    carried: 2,
                    check_again: 0,
                }),
            },
            PlanRow {
                tool: Tool::TranslationNotes,
                unit: CheckCollection::Book(BookId::Titus),
                decisions: 2,
                against: "es-419_tn".into(),
                carry: None,
            },
        ],
        blocked: vec![BlockedUnit {
            tool: Tool::TranslationNotes,
            unit: CheckCollection::Stories,
            reason: BlockReason::VersificationUnknown,
        }],
        alignments: Some(12),
    }
}

fn spanish_package() -> GatewayPackage {
    GatewayPackage {
        gateway: spanish(),
        name: "Latin American Spanish".into(),
        autonym: "Español (Latinoamérica)".into(),
        direction: Direction::Ltr,
        seen: Seen {
            tn: "v66".into(),
            tw: "v66".into(),
            ta: "v66".into(),
        },
    }
}

impl Example for PlanApplied {
    fn example() -> Self {
        PlanApplied::Replanned(Box::new(PlanView {
            what: PlanWhat::Set {
                rung: SetRung::Primary,
                language: "es-419".into(),
                moves: vec![tn_move()],
            },
            ..gateway_plan()
        }))
    }
}

impl Example for PlanView {
    fn example() -> Self {
        PlanView {
            what: PlanWhat::Repin {
                rung: SetRung::Fallback,
                language: "en".into(),
                moves: vec![tn_move()],
            },
            ..gateway_plan()
        }
    }
}

impl Example for FixState {
    fn example() -> Self {
        FixState {
            at: scope(),
            tool: Tool::TranslationWords,
            unit: CheckUnit::Book(BookId::Titus),
        }
    }
}

fn pin_label(version: Option<&str>, sha: &str) -> PinLabel {
    PinLabel {
        repo_path: "git.door43.org/es-419_gl/es-419_twl".into(),
        version: version.map(str::to_owned),
        sha: sha.into(),
    }
}

impl Example for FixView {
    fn example() -> Self {
        FixView {
            pin: pin_label(Some("v18"), "1111111111111111111111111111111111111111"),
            rung: SetRung::Primary,
            candidates: vec![pin_label(
                Some("v19"),
                "2222222222222222222222222222222222222222",
            )],
            candidates_error: None,
        }
    }
}

impl Example for FixFetch {
    fn example() -> Self {
        FixFetch {
            request: request(25),
            at: scope(),
            tool: Tool::TranslationWords,
            unit: CheckUnit::Book(BookId::Titus),
        }
    }
}

impl Example for FixSideload {
    fn example() -> Self {
        FixSideload {
            request: request(26),
            at: scope(),
            tool: Tool::TranslationWords,
            unit: CheckUnit::Book(BookId::Titus),
            upload: tc_api::ids::UploadId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        }
    }
}

impl Example for FixRepin {
    fn example() -> Self {
        FixRepin {
            request: request(27),
            at: scope(),
            tool: Tool::TranslationWords,
            unit: CheckUnit::Book(BookId::Titus),
            sha: "2222222222222222222222222222222222222222".into(),
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

impl Example for SaveSection {
    fn example() -> Self {
        SaveSection {
            at: scope(),
            request: request(19),
            book: BookId::Titus,
            displayed: vec![
                token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
                token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            ],
            text: "1-2 Paul, a servant of God\n\t3 an apostle".into(),
        }
    }
}

impl Example for SaveLayout {
    fn example() -> Self {
        SaveLayout {
            at: scope(),
            request: request(20),
            shown: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            layout: "\n\\q1\n".into(),
        }
    }
}

impl Example for AddComment {
    fn example() -> Self {
        Self {
            at: scope(),
            request: request(23),
            target: tc_api::editor::CommentTarget::Verse {
                book: BookId::Titus,
                chapter: 1,
                verse: 4,
            },
            shown: None,
            text: "Discuss this verse with the team.".into(),
        }
    }
}
impl Example for story::SaveFrame {
    fn example() -> Self {
        Self {
            at: scope(),
            request: request(24),
            shown: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            text: "Así fue como Dios hizo todo.".into(),
        }
    }
}
impl Example for story::SaveRef {
    fn example() -> Self {
        Self {
            at: scope(),
            request: request(25),
            shown: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            text: "Una historia de la Biblia: Génesis 1-2".into(),
        }
    }
}

impl Example for EditorDraft {
    fn example() -> Self {
        EditorDraft {
            at: scope(),
            request: request(21),
            instance: ServerInstance::from_bytes([7; 16]),
            field: EditorDraftField::Section {
                book: BookId::Titus,
            },
            shown: SaveSection::example().displayed,
            text: "1-2 Paul, a servant of God\n\t3 an apostle".into(),
            changed: WallMs(1_792_000_000_000),
        }
    }
}

impl Example for EditorDiscardDraft {
    fn example() -> Self {
        let draft = EditorDraft::example();
        Self {
            at: draft.at,
            request: request(22),
            instance: draft.instance,
            field: draft.field,
            shown: draft.shown,
            text: draft.text,
            changed: draft.changed,
        }
    }
}

impl Example for Ack {
    fn example() -> Self {
        ack()
    }
}

fn offered_key() -> BufferKey {
    BufferKey {
        project: ProjectRef::parse("kr_ulb").unwrap(),
        id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse("TIT 1:4").unwrap(),
        },
    }
}

impl Example for Drafts {
    fn example() -> Self {
        let entry = BufferEntry {
            api: 1,
            instance: ServerInstance::from_bytes([0xab; 16]),
            base: vec![token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")],
            text: "To Titus, a true son".into(),
            sent: None,
            at: WallMs(1_760_000_000_123),
            draft: None,
        };
        Drafts {
            at: scope(),
            request: request(90),
            mirror: true,
            buffered: vec![
                serde_json::to_string(&BufferLine::Changed {
                    key: offered_key(),
                    entry,
                })
                .unwrap(),
            ],
        }
    }
}

impl Example for OfferedDrafts {
    fn example() -> Self {
        let slot = BufferSlot {
            key: offered_key(),
            instance: ServerInstance::from_bytes([0xab; 16]),
            text: "To Titus, a true son".into(),
            at: WallMs(1_760_000_000_123),
        };
        OfferedDrafts {
            offers: vec![
                DraftOffer {
                    kind: DraftKind::Words,
                    place: FieldPlace::parse("TIT 1:4").unwrap(),
                    text: "To Titus, a true son".into(),
                    keep: Keep::Save,
                    shown: vec![token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")],
                    settles: Settles {
                        stored: vec![StoredDraftId(1)],
                        buffered: vec![slot.clone()],
                    },
                },
                DraftOffer {
                    kind: DraftKind::Section,
                    place: FieldPlace::parse("TIT 1:5").unwrap(),
                    text: "{\"tokens\":[],\"buffer\":\"For this reason\"}".into(),
                    keep: Keep::CopyText,
                    shown: Vec::new(),
                    settles: Settles {
                        stored: vec![StoredDraftId(2)],
                        buffered: Vec::new(),
                    },
                },
                DraftOffer {
                    kind: DraftKind::CheckComment,
                    place: FieldPlace::parse("TIT 1:6").unwrap(),
                    text: "check the word for elder".into(),
                    keep: Keep::AsVersion,
                    shown: vec![token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")],
                    settles: Settles::default(),
                },
            ],
            settle: vec![BufferSlot {
                text: "Paul, a servant of God".into(),
                ..slot
            }],
        }
    }
}

impl Example for KeepDraft {
    fn example() -> Self {
        KeepDraft {
            at: scope(),
            request: request(30),
            kind: DraftKind::Words,
            shown: vec![token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")],
            text: "To Titus, a true son".into(),
            discard: vec![StoredDraftId(1)],
        }
    }
}

impl Example for KeptDraft {
    fn example() -> Self {
        let ack = ack();
        KeptDraft {
            view: ack.view,
            stored: Stored::Written,
            new_conflicts: ack.new_conflicts,
        }
    }
}

impl Example for DiscardDraft {
    fn example() -> Self {
        DiscardDraft {
            at: scope(),
            request: request(31),
            stored: vec![StoredDraftId(1), StoredDraftId(2)],
        }
    }
}

impl Example for CopyProject {
    fn example() -> Self {
        CopyProject {
            at: scope(),
            request: request(32),
        }
    }
}

impl Example for ProjectCopy {
    fn example() -> Self {
        ProjectCopy {
            project: ProjectRef::parse("kr_ulb - Copy").unwrap(),
            id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
        }
    }
}

impl Example for OtherComputerClosed {
    fn example() -> Self {
        OtherComputerClosed {
            at: scope(),
            request: request(33),
        }
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

// ---- B3: open, the book, the source panes, the helps ---------------------------------------

fn opened() -> OpenedProject {
    OpenedProject {
        session: OpenSession {
            at: Scope {
                project: ProjectRef::parse("sample_burrito").unwrap(),
                epoch: Epoch::new(4),
            },
            id: ProjectId::parse("0a1b2c3d-0000-4000-8000-000000000001").unwrap(),
            mode: SessionMode::Writable,
            role: SessionRole::Screen,
        },
        name: "Equipo Ejemplo — Tito y Jonás".into(),
        tag: "es-419".into(),
        dir: Direction::Ltr,
        kind: ProjectKind::Bible,
        books: vec![BookId::Jonah, BookId::Titus],
        draft_unit: DraftUnit::Section,
        place: Some(KeptPlace {
            mode: PlaceMode::Check,
            chapter: 2,
            tool: Some(Tool::TranslationNotes),
        }),
    }
}

impl Example for Open {
    fn example() -> Self {
        Open {
            request: request(30),
            project: ProjectRef::parse("sample_burrito").unwrap(),
            unit: Some(OpenUnit::Book(BookId::Titus)),
        }
    }
}

impl Example for Close {
    fn example() -> Self {
        Close {
            request: request(31),
            at: scope(),
        }
    }
}

impl Example for Book {
    fn example() -> Self {
        Book {
            at: scope(),
            book: BookId::Titus,
        }
    }
}

impl Example for BookView {
    fn example() -> Self {
        BookView {
            view: ViewSeq::new(42),
            book: BookId::Titus,
            chapters: vec![1, 2, 3],
            rail: vec![
                RailBook {
                    book: BookId::Jonah,
                    drafted: Known::Unknown,
                },
                RailBook {
                    book: BookId::Titus,
                    drafted: Known::Known(11),
                },
            ],
            comments: vec![CommentView {
                token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
                chapter: 1,
                verse: 4,
                text: "Pablo escribe a Tito.".into(),
                at: WallMs(1_792_378_282_100),
            }],
        }
    }
}

impl Example for Sources {
    fn example() -> Self {
        Sources {
            at: scope(),
            book: BookId::Titus,
            chapter: 1,
        }
    }
}

impl Example for SourcesView {
    fn example() -> Self {
        SourcesView {
            panes: vec![
                SourcePane {
                    id: "ult".into(),
                    version: Some("v91".into()),
                    dir: Direction::Ltr,
                    text: PaneText::Ready(PaneChapter {
                        sections: vec![1, 4],
                        verses: vec![PaneVerse {
                            key: RefPart::parse("1").unwrap(),
                            text: "Paul, a servant of God".into(),
                            tokens: vec![
                                "Paul".into(),
                                ", ".into(),
                                "a".into(),
                                " ".into(),
                                "servant".into(),
                                " ".into(),
                                "of".into(),
                                " ".into(),
                                "God".into(),
                            ],
                            lead: None,
                            before: Some(0),
                        }],
                    }),
                },
                SourcePane {
                    id: "ust".into(),
                    version: Some("v88".into()),
                    dir: Direction::Ltr,
                    text: PaneText::NotInstalled,
                },
                SourcePane {
                    id: "glt".into(),
                    version: None,
                    dir: Direction::Rtl,
                    text: PaneText::Error {
                        reason: "the book could not be read".into(),
                    },
                },
            ],
            original: Some(SourcePane {
                id: "orig".into(),
                version: Some("v0.34".into()),
                dir: Direction::Ltr,
                text: PaneText::Missing,
            }),
            // A project in another numbering: `rsc` Jonah 2 holds `eng` 1:17 and 2:1-10.
            frame: SourceFrame::Mapped {
                rows: vec![
                    FrameRow {
                        verse: RefPart::parse("1").unwrap(),
                        source: RowSource::Verse {
                            chapter: 1,
                            verse: RefPart::parse("17").unwrap(),
                        },
                    },
                    FrameRow {
                        verse: RefPart::parse("2").unwrap(),
                        source: RowSource::OtherBook {
                            to: "NEH 1:1".into(),
                        },
                    },
                    FrameRow {
                        verse: RefPart::parse("3").unwrap(),
                        source: RowSource::Unmapped,
                    },
                ],
            },
        }
    }
}

impl Example for Chapters {
    fn example() -> Self {
        Chapters {
            at: scope(),
            book: BookId::Titus,
        }
    }
}

impl Example for ChaptersView {
    fn example() -> Self {
        let words = |value: &str, marker: Option<ConflictMarker>| Shown {
            value: value.into(),
            token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            marker,
        };
        ChaptersView {
            view: ViewSeq::new(42),
            chapters: vec![ChapterText {
                chapter: 3,
                sections: vec![1, 4],
                verses: vec![
                    VerseRow {
                        key: RefPart::parse("1").unwrap(),
                        record: record(r#"{"book":"TIT","chapter":3,"kind":"verse","verse":1}"#),
                        drafted: true,
                        text: "Recuérdales que se sujeten".into(),
                        lead: None,
                        before: Some(0),
                        words: words("Recuérdales que se sujeten", None),
                        format: None,
                        covered: Vec::new(),
                    },
                    VerseRow {
                        key: RefPart::parse("2-3").unwrap(),
                        record: record(r#"{"book":"TIT","chapter":3,"kind":"verse","verse":2}"#),
                        drafted: true,
                        text: "que no hablen mal de nadie, que sean amables".into(),
                        lead: Some(1),
                        before: None,
                        words: words(
                            "\\q1 que no hablen mal de nadie,\n\\q2 que sean amables",
                            Some(ConflictMarker {
                                conflict: ConflictRef::parse(&"e".repeat(64)).unwrap(),
                                versions: 2,
                            }),
                        ),
                        format: Some(ParaFormat::Q1),
                        covered: vec![token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")],
                    },
                    VerseRow {
                        key: RefPart::parse("4").unwrap(),
                        record: record(r#"{"book":"TIT","chapter":3,"kind":"verse","verse":4}"#),
                        drafted: false,
                        text: String::new(),
                        lead: None,
                        before: None,
                        words: words("___", None),
                        format: None,
                        covered: Vec::new(),
                    },
                ],
            }],
            conflicts: 1,
        }
    }
}

impl Example for Passage {
    fn example() -> Self {
        Passage {
            at: scope(),
            book: BookId::Titus,
            chapter: 1,
        }
    }
}

fn spans(text: &str, bold: &str) -> Vec<ArticleSpan> {
    vec![
        ArticleSpan {
            text: bold.into(),
            bold: true,
            italic: false,
        },
        ArticleSpan {
            text: text.into(),
            bold: false,
            italic: false,
        },
    ]
}

impl Example for PassageView {
    fn example() -> Self {
        let focus = PaneFocus {
            pane: "ult".into(),
            title: Some("of God".into()),
            lit: vec![LitVerse {
                verse: RefPart::parse("1").unwrap(),
                tokens: vec![6, 8],
            }],
        };
        PassageView {
            notes: Slot::Ready(ReadySlot {
                items: vec![NoteCard {
                    id: "rtc9".into(),
                    chapter: RefPart::parse("1").unwrap(),
                    verse: RefPart::parse("1").unwrap(),
                    quote: "Θεοῦ".into(),
                    occurrence: 1,
                    note: vec![ArticleBlock::new(
                        BlockKind::P,
                        0,
                        spans(" is an abstract noun.", "Faith"),
                    )],
                    academy: Some("figs-abstractnouns".into()),
                    panes: vec![focus.clone()],
                }],
                dir: Some(Direction::Ltr),
                fallback: false,
            }),
            words: Slot::Ready(ReadySlot {
                items: vec![WordCard {
                    id: "x4ab".into(),
                    chapter: RefPart::parse("1").unwrap(),
                    verse: RefPart::parse("1").unwrap(),
                    quote: "Θεοῦ".into(),
                    occurrence: 1,
                    category: Some("kt".into()),
                    slug: "god".into(),
                    panes: vec![focus],
                }],
                dir: None,
                fallback: true,
            }),
            questions: Slot::Fetch,
            simplified: Slot::Ready(ReadySlot {
                items: vec![SimplifiedVerse {
                    key: RefPart::parse("1").unwrap(),
                    text: "I, Paul, serve God.".into(),
                    source: Some("1:1".into()),
                }],
                dir: Some(Direction::Ltr),
                fallback: false,
            }),
            simplified_pin: Some("en_ust".into()),
            other_book: None,
        }
    }
}

impl Example for Article {
    fn example() -> Self {
        Article {
            at: scope(),
            book: BookId::Titus,
            link: ArticleLink::Words {
                category: Some("kt".into()),
                slug: "god".into(),
            },
        }
    }
}

impl Example for ArticleView {
    fn example() -> Self {
        ArticleView::Found {
            title: "God".into(),
            path: "payload/kt/god.md".into(),
            blocks: vec![
                ArticleBlock::new(BlockKind::H, 0, spans("", "Definition:")),
                ArticleBlock {
                    depth: 1,
                    ..ArticleBlock::new(BlockKind::Li, 1, spans(" created the universe.", "God"))
                },
                ArticleBlock {
                    number: Some("1".into()),
                    ..ArticleBlock::new(BlockKind::Ol, 0, spans(" is one.", "God"))
                },
                ArticleBlock {
                    cells: vec![spans("", "Word"), spans("", "Meaning")],
                    header: true,
                    ..ArticleBlock::new(BlockKind::Tr, 0, spans(" Meaning", "Word"))
                },
            ],
            dir: Direction::Ltr,
        }
    }
}

// ---- B5a: the tool picker, a tool's rail, the guided fix -------------------------------------

fn titus_tn() -> CheckListRef {
    CheckListRef {
        tool: Tool::TranslationNotes,
        unit: CheckUnit::Book(BookId::Titus),
    }
}

fn pin_citation(repo: &str, version: Option<&str>, sha: char) -> PinCitation {
    PinCitation {
        repo_path: format!("git.door43.org/{repo}"),
        version: version.map(str::to_owned),
        sha: sha.to_string().repeat(40),
    }
}

fn align_word(word: &str, occurrence: u32, occurrences: u32) -> Word {
    Word {
        word: word.to_owned(),
        occurrence,
        occurrences,
    }
}

impl Example for Index {
    fn example() -> Self {
        Index {
            at: scope(),
            book: BookId::Titus,
        }
    }
}

impl Example for IndexView {
    fn example() -> Self {
        IndexView {
            items: vec![
                IndexItem {
                    place: AlignPlace {
                        chapter: 1,
                        verse: 1,
                        last: 1,
                    },
                    text: "Pablo, siervo de Dios".into(),
                    status: RailStatus {
                        state: RailState::Todo,
                        placed: 6,
                        total: 27,
                    },
                },
                IndexItem {
                    place: AlignPlace {
                        chapter: 1,
                        verse: 9,
                        last: 10,
                    },
                    text: String::new(),
                    status: RailStatus {
                        state: RailState::Undrafted,
                        placed: 0,
                        total: 0,
                    },
                },
            ],
        }
    }
}

impl Example for AlignOpen {
    fn example() -> Self {
        AlignOpen {
            at: scope(),
            request: request(1),
            book: BookId::Titus,
            verse: Some(OpenVerse {
                chapter: 1,
                verse: 1,
            }),
        }
    }
}

impl Example for Opened {
    fn example() -> Self {
        Opened::Ready(Box::new(AlignerView::example()))
    }
}

impl Example for AlignerView {
    fn example() -> Self {
        AlignerView {
            aligner: AlignerId::new(5),
            seq: 3,
            place: AlignPlace {
                chapter: 1,
                verse: 1,
                last: 1,
            },
            testament: Testament::Nt,
            target_dir: Direction::Ltr,
            orig_dir: Direction::Ltr,
            original: "Παῦλος δοῦλος Θεοῦ".into(),
            gateway: Some(GatewayLens {
                text: "Paul, a servant of God".into(),
            }),
            bank: vec![align_word("de", 1, 2), align_word("Dios", 1, 1)],
            selection: BankSelection::Armed(vec![0]),
            cards: vec![
                AlignCard {
                    index: 0,
                    top: vec![align_word("Παῦλος", 1, 1)],
                    placed: vec![align_word("Pablo", 1, 1)],
                    suggested: vec![],
                    gateway: Some("Paul".into()),
                },
                AlignCard {
                    index: 1,
                    top: vec![align_word("δοῦλος", 1, 1), align_word("Θεοῦ", 1, 1)],
                    placed: vec![],
                    suggested: vec![align_word("siervo", 1, 1)],
                    gateway: None,
                },
            ],
            placed: 1,
            total: 4,
            done: false,
            mark_refused: false,
            suggest: SuggestView {
                on: true,
                status: SuggestStatus::Ready {
                    verses: 5,
                    boosted: false,
                },
                asked: true,
                standing: 1,
                suggesting: false,
            },
            freshness: Freshness::Stale,
            dropped: vec![DroppedLinkView {
                card: 1,
                top: vec![align_word("δοῦλος", 1, 1)],
                word: align_word("esclavo", 1, 1),
            }],
            status: RailStatus {
                state: RailState::Invalid,
                placed: 1,
                total: 4,
            },
            save: AlignSave::Unsaved,
            marker: None,
        }
    }
}

impl Example for AlignEditRequest {
    fn example() -> Self {
        AlignEditRequest {
            at: scope(),
            request: request(2),
            aligner: AlignerId::new(5),
            seq: 3,
            edit: AlignEdit::ClickBank {
                word: align_word("de", 1, 2),
                click: BankClick::Toggle,
            },
        }
    }
}

impl Example for KeepReflow {
    fn example() -> Self {
        KeepReflow {
            at: scope(),
            request: request(3),
            aligner: AlignerId::new(5),
            seq: 3,
        }
    }
}

impl Example for AlignSuggest {
    fn example() -> Self {
        AlignSuggest {
            at: scope(),
            request: request(6),
            aligner: AlignerId::new(5),
        }
    }
}

impl Example for Flush {
    fn example() -> Self {
        Flush {
            at: scope(),
            request: request(4),
        }
    }
}

impl Example for AlignClose {
    fn example() -> Self {
        AlignClose {
            at: scope(),
            request: request(5),
            aligner: AlignerId::new(5),
        }
    }
}

impl Example for AlignSave {
    fn example() -> Self {
        AlignSave::Failed(NotSaved::Disk("The disk is full.".into()))
    }
}

/// Every edit, so the wire form of each is in a file.
fn align_edits() -> Vec<AlignEdit> {
    vec![
        AlignEdit::ClickBank {
            word: align_word("Dios", 1, 1),
            click: BankClick::Range,
        },
        AlignEdit::PlaceSelection { card: 1 },
        AlignEdit::PlaceWords {
            card: 1,
            words: vec![align_word("de", 2, 2)],
        },
        AlignEdit::Unplace {
            card: 0,
            word: align_word("Pablo", 1, 1),
        },
        AlignEdit::Move {
            from: 0,
            to: 1,
            word: align_word("Pablo", 1, 1),
        },
        AlignEdit::Merge { from: 0, to: 1 },
        AlignEdit::Split { card: 1 },
        AlignEdit::MarkValid,
        AlignEdit::Accept {
            card: 1,
            word: align_word("siervo", 1, 1),
        },
        AlignEdit::AcceptAll,
        AlignEdit::RejectAll,
    ]
}

fn align_blocked() -> Vec<Blocked> {
    vec![
        Blocked::Undrafted,
        Blocked::Unpinned,
        Blocked::Missing,
        Blocked::Unreadable,
        Blocked::VersificationUnavailable,
        Blocked::VersificationUnknown,
        Blocked::NoCounterpart,
        Blocked::AlignedInText,
    ]
}

fn align_opened() -> Vec<Opened> {
    vec![
        Opened::example(),
        Opened::Blocked {
            why: Blocked::Missing,
            at: Some(OpenVerse {
                chapter: 1,
                verse: 6,
            }),
        },
        Opened::Blocked {
            why: Blocked::Undrafted,
            at: None,
        },
    ]
}

fn suggest_statuses() -> Vec<SuggestStatus> {
    vec![
        SuggestStatus::Off,
        SuggestStatus::Reading,
        SuggestStatus::None,
        SuggestStatus::Ready {
            verses: 1,
            boosted: false,
        },
        SuggestStatus::Error {
            reason: "the engine stopped".into(),
        },
    ]
}

impl Example for Picker {
    fn example() -> Self {
        Picker {
            at: scope(),
            unit: CheckUnit::Book(BookId::Titus),
        }
    }
}

impl Example for PickerView {
    fn example() -> Self {
        PickerView {
            tools: vec![
                ToolCard {
                    tool: Tool::TranslationWords,
                    state: ToolState::Fetch,
                    pin: Some(pin_citation("es-419_gl/es-419_twl", Some("v18"), 'c')),
                    rung: Some(Rung::Primary),
                    fallback_for: None,
                    progress: None,
                },
                ToolCard {
                    tool: Tool::TranslationNotes,
                    state: ToolState::Ready,
                    pin: Some(pin_citation("es-419_gl/es-419_tn", Some("v20"), 'a')),
                    rung: Some(Rung::Primary),
                    fallback_for: None,
                    progress: Some(CardCount::Counted(CheckProgress {
                        decided: 1,
                        total: 112,
                        dropped: Some(DroppedNote {
                            count: 2,
                            scheme: Some("rsc".into()),
                        }),
                        next: Some(NextCheck {
                            title: "knowledge".into(),
                            place: verse_place(BookId::Titus, "1", "1"),
                        }),
                    })),
                },
            ],
            align: Some(CardCount::Counted(AlignProgress {
                done: 0,
                total: 5,
                next: Some("1:1".into()),
            })),
            old_twl: None,
        }
    }
}

impl Example for HelpsStory {
    fn example() -> Self {
        HelpsStory {
            at: scope(),
            story: StoryNo::new(1).unwrap(),
        }
    }
}

impl Example for StoryArticle {
    fn example() -> Self {
        StoryArticle {
            at: scope(),
            link: ArticleLink::Academy {
                slug: "figs-metaphor".into(),
            },
        }
    }
}

impl Example for story::List {
    fn example() -> Self {
        story::List { at: scope() }
    }
}

impl Example for story::StoryList {
    fn example() -> Self {
        story::StoryList {
            stories: vec![
                story::StoryRow {
                    story: StoryNo::new(1).unwrap(),
                    title: "La Creación".into(),
                    drafted: Known::Known(13),
                },
                story::StoryRow {
                    story: StoryNo::new(2).unwrap(),
                    title: String::new(),
                    drafted: Known::Unknown,
                },
            ],
        }
    }
}

impl Example for story::Open {
    fn example() -> Self {
        story::Open {
            at: scope(),
            story: StoryNo::new(1).unwrap(),
        }
    }
}

impl Example for story::StoryView {
    fn example() -> Self {
        story::StoryView {
            view: ViewSeq::new(42),
            title_token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            reference_token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            story: StoryNo::new(1).unwrap(),
            title: "La Creación".into(),
            reference: "Una historia de la Biblia: Génesis 1-2".into(),
            frames: vec![
                story::FrameView {
                    token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
                    text: "Así fue como Dios hizo todo.".into(),
                    image: Some(
                        "/media/obs/uW~obs_images_360~7146d5b504f6b63b9e11f7dc0b18c594d0ae179d/360px/obs-en-01-01.jpg"
                            .into(),
                    ),
                },
                story::FrameView {
                    token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
                    text: String::new(),
                    image: None,
                },
            ],
            gateway: Some(story::GatewayStory {
                title: "The Creation".into(),
                reference: "A Bible story from: Genesis 1-2".into(),
                frames: vec!["This is how God made everything.".into(), String::new()],
            }),
            source: Some(story::SourceNotice::Mismatch {
                message: "the gateway story has 15 frames, the project's has 16".into(),
            }),
            source_version: Some("v9".into()),
            image_note: Some(story::ImageNote {
                wanted: 16,
                packs: vec![
                    story::PackReport {
                        pin: "git.door43.org/uW/obs_images_360@7146d5b504f6".into(),
                        installed_at: None,
                        via: None,
                        files: 0,
                        error: Some("not installed".into()),
                    },
                    story::PackReport {
                        pin: "git.door43.org/uW/obs_images_361@aaaaaaaaaaaa".into(),
                        installed_at: Some("/home/ana/resources/obs_images_361".into()),
                        via: Some(story::PackVia::Metadata),
                        files: 0,
                        error: Some("the listing could not be read".into()),
                    },
                ],
            }),
            comments: vec![story::FrameComment {
                token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
                frame: 0,
                text: "Check the title with the team.".into(),
                at: WallMs(1_790_000_000_000),
            }],
        }
    }
}

impl Example for StoryItem {
    fn example() -> Self {
        StoryItem {
            at: scope(),
            list: CheckListRef {
                tool: Tool::TranslationNotes,
                unit: CheckUnit::Story(StoryNo::new(1).unwrap()),
            },
            index: 0,
        }
    }
}

impl Example for StoryItemView {
    fn example() -> Self {
        StoryItemView {
            index: 0,
            place: frame_place(1, 0),
            title: "The Creation".into(),
            note: vec![ArticleBlock::new(
                BlockKind::P,
                0,
                spans(
                    " can also be translated as: “About how God made the world”.",
                    "This title",
                ),
            )],
            note_dir: Some(Direction::Ltr),
            article: Some(ArticleLink::Academy {
                slug: "translate-titles".into(),
            }),
            gateway: StoryPane::Text {
                tokens: vec!["The".into(), " ".into(), "Creation".into()],
                lit: vec![0, 2],
            },
            target: "La Creación".into(),
        }
    }
}

impl Example for CheckOpen {
    fn example() -> Self {
        CheckOpen {
            at: scope(),
            list: titus_tn(),
            since: Some(RailSince {
                session: CheckSessionId::new(3),
                view: ViewSeq::new(40),
            }),
        }
    }
}

impl Example for CheckSessionView {
    fn example() -> Self {
        CheckSessionView {
            session: CheckSessionId::new(3),
            list: titus_tn(),
            view: ViewSeq::new(41),
            rows: RailRows::Full(vec![Ack::example().rows[0].row.clone()]),
            terms: vec![],
            progress: RailProgress {
                decided: 1,
                total: 112,
            },
            citation: Some(ResourceCitation {
                pin: pin_citation("es-419_gl/es-419_tn", Some("v20"), 'a'),
                rung: Rung::Primary,
            }),
            dropped: None,
            empty: Some(CheckEmpty::AllDropped),
            start: 7,
        }
    }
}

impl Example for Rows {
    fn example() -> Self {
        Rows {
            at: scope(),
            list: titus_tn(),
            since: RailSince {
                session: CheckSessionId::new(3),
                view: ViewSeq::new(41),
            },
        }
    }
}

impl Example for RowsView {
    fn example() -> Self {
        RowsView::Changed {
            view: ViewSeq::new(44),
            rows: Ack::example().rows,
            progress: RailProgress {
                decided: 2,
                total: 112,
            },
        }
    }
}

impl Example for Item {
    fn example() -> Self {
        Item {
            at: scope(),
            list: titus_tn(),
            index: 7,
        }
    }
}

impl Example for CheckArticleRead {
    fn example() -> Self {
        CheckArticleRead {
            at: scope(),
            list: titus_tn(),
            index: 7,
        }
    }
}

impl Example for CheckItemView {
    fn example() -> Self {
        CheckItemView {
            index: 7,
            view: ViewSeq::new(43),
            check_id: "abcd".into(),
            place: verse_place(BookId::Titus, "1", "1"),
            title: "knowledge".into(),
            note: vec![ArticleBlock::new(
                BlockKind::P,
                0,
                vec![ArticleSpan {
                    text: "The words faith and knowledge are abstract nouns.".into(),
                    ..ArticleSpan::default()
                }],
            )],
            helps_dir: Some(Direction::Ltr),
            article: CheckArticle {
                link: ArticleLink::Academy {
                    slug: "figs-abstractnouns".into(),
                },
                title: Some("Abstract Nouns".into()),
                category: Category::Note(NoteCategory::Grammar),
            },
            original: OriginalView::Ready {
                testament: CheckTestament::Nt,
                verse: Some("Παῦλος, δοῦλος Θεοῦ".into()),
            },
            gateway: GatewayView::Pane {
                pane: "ult".into(),
                text: GatewayText::Verse {
                    tokens: vec!["Paul".into(), ", ".into(), "a".into()],
                    lit: vec![0],
                },
            },
            target: TargetWords {
                words: vec!["Pablo".into(), "siervo".into()],
                selected: vec![0],
                verse: Some(token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")),
            },
            decision: Shown {
                value: DecisionView {
                    status: Some(DecisionStatus::Valid),
                    nothing_to_select: false,
                    bookmarked: true,
                    comment: Some("ask the team".into()),
                    invalidated: false,
                },
                token: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
                marker: None,
            },
            notices: vec![CheckNotice::Invalidated { count: 2 }],
        }
    }
}

impl Example for Decide {
    fn example() -> Self {
        Decide {
            at: scope(),
            request: request(23),
            list: titus_tn(),
            place: verse_place(BookId::Titus, "1", "1"),
            check_id: "abcd".into(),
            decision: token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"),
            verse: Some(token("eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA")),
            edit: DecideEdit::Valid { words: vec![0, 1] },
        }
    }
}

fn review_token(body: &str) -> ReviewToken {
    ReviewToken::from_body(body).unwrap()
}

fn titus_1_1() -> RecordRef {
    record(r#"{"book":"TIT","chapter":1,"kind":"verse","verse":1}"#)
}

fn version(change: char, who: Who, at: i64, shown: bool, text: &str) -> Version {
    Version {
        change: ChangeRef::parse(&change.to_string().repeat(64)).unwrap(),
        by: MadeBy::Person(PersonLabel {
            who,
            computer: None,
        }),
        at: WallMs(at),
        clock_ahead: false,
        shown,
        value: VersionValue::Text(text.into()),
    }
}

impl Example for ConflictList {
    fn example() -> Self {
        ConflictList {
            at: scope(),
            tab: ReviewTab::Text,
        }
    }
}

impl Example for ReviewView {
    /// Titus 1:1 after two teammates changed it from one version (the gallery's S4).
    fn example() -> Self {
        let counts = [2, 0, 0, 0, 0, 1];
        ReviewView {
            view: ViewSeq::new(12),
            tabs: ReviewTab::ALL
                .into_iter()
                .zip(counts)
                .map(|(tab, count)| TabCount { tab, count })
                .collect(),
            entries: ReviewEntries::Conflicts(vec![ConflictEntry {
                conflict: ConflictRef::parse(&"c".repeat(64)).unwrap(),
                kind: ConflictKind::VerseText,
                record: titus_1_1(),
                place: ReviewPlace::Verse {
                    book: BookId::Titus,
                    chapter: 1,
                    verse: 1,
                },
                group: 0,
                versions: vec![
                    version(
                        'b',
                        Who::Teammate(2),
                        1_790_812_920_000,
                        true,
                        "Pablo, versión del compañero 2.",
                    ),
                    version(
                        'a',
                        Who::Teammate(1),
                        1_790_812_860_000,
                        false,
                        "Pablo, versión del compañero 1.",
                    ),
                ],
                review: vec![review_token(
                    "eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA",
                )],
                incomplete_history: false,
                unreadable: 0,
            }]),
        }
    }
}

impl Example for History {
    fn example() -> Self {
        History {
            at: scope(),
            record: titus_1_1(),
        }
    }
}

impl Example for HistoryView {
    fn example() -> Self {
        HistoryView {
            versions: vec![
                HistoryVersion {
                    version: version(
                        'b',
                        Who::Mine,
                        1_790_812_920_000,
                        true,
                        "Pablo, siervo de Dios.",
                    ),
                    cause: VersionCause::Kept,
                },
                HistoryVersion {
                    version: Version {
                        by: MadeBy::Import,
                        ..version('a', Who::Mine, 1_790_000_000_000, false, "Pablo, siervo")
                    },
                    cause: VersionCause::Imported,
                },
            ],
        }
    }
}

impl Example for Resolve {
    fn example() -> Self {
        Resolve {
            at: scope(),
            request: request(24),
            conflict: ConflictRef::parse(&"c".repeat(64)).unwrap(),
            review: vec![review_token(
                "eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA",
            )],
            keep: ChangeRef::parse(&"a".repeat(64)).unwrap(),
        }
    }
}

/// Every notice the Notices tab words.
fn review_notices() -> Vec<NoticeView> {
    let file = || "ingredients/TIT.usfm".to_owned();
    vec![
        NoticeView::DamagedChanges { recovered: 212 },
        NoticeView::NewerApp,
        NoticeView::ClockAhead {
            by: Some(PersonLabel {
                who: Who::Username("juan".into()),
                computer: None,
            }),
            days: 3,
        },
        NoticeView::OutsideEditRecorded {
            file: file(),
            changes: 4,
        },
        NoticeView::OutsideEditHeld { file: file() },
        NoticeView::OutsideEditUnreadable { file: file() },
        NoticeView::DetailsEditedOutside,
        NoticeView::WriteBlocked { file: file() },
        NoticeView::FileRepaired { file: file() },
        NoticeView::FileMissing { file: file() },
        NoticeView::FileUnreadable { file: file() },
        NoticeView::StorageDamage { file: file() },
        NoticeView::FileMoved {
            file: "ingredients/tit.USFM".into(),
            to: file(),
        },
        NoticeView::FolderCopied,
        NoticeView::SecondWriter,
        NoticeView::SavedWithDiskError,
        NoticeView::ImportUnfinished,
        NoticeView::IncompleteChanges { have: 3, of: 5 },
        NoticeView::KeptApart { changes: 2 },
    ]
}

/// Every place a review entry names.
fn review_places() -> Vec<ReviewPlace> {
    vec![
        ReviewPlace::Verse {
            book: BookId::Titus,
            chapter: 1,
            verse: 4,
        },
        ReviewPlace::ChapterFront {
            book: BookId::Titus,
            chapter: 2,
        },
        ReviewPlace::Book {
            book: BookId::Jonah,
        },
        ReviewPlace::Check {
            tool: Some(Tool::TranslationNotes),
            place: verse_place(BookId::Titus, "1", "4"),
        },
        ReviewPlace::Story {
            story: StoryNo::new(3).unwrap(),
            frame: Some(4),
        },
        ReviewPlace::Project,
    ]
}

fn review_values() -> Vec<VersionValue> {
    vec![
        VersionValue::Text("Pablo, siervo de Dios".into()),
        VersionValue::Joined { by: 4 },
        VersionValue::Absent,
        VersionValue::NotShown,
    ]
}

fn review_makers() -> Vec<MadeBy> {
    vec![
        MadeBy::Person(PersonLabel {
            who: Who::Mine,
            computer: None,
        }),
        MadeBy::Door43,
        MadeBy::ExchangeFile,
        MadeBy::Import,
    ]
}

fn review_entries() -> Vec<ReviewEntries> {
    vec![
        ReviewEntries::Conflicts(Vec::new()),
        ReviewEntries::Detached(vec![DetachedEntry {
            record: record(r#"{"kind":"check"}"#),
            what: DetachedKind::Decision,
            place: review_places()[3].clone(),
        }]),
        ReviewEntries::Notices(vec![NoticeEntry {
            notice: NoticeView::WriteBlocked {
                file: "ingredients/TIT.usfm".into(),
            },
        }]),
    ]
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

impl Example for export::Book {
    fn example() -> Self {
        export::Book {
            at: scope(),
            book: BookId::Titus,
        }
    }
}

impl Example for export::BookPrint {
    fn example() -> Self {
        let verse = |label: &str, text: Option<&str>| export::PrintVerse {
            label: RefPart::parse(label).unwrap(),
            text: text.map(str::to_owned),
        };
        export::BookPrint {
            items: vec![
                export::PrintItem::Chapter(export::PrintChapter {
                    chapter: 1,
                    verses: vec![
                        verse("1", Some("Pablo, siervo de Dios")),
                        verse("2-3", None),
                    ],
                }),
                export::PrintItem::Gap(export::Run { from: 2, to: 2 }),
                export::PrintItem::Chapter(export::PrintChapter {
                    chapter: 3,
                    verses: vec![verse("1", Some("Recuérdales"))],
                }),
            ],
            undrafted: true,
        }
    }
}

impl Example for export::Stories {
    fn example() -> Self {
        export::Stories { at: scope() }
    }
}

impl Example for export::StoriesPrint {
    fn example() -> Self {
        let story = |n: u8| StoryNo::new(n).unwrap();
        export::StoriesPrint {
            items: vec![
                export::PrintStoryItem::Story(export::PrintStory {
                    story: story(1),
                    title: "La Creación".into(),
                    frames: vec![
                        export::PrintFrame::Drafted(export::DraftedFrame {
                            frame: 1,
                            text: "Así fue como Dios hizo todo.".into(),
                            picture: Some(
                                "/media/obs/git.door43.org~uW~obs_images_360~7146d5b504f6b63b9e11f7dc0b18c594d0ae179d/360px/obs-en-01-01.jpg"
                                    .into(),
                            ),
                        }),
                        export::PrintFrame::Gap(export::Run { from: 2, to: 16 }),
                    ],
                    reference: Some("Una historia bíblica de: Génesis 1-2".into()),
                }),
                export::PrintStoryItem::Gap(export::StoryRun {
                    from: story(2),
                    to: story(3),
                }),
                export::PrintStoryItem::Story(export::PrintStory {
                    story: story(4),
                    title: String::new(),
                    frames: vec![export::PrintFrame::Drafted(export::DraftedFrame {
                        frame: 1,
                        text: "Dios dijo".into(),
                        picture: None,
                    })],
                    reference: None,
                }),
            ],
            undrafted: true,
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
            "align/close",
            "align/edit",
            "align/suggest",
            "align/flush",
            "align/keepReflow",
            "align/open",
            "check/decide",
            "conflicts/resolve",
            "prefs/recordPlace",
            "prefs/setAlignSuggestions",
            "prefs/setDraftUnit",
            "resources/applyPlan",
            "resources/download",
            "resources/fixFetch",
            "resources/fixRepin",
            "resources/fixSideload",
            "resources/planGateway",
            "resources/planUpgrade",
            "session/close",
            "session/copy",
            "session/discardDraft",
            "session/drafts",
            "session/keepDraft",
            "session/otherComputerClosed",
            "editor/saveSection",
            "editor/addComment",
            "story/saveFrame",
            "story/saveRef",
            "editor/saveVerse",
            "editor/saveLayout",
            "editor/draft",
            "editor/discardDraft",
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
        ApiError::ToolNotReady,
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
        NotSaved::Refused(SaveRefusal::InvalidText {
            reason: "a comment needs text".into(),
        }),
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
            kind: JobKind::Open,
            project: Some(ProjectRef::parse("sample_burrito").unwrap()),
            state: JobState::Done(JobOutcome::Opened(Box::new(opened()))),
        }),
        Event::Job(JobView {
            id: JobId::new(11),
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
        Event::Job(JobView {
            id: JobId::new(12),
            kind: JobKind::GatewayPlan,
            project: Some(ProjectRef::parse("sample_burrito").unwrap()),
            state: JobState::Done(JobOutcome::PlanReady(Box::new(gateway_plan()))),
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
                        unit: CheckUnit::Story(StoryNo::new(12).unwrap()),
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
        SessionEvent::Suggest {
            testament: Testament::Nt,
            status: SuggestStatus::Ready {
                verses: 12,
                boosted: true,
            },
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
            BufferLine::Removed { key: verse.clone(), instance: ServerInstance::from_bytes([0xab; 16]), text: None },
            BufferLine::Removed { key: verse, instance: ServerInstance::from_bytes([0xab; 16]), text: Some("Paul, a servant".into()) },
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
        ("types/align-edits.json", wire_samples(&align_edits())),
        ("types/align-blocked.json", wire_samples(&align_blocked())),
        ("types/align-opened.json", wire_samples(&align_opened())),
        (
            "types/suggest-statuses.json",
            wire_samples(&suggest_statuses()),
        ),
        ("types/check-places.json", plain_samples(&check_places())),
        (
            "types/review.json",
            json!({
                "notices": wire_samples(&review_notices()),
                "places": wire_samples(&review_places()),
                "values": wire_samples(&review_values()),
                "makers": wire_samples(&review_makers()),
                "entries": wire_samples(&review_entries()),
            }),
        ),
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
        "align-edits",
        "align-blocked",
        "align-opened",
        "suggest-statuses",
        "check-places",
        "check-units",
        "check-lists",
        "buffer",
        "review",
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
