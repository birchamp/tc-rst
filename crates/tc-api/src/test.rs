//! The `test` service: hooks that exist only in test builds of the server (feature
//! `test-hooks`, APP-DESIGN §5.8, AD35). A release build has none of them.

use serde::{Deserialize, Serialize};

use crate::home::ResumePlace;
use crate::ids::{ProjectRef, RequestId, WallMs};
use crate::prefs::PlaceMode;
use crate::shown::Shown;
use crate::token::ShownToken;

/// `test/quiet`: whether anything is still in flight, so a capture can settle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Quiet {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct QuietView {
    /// A request, a job, an actor queue or a pending materialize is live.
    pub busy: bool,
}

/// `test/validate`: every project folder in the projects folder, checked as a Scripture
/// Burrito once the open projects have settled (P5, INV-VALID-BURRITO), so a journey ends
/// knowing the folders it left are valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Validate {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ValidateView {
    /// How many folders were checked: 0 when the projects are not on disk.
    pub checked: u32,
    pub invalid: Vec<InvalidProject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct InvalidProject {
    /// The folder's name in the projects folder.
    pub folder: String,
    pub problems: Vec<String>,
}

/// `test/echo`: a reply of about the size asked for, so the loopback round trip of the
/// aligner's click can be measured with a reply of its size (APP-DESIGN §8.3, unit A2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Echo {
    /// The reply's length in bytes, at least; at most [`MAX_ECHO`].
    pub size: u32,
}

/// The largest `size` an echo answers; a larger one is a `BadRequest`.
pub const MAX_ECHO: u32 = 4 * 1024 * 1024;

/// Many small objects of Hebrew text, the shape of a word bank, so that the cost of reading
/// the reply is the cost of reading the aligner's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct EchoView {
    pub words: Vec<EchoWord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct EchoWord {
    pub text: String,
    pub occurrence: u16,
    pub occurrences: u16,
    pub index: u32,
}

/// Genesis 1:1 in the Hebrew Bible, pointed, a word at a time.
const ECHO_TEXT: [&str; 7] = ["בְּרֵאשִׁ֖ית", "בָּרָ֣א", "אֱלֹהִ֑ים", "אֵ֥ת", "הַשָּׁמַ֖יִם", "וְאֵ֥ת", "הָאָֽרֶץ"];

impl EchoView {
    /// The shortest view of the cycle of words whose reply, `{"type":"ok","value":{…}}` as
    /// the server writes it, is at least `size` bytes: between `size` and `size` plus one
    /// word, or the empty view for a `size` it already reaches. The same size gives the
    /// same view.
    pub fn of_size(size: u32) -> EchoView {
        let wanted = u64::from(size);
        let mut words = Vec::new();
        let mut length = Self::reply_length_without_words();
        while length < wanted {
            let index = u32::try_from(words.len()).unwrap_or(u32::MAX);
            let cycle = words.len() / ECHO_TEXT.len();
            let word = EchoWord {
                text: ECHO_TEXT[words.len() % ECHO_TEXT.len()].to_owned(),
                occurrence: u16::try_from(cycle % 4 + 1).unwrap_or(1),
                occurrences: 4,
                index,
            };
            // A comma before every word but the first.
            length += Self::word_length(&word) + u64::from(!words.is_empty());
            words.push(word);
        }
        EchoView { words }
    }

    fn reply_length_without_words() -> u64 {
        let empty = crate::Reply::Ok(EchoView { words: Vec::new() });
        serde_json::to_vec(&empty).map_or(0, |bytes| bytes.len() as u64)
    }

    fn word_length(word: &EchoWord) -> u64 {
        serde_json::to_vec(word).map_or(0, |bytes| bytes.len() as u64)
    }
}

/// `test/crash`: ends the server at the next time a write reaches `at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Crash {
    pub request: RequestId,
    pub at: CrashPoint,
    /// Only a write to this endpoint path; any write when `None`.
    pub path: Option<String>,
}

