//! The `test` service: hooks that exist only in test builds of the server (feature
//! `test-hooks`, APP-DESIGN §5.8, AD35). A release build has none of them.

use serde::{Deserialize, Serialize};

use crate::ids::RequestId;
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
/// released, the armed crash disarmed, and the internet switch off as a launch finds it; the
/// parity harness's `home()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reset {
    pub request: RequestId,
}
