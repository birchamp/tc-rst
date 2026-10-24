//! The draft buffer's persisted format (APP-DESIGN §6.6, AD21): one browser storage item per
//! field, and one line of the shell's `drafts-buffer.log` per change.
//!
//! The UI writes these, `boot.js` updates them on every keystroke before the WebAssembly sees
//! it, the shell appends each change to its log and hands the lines back at the next launch,
//! and the server reads the lines that `session/open { buffered }` carries (C1b). One format,
//! here, so the four cannot drift.
//!
//! - A browser item's key is [`BUFFER_ITEM_PREFIX`] followed by its [`BufferKey`] as JSON,
//!   exactly as serde writes it. A folder name may hold any character but `/ \ : NUL`, so a
//!   key built by joining texts with a separator could name two fields; JSON cannot.
//! - An entry ([`BufferEntry`]) keeps the keys it does not know when a reader rewrites it
//!   (§3.10), so an older build never drops what a newer one stored. A request refuses unknown
//!   keys at any depth, so a request that carries entries carries them as raw lines.
//! - A log line ([`BufferLine`]) is one JSON object with no line feed in it (JSON escapes
//!   them), so a crash in the middle of an append can tear only the last line, which a reader
//!   skips.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{IdError, ProjectId, ProjectRef, RequestId, Scope, ServerInstance, WallMs};
use crate::token::ShownToken;

/// The `localStorage` key of a field is this prefix followed by its [`BufferKey`] as JSON.
pub const BUFFER_ITEM_PREFIX: &str = "tc4.drafts.v2:";

/// A field with typed text, by folder, project id and the field itself. The project id is part
/// of the key so that an entry of a folder that was deleted and made again never meets the
/// new project's records (A-9).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BufferKey {
    pub project: ProjectRef,
    pub id: ProjectId,
    pub field: FieldKey,
}

/// One editable field of a project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldKey {
    pub kind: FieldKind,
    pub place: FieldPlace,
}

/// The kinds of field the save machine knows, each with tC4's own timing (§6.5, AD20). The
/// list is closed: a build that does not know a kind skips its entries and leaves them alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    /// A verse of the translation.
    Verse,
    /// A chapter's or book's front matter, headings and other layout text.
    Layout,
    /// The section editor's buffer: saved only by its Save button (A-1).
    Section,
    /// A user comment, saved on blur (A-18).
    Comment,
    /// A check's comment, saved on Done and on leaving the item (A-10).
    CheckComment,
    /// An OBS frame.
    Frame,
    /// An OBS story's title.
    StoryTitle,
    /// An OBS story's reference line.
    StoryRef,
    /// A field of the test service (`test/fieldStore`). The variant is in every build: cargo
    /// unifies tc-api's features across the workspace, so a crate built without its own test
    /// hooks may still see it and must match it. Only a test build reads it off the wire.
    #[cfg_attr(not(feature = "test-hooks"), serde(skip))]
    Test,
}

/// Where a field is, in the form its kind's unit defines (`TIT 1:4` for a verse): 1 to 256
/// bytes with no control character.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FieldPlace(String);

impl FieldPlace {
    pub const MAX_BYTES: usize = 256;

    pub fn parse(text: &str) -> Result<Self, IdError> {
        const WHAT: &str = "field place";
        if text.is_empty() {
            return Err(IdError {
                what: WHAT,
                reason: "empty",
            });
        }
        if text.len() > Self::MAX_BYTES {
            return Err(IdError {
                what: WHAT,
                reason: "longer than 256 bytes",
            });
        }
        if text.chars().any(char::is_control) {
            return Err(IdError {
                what: WHAT,
                reason: "holds a control character",
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FieldPlace {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        Self::parse(&text)
    }
}

impl From<FieldPlace> for String {
    fn from(place: FieldPlace) -> Self {
        place.0
    }
}

impl fmt::Debug for FieldPlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldPlace({:?})", self.0)
    }
}

impl fmt::Display for FieldPlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A field's typed text as the buffer keeps it.
///
/// No `deny_unknown_fields`: a newer build may add keys, and a rewrite keeps them (§3.10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BufferEntry {
    /// The [`API_VERSION`](crate::API_VERSION) of the build that wrote it.
    pub api: u32,
    /// The server run it was written under. An entry is restored into the save machine, and
    /// its `sent` retried, only under the same run: a run that started again has no reply
    /// cache, so a resend would store the text a second time over its old token. Every other
    /// entry waits for the offer at its project's next open (AD4).
    pub instance: ServerInstance,
    /// The tokens the field's next save names: the version the person was editing.
    pub base: Vec<ShownToken>,
    /// The field's text now.
    pub text: String,
    /// The save in flight, if any: its id and its whole body, so a retry after a lost reply
    /// sends the same request and never the current text under the old id (A-2).
    pub sent: Option<SentSave>,
    /// The last change, `Date.now()` in the browser.
    pub at: WallMs,
    /// How `boot.js` posts this entry as a `Draft` with a plain `fetch` after three panics,
    /// when the WebAssembly can no longer run (§6.8). `None` for a field with no server
    /// draft.
    pub draft: Option<DraftCall>,
}

/// A save on its way: the request id and what it carried besides the tokens of `base`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SentSave {
    pub request: RequestId,
    pub text: String,
    /// The open it was sent to; `None` for a field of the app (a test field). A retry sends
    /// the same scope, because the reply cache compares the whole body.
    pub scope: Option<Scope>,
}

/// A draft request with its variable parts left out: `boot.js` posts `body` to
/// `/api/v1/<path>` with a fresh `request` id and the entry's `text` added. Every draft
/// endpoint's request has a top-level `request` and `text` for that reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftCall {
    /// The endpoint, `service/name`.
    pub path: String,
    /// A JSON object as text, without `request` and `text`. Text, so that this crate needs
    /// no JSON library in the UI's build.
    pub body: String,
}

/// One line of the shell's buffer log: a field's entry as it is now, or its removal once the
/// field is clean.
///
/// The shell keeps the last line per field and server run, and a removal drops only its own
/// run's entry: after a restart behind a reused port, one field can hold a text of each run, and
/// saving the new run's text must not drop the old run's, which is offered at the next open
/// (§5.7, §6.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BufferLine {
    Changed {
        key: BufferKey,
        entry: BufferEntry,
    },
    /// The field became clean in the run `instance`; another run's entry of it stays. With
    /// `text`, the entry goes only while it holds that text (NFC): an offer's slot settled,
    /// whose run may have written newer text since (§6.6).
    Removed {
        key: BufferKey,
        instance: ServerInstance,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
}

impl BufferLine {
    pub fn key(&self) -> &BufferKey {
        match self {
            Self::Changed { key, .. } | Self::Removed { key, .. } => key,
        }
    }

    /// The server run the line belongs to.
    pub fn instance(&self) -> ServerInstance {
        match self {
            Self::Changed { entry, .. } => entry.instance,
            Self::Removed { instance, .. } => *instance,
        }
    }
}

#[cfg(feature = "proptest")]
mod arbitrary {
    use proptest::prelude::*;

    use super::FieldPlace;

    impl Arbitrary for FieldPlace {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            // Any text a unit could use, `|`, quotes and non-Latin letters included.
            proptest::string::string_regex("[^\\x00-\\x1f\\x7f]{1,40}")
                .unwrap_or_else(|_| unreachable!("a valid regex"))
                .prop_filter_map("a valid place", |text| FieldPlace::parse(&text).ok())
                .boxed()
        }
    }
}