/// Where a write can be cut off (§6.6, A-2, A-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum CrashPoint {
    /// Before the change is durable (before STORAGE's B3): nothing was stored.
    BeforeStore,
    /// After the change is durable, before the reply is sent: the reply is lost.
    BeforeReply,
    /// After the reply is sent.
    AfterReply,
    /// While a draft is written.
    DuringDraftWrite,
}

/// `test/hold`: the next operation that reaches `at` waits until `test/release`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hold {
    pub request: RequestId,
    pub at: HoldPoint,
}

/// `test/release`: lets the held operation go on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub request: RequestId,
    pub at: HoldPoint,
}

/// Where an operation can be held (§5.8; R8 adds the sync steps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum HoldPoint {
    /// A save, before it is durable.
    Save,
    /// A project's open for the screens, before the store opens it (B3).
    Open,
    /// Suggestion training, after capturing its snapshot and before reading it.
    Train,
    BookRead,
    Checkpoint,
    Materialize,
    DraftWrite,
}

/// `test/fieldStore`: a write that goes through the real reply cache and records every
/// text it stores, so the UI's save machine can be proved before any real save exists
/// (A-2). Its reply is a real `Ack`; `test/crash` and `test/hold` apply to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldStore {
    pub request: RequestId,
    /// A name the test chooses for the field.
    pub field: String,
    pub shown: ShownToken,
    pub text: String,
}

/// `test/fieldLog`: a test field's current value with its token, and every text stored
/// into it, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldLog {
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FieldLogView {
    pub current: Shown<String>,
    pub stores: Vec<StoredText>,
    /// The newest draft `test/fieldDraft` kept for the field, if any.
    pub draft: Option<StoredDraft>,
    /// How many drafts the field was sent, so a test can count them.
    pub drafts: u32,
}

/// `test/fieldDraft`: the draft of a test field, as `editor/draft` will keep a verse's in
/// `.tc/drafts.json` (STORAGE §5.14). It keeps the newest text per field and changes neither
/// the field's value nor its stores, so a test can show that the drafts `boot.js` posts after
/// three panics reach the server (§6.8). Its token is checked as `test/fieldStore` checks one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldDraft {
    pub request: RequestId,
    pub field: String,
    pub shown: ShownToken,
    pub text: String,
}

/// A draft as `test/fieldLog` shows it: its request, its text in NFC, and which version of
/// the field its token named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoredDraft {
    pub request: RequestId,
    pub text: String,
    pub base: TokenBase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoredText {
    pub request: RequestId,
    pub text: String,
    /// Which version of the field the request's token named.
    pub base: TokenBase,
}

/// What the token of a stored text named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum TokenBase {
    /// The field's current version.
    Current,
    /// An earlier version. On a real project the new text would stand beside the newer
    /// version as a conflict (STORAGE:1449-1453); in a test of the save machine it is a
    /// conflict with the person's own earlier save, which AD4 exists to prevent.
    Stale,
}

/// `test/reset`: prefs, summaries and reports back to the scenario's seed, every hold
/// released, the armed crash disarmed, the internet switch off as a launch finds it, and the
/// account as a launch finds it with the test keychain working and empty; the parity
/// harness's `home()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reset {
    pub request: RequestId,
}

/// `test/lastEdit`: records a Resume record and the project's last use through the calls a
/// save makes (C2), then tells Home the projects changed, so the Resume card can be
/// photographed before any save exists (B2). It applies to no open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LastEdit {
    pub request: RequestId,
    pub project: ProjectRef,
    pub place: ResumePlace,
    pub mode: PlaceMode,
    /// Kept as given, spaces included.
    pub snippet: String,
    pub at: WallMs,
}

