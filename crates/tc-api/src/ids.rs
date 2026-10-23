//! Identifiers on the wire (APP-DESIGN §3.2). Each one checks its own format when it is
//! deserialized, so a malformed id is refused at the boundary and code past it can trust it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a text is not a valid identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{what}: {reason}")]
pub struct IdError {
    pub what: &'static str,
    pub reason: &'static str,
}

fn refuse<T>(what: &'static str, reason: &'static str) -> Result<T, IdError> {
    Err(IdError { what, reason })
}

fn is_lower_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex16(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_hex16(what: &'static str, text: &str) -> Result<[u8; 16], IdError> {
    if !is_lower_hex(text, 32) {
        return refuse(what, "expected 32 lowercase hex digits");
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        let pair = text.get(2 * i..2 * i + 2).unwrap_or("");
        *byte = u8::from_str_radix(pair, 16).map_err(|_| IdError {
            what,
            reason: "expected 32 lowercase hex digits",
        })?;
    }
    Ok(out)
}

/// A project folder: its name under the projects root, one path segment (AD12).
///
/// The registry lists folders and the server joins this name to the projects root, so the
/// check here keeps a name from leaving the root on every system the app runs on. It
/// refuses an empty name, any name holding `/`, `\`, NUL or `:`, and any name made only of
/// dots and spaces. On Windows, joining a name with a drive prefix (`C:`, `c:..`) replaces
/// the whole path, and `name:stream` names an NTFS stream, so no Windows folder name holds a
/// colon; and Windows drops trailing dots and spaces from the last segment of a path, so
/// `...` or `.. ` would name the root or its parent. Every other name is kept, so no
/// project folder becomes unreachable from the UI; the registry reports a folder whose
/// name fails this check instead of skipping it.
///
/// Two names can still name one folder where the file system ignores letter case or a
/// trailing dot, so the registry opens only a folder it lists, by the name it listed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectRef(String);

impl ProjectRef {
    /// Longer than any folder name Windows (255 UTF-16 units) or Linux (255 bytes) allows.
    pub const MAX_BYTES: usize = 1024;

    pub fn parse(name: &str) -> Result<Self, IdError> {
        const WHAT: &str = "project folder";
        if name.is_empty() {
            return refuse(WHAT, "empty");
        }
        if name.chars().all(|c| c == '.' || c == ' ') {
            return refuse(WHAT, "not a folder name");
        }
        if name.len() > Self::MAX_BYTES {
            return refuse(WHAT, "too long");
        }
        if name.contains(['/', '\\', '\0', ':']) {
            return refuse(WHAT, "not one path segment on every system");
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProjectRef {
    type Error = IdError;
    fn try_from(name: String) -> Result<Self, IdError> {
        Self::parse(&name)
    }
}

impl From<ProjectRef> for String {
    fn from(id: ProjectRef) -> Self {
        id.0
    }
}

impl fmt::Display for ProjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `projectId` of `ingredients/tc/project.json`: a UUID in lowercase hyphenated form. Data
/// about a project, never the key of one (two folders may share it, AD12).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectId(String);

impl ProjectId {
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let groups: Vec<&str> = text.split('-').collect();
        let lens = [8, 4, 4, 4, 12];
        let ok = groups.len() == lens.len()
            && groups
                .iter()
                .zip(lens)
                .all(|(group, len)| is_lower_hex(group, len));
        if !ok {
            return refuse("project id", "expected a lowercase hyphenated UUID");
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProjectId {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        Self::parse(&text)
    }
}

impl From<ProjectId> for String {
    fn from(id: ProjectId) -> Self {
        id.0
    }
}

macro_rules! counter_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(n: u64) -> Self {
                Self(n)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

counter_id!(
    /// One open of one folder in this server run. Never reused, so a reply or a write for
    /// an earlier open is recognized as stale.
    Epoch
);
counter_id!(
    /// A session's view of its project: one more for every fold delta the session applies.
    /// A reply or event with an older `ViewSeq` than the one held is older data.
    ViewSeq
);
counter_id!(JobId);
counter_id!(PlanId);
counter_id!(ProposalId);
counter_id!(CheckSessionId);
counter_id!(AlignerId);
counter_id!(
    /// One operation report in the server's report log; "Ask for help" attaches it.
    ReportId
);

/// Where a project-scoped request applies: the folder and the open it was made against.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub project: ProjectRef,
    pub epoch: Epoch,
}

macro_rules! bytes_id {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
        #[serde(try_from = "String", into = "String")]
        pub struct $name([u8; 16]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(bytes)
            }

            pub const fn bytes(&self) -> [u8; 16] {
                self.0
            }

            pub fn parse(text: &str) -> Result<Self, IdError> {
                parse_hex16($what, text).map(Self)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(text: String) -> Result<Self, IdError> {
                Self::parse(&text)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                hex16(&id.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&hex16(&self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({self})", stringify!($name))
            }
        }
    };
}

bytes_id!(
    /// Made by the UI, one per save attempt of one text, and kept with the text that was
    /// sent, so a retry after a lost reply is recognized (AD4). On the wire: 32 hex digits.
    RequestId,
    "request id"
);
bytes_id!(
    /// Random for each server launch. A different one means the server restarted, its
    /// sessions and reply cache are gone, and the UI goes Home (§3.10).
    ServerInstance,
    "server instance"
);

macro_rules! hex_id {
    ($(#[$doc:meta])* $name:ident, $len:literal, $what:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub const LEN: usize = $len;

            pub fn parse(text: &str) -> Result<Self, IdError> {
                if !is_lower_hex(text, $len) {
                    return refuse(
                        $what,
                        concat!("expected ", stringify!($len), " lowercase hex digits"),
                    );
                }
                Ok(Self(text.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(text: String) -> Result<Self, IdError> {
                Self::parse(&text)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        #[cfg(feature = "proptest")]
        impl proptest::arbitrary::Arbitrary for $name {
            type Parameters = ();
            type Strategy = proptest::strategy::BoxedStrategy<Self>;
            fn arbitrary_with((): ()) -> Self::Strategy {
                use proptest::strategy::Strategy;
                // The pattern is a literal that compiles.
                proptest::string::string_regex(concat!("[0-9a-f]{", stringify!($len), "}"))
                    .unwrap_or_else(|_| unreachable!("a valid regex"))
                    .prop_map(Self)
                    .boxed()
            }
        }
    };
}

hex_id!(
    /// A file the UI uploaded into the server's `uploads/` folder (§5.5).
    UploadId,
    32,
    "upload id"
);
hex_id!(
    /// A finished export the UI can download once (§5.5).
    DownloadId,
    32,
    "download id"
);
hex_id!(
    /// tc-journal's `ConflictId`: SHA-256 over the conflict's heads.
    ConflictRef,
    64,
    "conflict id"
);
hex_id!(
    /// tc-journal's `ChangeId`.
    ChangeRef,
    64,
    "change id"
);

/// A record of the project, as the canonical JSON of tc-journal's `RecordKey`. The UI
/// never reads inside it: it holds it to match `Ack` and `Changed` events to fields.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordRef(String);

impl RecordRef {
    /// A record key is a handful of short members; this bounds what a request can carry.
    pub const MAX_BYTES: usize = 4096;

    pub fn parse(text: &str) -> Result<Self, IdError> {
        const WHAT: &str = "record";
        if text.len() > Self::MAX_BYTES {
            return refuse(WHAT, "too long");
        }
        if !(text.starts_with('{') && text.ends_with('}')) {
            return refuse(WHAT, "not a JSON object");
        }
        // tc-journal's canonical JSON escapes every character below U+0020 and writes every
        // other one raw, U+007F to U+009F included (`tc-journal/src/json.rs`), so only these
        // can never appear.
        if text.chars().any(|c| c < ' ') {
            return refuse(WHAT, "not canonical JSON");
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RecordRef {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        Self::parse(&text)
    }
}

impl From<RecordRef> for String {
    fn from(id: RecordRef) -> Self {
        id.0
    }
}

/// A chapter or a verse part of a reference as tC4 writes it: `1`, `9-10`, `front`,
/// `intro`, `3,5`, `1a`. It reads every part tc-check's `RefPart` holds, and a little more.
/// tc-check's part is the journal's identity grammar: NFC text that is not empty and holds
/// neither `:`, the separator between the parts, nor `|`. This one takes any text that is not
/// empty and holds no `:`, so a check the server sends always reads, and the server refuses
/// at its boundary a part that tc-check cannot hold.
///
/// It has no length limit. A check whose place the UI cannot read would drop out of the rail
/// and show its decision as not saved, and tc-check accepts any length, so this type must
/// too. A request is bounded where the server reads its body, not field by field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RefPart(String);

impl RefPart {
    pub fn parse(text: &str) -> Result<Self, IdError> {
        const WHAT: &str = "reference part";
        if text.is_empty() {
            return refuse(WHAT, "empty");
        }
        if text.contains(':') {
            return refuse(WHAT, "holds a part separator");
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RefPart {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        Self::parse(&text)
    }
}

impl From<RefPart> for String {
    fn from(part: RefPart) -> Self {
        part.0
    }
}

impl fmt::Display for RefPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An Open Bible Stories story, 1 to 50 (tc-helps `ObsRef`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct StoryNo(u8);

impl StoryNo {
    pub const COUNT: u8 = 50;

    pub fn new(number: u8) -> Result<Self, IdError> {
        if (1..=Self::COUNT).contains(&number) {
            Ok(Self(number))
        } else {
            refuse("story", "not a story from 1 to 50")
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for StoryNo {
    type Error = IdError;
    fn try_from(number: u8) -> Result<Self, IdError> {
        Self::new(number)
    }
}

impl From<StoryNo> for u8 {
    fn from(story: StoryNo) -> Self {
        story.0
    }
}

/// The interface languages, in the picker's order: the catalogs tc-ui ships
/// (`crates/tc-ui/src/i18n.rs` `LOCALES`).
pub const LOCALE_IDS: [&str; 10] = [
    "en", "es-419", "fr", "hi", "pt-BR", "id", "uk", "ru", "vi", "ar",
];

/// One of the ten interface languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocaleId(&'static str);

impl Serialize for LocaleId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for LocaleId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let id = String::deserialize(d)?;
        Self::parse(&id).map_err(serde::de::Error::custom)
    }
}

impl LocaleId {
    pub const ENGLISH: LocaleId = LocaleId("en");

    pub fn parse(id: &str) -> Result<Self, IdError> {
        LOCALE_IDS
            .iter()
            .find(|known| **known == id)
            .map(|known| Self(known))
            .ok_or(IdError {
                what: "locale",
                reason: "not one of the interface languages",
            })
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }

    pub fn all() -> impl Iterator<Item = LocaleId> {
        LOCALE_IDS.iter().map(|id| LocaleId(id))
    }
}

/// Milliseconds since the Unix epoch, as the server's clock read them. tc-api reads no
/// clock itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(transparent)]
pub struct WallMs(pub i64);

#[cfg(feature = "proptest")]
mod arbitrary {
    use proptest::prelude::*;

    use super::*;

    /// Every caller passes a literal pattern that compiles.
    fn regex(pattern: &str) -> BoxedStrategy<String> {
        proptest::string::string_regex(pattern)
            .unwrap_or_else(|_| unreachable!("a valid regex"))
            .boxed()
    }

    impl Arbitrary for ProjectRef {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            // Any characters but the separators, including spaces, dots and non-Latin
            // letters that real folder names have.
            regex("[^/\\\\\u{0}:]{1,40}")
                .prop_filter_map("not only dots and spaces", |name| {
                    ProjectRef::parse(&name).ok()
                })
                .boxed()
        }
    }

    impl Arbitrary for ProjectId {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            regex("[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                .prop_map(ProjectId)
                .boxed()
        }
    }

    impl Arbitrary for RecordRef {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            regex("\\{\"kind\":\"[a-z]{1,10}\",\"book\":\"[A-Z1-3]{3}\",\"v\":[0-9]{1,3}\\}")
                .prop_map(RecordRef)
                .boxed()
        }
    }

    impl Arbitrary for RefPart {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            let forms = prop_oneof![
                (1u16..=176, 0u16..3).prop_map(|(v, extra)| if extra == 0 {
                    v.to_string()
                } else {
                    format!("{v}-{}", v + extra)
                }),
                proptest::sample::select(&["intro", "front", "3,5", "1a"][..])
                    .prop_map(str::to_owned),
                regex("[^:]{1,20}"),
            ];
            forms.prop_map(RefPart).boxed()
        }
    }

    impl Arbitrary for StoryNo {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            (1..=StoryNo::COUNT).prop_map(StoryNo).boxed()
        }
    }

    impl Arbitrary for LocaleId {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            proptest::sample::select(&LOCALE_IDS[..])
                .prop_map(LocaleId)
                .boxed()
        }
    }
}