/// `test/keychain`: the test keychain's behaviour and what it holds, then, with `relaunch`,
/// the account as a launch finds it: every sign-in in flight cancelled, the session and the
/// notice gone, the keychain read once (A8). A journey uses it where tC4's use a fake
/// keychain and a reload. The token is never in the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keychain {
    pub request: RequestId,
    /// How the keychain behaves from now on; `None` keeps its mode.
    pub mode: Option<KeychainMode>,
    /// What it holds from now on; `None` keeps what it holds.
    pub holds: Option<KeychainHolds>,
    /// Read the account again as a launch does.
    pub relaunch: bool,
}

/// How the test keychain behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum KeychainMode {
    Working,
    /// This computer has no keychain: nothing is kept, a read answers that there is none.
    None,
    /// Keeping fails; reading and forgetting work.
    FailingKeep,
    /// Forgetting fails; keeping and reading work.
    FailingForget,
    /// The kept bytes cannot be opened: the next read removes them and fails.
    Unreadable,
}

/// What the test keychain holds.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum KeychainHolds {
    Nothing,
    /// A Door43 token, as an earlier session kept it.
    Token(String),
}

/// A token never reaches a log line, not even a test's.
impl std::fmt::Debug for KeychainHolds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nothing => f.write_str("Nothing"),
            Self::Token(_) => f.write_str("Token(..)"),
        }
    }
}

/// What the test keychain is and has done. The token itself never comes back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct KeychainView {
    pub mode: KeychainMode,
    /// Whether it holds a token.
    pub holds: bool,
    /// The calls it was asked, since the last `test/reset`.
    pub keeps: u32,
    pub reads: u32,
    pub forgets: u32,
}

/// `test/script`: how the next job of a kind ends, until units X and E1b bring the real Door43
/// open and sync (AD6, B10). A job with nothing scripted answers `Unavailable(Sync)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Script {
    pub request: RequestId,
    pub next: Scripted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Scripted {
    /// A `door43/open` fails so, as a clone from Door43 fails (tC4 refuses an open with the
    /// sync's codes). Scripted opens wait in order.
    OpenDoor43Fails(crate::refusal::SyncFailure),
    /// The next `door43/sync` of `project` ends so, whatever was scripted for it before: a
    /// test scripts what Door43 holds now.
    Sync {
        project: ProjectRef,
        outcome: ScriptedSync,
    },
    /// The next `resources/checkUpdates` answers this view without asking Door43: the offers
    /// tC4's gallery draws, whose releases no server holds (C8's parity captures).
    Updates(crate::resources::UpdatesView),
    /// The next plan (`resources/planUpgrade`, `planGateway` or `fixRepin`) is this view, as made,
    /// with no download and no reading of the project: a dialog tC4's gallery draws with the
    /// carry-over counts of another change. The server keeps no plan under it, so confirming it
    /// answers `PlanStale`, which is what tC4's gallery shows for a failed upgrade.
    Plan(Box<crate::resources::PlanView>),
}

/// How a scripted sync ends: what Door43 held, as a person's sync would find it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ScriptedSync {
    /// The sync completes. `teammates` changed one verse on Door43, each text by a teammate of
    /// its own, all from the verse's current version: the sync receives their changes.
    /// `restored` is how many changes Door43 had lost and the sync sends again.
    Synced {
        teammates: Option<TeammatesEdit>,
        /// Teammate 1 joined a run of verses into one on Door43, in one batch, from the
        /// verses as Door43 held them: the sync receives the join (C7a's passage conflict
        /// when this computer changed one of the verses meanwhile).
        #[serde(default)]
        joined: Option<TeammatesJoin>,
        restored: u32,
    },
    Failed(crate::refusal::SyncFailure),
}

/// Teammate 1's join of the verses `first` to `last` of one chapter into one span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TeammatesJoin {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: tc_core::BookId,
    pub chapter: u16,
    pub first: u16,
    pub last: u16,
}

/// Teammates' versions of one verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TeammatesEdit {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: tc_core::BookId,
    pub chapter: u16,
    pub verse: u16,
    pub texts: Vec<String>,
}
