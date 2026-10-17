//! Record keys (`docs/STORAGE.md` §2.2). A key names one record; two changes conflict only
//! if they have the same key. The identity strings inside keys are checked when they are
//! made (rule V2): normalizing an identity could merge two records, so a string that breaks
//! its grammar is refused, never repaired.

use crate::ids::{BatchId, ChapterNo, CommentId, DeviceId, FrameNo, RepoPath, StoryNo, VerseNo};
use std::fmt;
use tc_core::BookId;
use unicode_normalization::is_nfc;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKey {
    // ---- Bible text ----
    /// In scope with a passage-set scope, or removed.
    BookPresence(BookId),
    /// Every byte before the first `\c` marker.
    BookHeader(BookId),
    /// `\c N` through the byte before the chapter's first `\v`.
    ChapterFront(BookId, ChapterNo),
    /// One verse number of the versification frame.
    Verse(BookId, ChapterNo, VerseNo),
    /// Only for imported books whose order is not numeric.
    BookLayout(BookId),
    // ---- checking ----
    /// Keyed by the anchor of the first verse of the span it aligns (§7.7).
    Alignment(BookId, ChapterNo, VerseNo),
    /// One tN / tW / tQ decision.
    Check(CheckKey),
    // ---- comments (grow-only) ----
    Comment(CommentId),
    // ---- project ----
    Pins(PinGroup),
    /// A key of `resources.json` that no pin group holds (§3.7 of the record list): kept so
    /// an import or an outside edit never loses it.
    ResourcesExtra(ResourcesKey),
    /// One top-level key of settings.json.
    Setting(SettingKey),
    ProjectMeta(MetaField),
    Versification,
    /// Written only by that device (rule V4).
    DeviceProfile(DeviceId),
    // ---- Open Bible Stories ----
    /// Template bytes of content/NN.md.
    StoryBase(StoryNo),
    /// A frame paragraph; frame 0 is the title.
    Frame(StoryNo, FrameNo),
    /// The closing `_…_` reference line.
    StoryRef(StoryNo),
    // ---- files the app does not derive ----
    CarriedFile(RepoPath),
    // ---- a person's decision about an incomplete batch (§7.4); grow-only ----
    BatchRelease(BatchId),
    // ---- a kind from a newer minor format (§12) ----
    Unknown(UnknownKey),
}

impl RecordKey {
    /// The book the record belongs to, for the records §7.6 counts as a book's records
    /// (checks and comments are placed by their target, which needs the fold).
    pub fn book(&self) -> Option<BookId> {
        match self {
            Self::BookPresence(b)
            | Self::BookHeader(b)
            | Self::ChapterFront(b, _)
            | Self::Verse(b, ..)
            | Self::BookLayout(b)
            | Self::Alignment(b, ..) => Some(*b),
            Self::Check(key) => key.book(),
            _ => None,
        }
    }

    /// The record kind's name on the wire, such as `verse` or `carriedFile`.
    pub fn kind_name(&self) -> &str {
        match self {
            Self::BookPresence(_) => "bookPresence",
            Self::BookHeader(_) => "bookHeader",
            Self::ChapterFront(..) => "chapterFront",
            Self::Verse(..) => "verse",
            Self::BookLayout(_) => "bookLayout",
            Self::Alignment(..) => "alignment",
            Self::Check(_) => "check",
            Self::Comment(_) => "comment",
            Self::Pins(_) => "pins",
            Self::ResourcesExtra(_) => "resourcesExtra",
            Self::Setting(_) => "setting",
            Self::ProjectMeta(_) => "projectMeta",
            Self::Versification => "versification",
            Self::DeviceProfile(_) => "deviceProfile",
            Self::StoryBase(_) => "storyBase",
            Self::Frame(..) => "frame",
            Self::StoryRef(_) => "storyRef",
            Self::CarriedFile(_) => "carriedFile",
            Self::BatchRelease(_) => "batchRelease",
            Self::Unknown(key) => &key.kind,
        }
    }
}

/// The kind names this format version knows, in wire spelling.
pub const KNOWN_KINDS: &[&str] = &[
    "bookPresence",
    "bookHeader",
    "chapterFront",
    "verse",
    "bookLayout",
    "alignment",
    "check",
    "comment",
    "pins",
    "resourcesExtra",
    "setting",
    "projectMeta",
    "versification",
    "deviceProfile",
    "storyBase",
    "frame",
    "storyRef",
    "carriedFile",
    "batchRelease",
];

/// Why an identity string is refused (rule V2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error("{what} is not in Unicode normalization form C")]
    NotNfc { what: &'static str },
    #[error("{what} is empty")]
    Empty { what: &'static str },
    #[error("{what} {text:?} does not match its grammar: {rule}")]
    Grammar {
        what: &'static str,
        text: String,
        rule: &'static str,
    },
}

fn nfc_non_empty(what: &'static str, text: &str) -> Result<(), IdentityError> {
    if text.is_empty() {
        return Err(IdentityError::Empty { what });
    }
    if !is_nfc(text) {
        return Err(IdentityError::NotNfc { what });
    }
    Ok(())
}

/// Object keys a JavaScript reader would treat as the prototype, so tC4 refuses them.
const PROTOTYPE_NAMES: &[&str] = &["__proto__", "constructor", "prototype"];

macro_rules! identity_string {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Box<str>);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), self.0)
            }
        }
    };
}

identity_string!(
    /// tC4's check id: NFC, not empty, no `|` or `:`.
    CheckId
);

impl CheckId {
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        nfc_non_empty("a check id", text)?;
        if text.contains(['|', ':']) {
            return Err(IdentityError::Grammar {
                what: "a check id",
                text: text.to_owned(),
                rule: "it may not contain '|' or ':'",
            });
        }
        Ok(Self(text.into()))
    }
}

identity_string!(
    /// A chapter or verse as tC4 writes it in a check's reference: `1`, `9-10`, `front`,
    /// `1,3,8,12` (en_tn's sbh4 at PSA 5). Compared as a string. tC4's identity part grammar
    /// (`identityPartError`, `journal/grammar.mjs` at d310bf3): not empty, no `|` (its
    /// identity delimiter) and no `:` (its register delimiter); and NFC, as every identity
    /// string here (rule V2).
    RefPart
);

impl RefPart {
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        nfc_non_empty("a reference part", text)?;
        if text.contains(['|', ':']) {
            return Err(IdentityError::Grammar {
                what: "a reference part",
                text: text.to_owned(),
                rule: "it may not contain '|' or ':'",
            });
        }
        Ok(Self(text.into()))
    }

    /// The number this part names, when it is one: plain ASCII digits, as tC4's `refPart`
    /// reads a number (`derive.ts`).
    pub fn number(&self) -> Option<u16> {
        if self.0.bytes().all(|b| b.is_ascii_digit()) {
            self.0.parse().ok()
        } else {
            None
        }
    }

    /// The verse span this part names, when it is a number or `first-last` with numbers:
    /// the keys of tC4's draft-text index (`state.jsx` `verseTextIndex`).
    pub fn span(&self) -> Option<(u16, u16)> {
        if let Some(n) = self.number() {
            return Some((n, n));
        }
        let (first, last) = self.0.split_once('-')?;
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(first) || !digits(last) {
            return None;
        }
        Some((first.parse().ok()?, last.parse().ok()?))
    }
}

identity_string!(
    /// A top-level key of settings.json: NFC, not empty, not `__proto__`, `constructor` or
    /// `prototype`.
    SettingKey
);

impl SettingKey {
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        nfc_non_empty("a setting key", text)?;
        if PROTOTYPE_NAMES.contains(&text) {
            return Err(IdentityError::Grammar {
                what: "a setting key",
                text: text.to_owned(),
                rule: "__proto__, constructor and prototype are reserved",
            });
        }
        Ok(Self(text.into()))
    }
}

identity_string!(
    /// A tC4 pin slot name such as `gatewayLanguage`, `translationNotes` or `obs-tn`: an
    /// ASCII letter, then letters, digits, `_` and `-`, at most 64 bytes, not `constructor` or
    /// `prototype`. tC4's slot grammar (`PIN_SLOT_RE`, `journal/grammar.mjs` at d310bf3)
    /// names `obs-tn`, `obs-twl`, `obs-tq` and `obs-images`, and allows `-` in an
    /// `extraScripture` pane id.
    SlotName
);

impl SlotName {
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        let bytes = text.as_bytes();
        let ok = (1..=64).contains(&bytes.len())
            && bytes[0].is_ascii_alphabetic()
            && bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            && !PROTOTYPE_NAMES.contains(&text);
        if !ok {
            return Err(IdentityError::Grammar {
                what: "a pin slot name",
                text: text.to_owned(),
                rule: "an ASCII letter, then letters, digits, '_' and '-', at most 64 bytes, not a prototype name",
            });
        }
        Ok(Self(text.into()))
    }
}

/// Where in `resources.json` a key the pin groups do not hold stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourcesLevel {
    /// Beside `schemaVersion`, `languageSets`, `resources` and `extraScripture`.
    Top,
    /// Under `languageSets`, beside `primary` and `fallback`.
    LanguageSets,
    /// Under `resources`, beside `originalLanguage` and `lexicon`.
    Resources,
}

impl ResourcesLevel {
    /// The members the projection writes at this level, which an extra key may not name.
    pub fn owned(self) -> &'static [&'static str] {
        match self {
            Self::Top => &[
                "schemaVersion",
                "languageSets",
                "resources",
                "extraScripture",
            ],
            Self::LanguageSets => &["primary", "fallback"],
            Self::Resources => &["originalLanguage", "lexicon"],
        }
    }
}

/// The key of a `resources.json` member no pin group holds: its level and its name. The
/// name is NFC, not empty, not a prototype name, and not a member the projection writes at
/// that level.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourcesKey {
    level: ResourcesLevel,
    name: Box<str>,
}

impl ResourcesKey {
    pub fn new(level: ResourcesLevel, name: &str) -> Result<Self, IdentityError> {
        nfc_non_empty("a resources.json key", name)?;
        if PROTOTYPE_NAMES.contains(&name) || level.owned().contains(&name) {
            return Err(IdentityError::Grammar {
                what: "a resources.json key",
                text: name.to_owned(),
                rule: "not a prototype name nor a member the pin groups project",
            });
        }
        Ok(Self {
            level,
            name: name.into(),
        })
    }

    pub fn level(&self) -> ResourcesLevel {
        self.level
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The paths in `metadata.json` that the modelled [`MetaField`]s own, so an `Extra` field
/// cannot name them a second time.
const MODELLED_META_PATHS: &[&str] = &[
    "identification.name",
    "identification.abbreviation",
    "identification.description",
    "languages",
    "copyright",
];

identity_string!(
    /// The dotted `metadata.json` path of a project detail the app does not model, such as
    /// `localizedNames` or `identification.primary.ebible`: NFC, non-empty segments
    /// separated by `.`, no control characters, no prototype names, and neither a path a
    /// modelled field owns nor one above or below such a path, so no field has two records.
    MetaPath
);

impl MetaPath {
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        nfc_non_empty("a project detail path", text)?;
        let grammar = |rule| IdentityError::Grammar {
            what: "a project detail path",
            text: text.to_owned(),
            rule,
        };
        if text.chars().any(char::is_control) {
            return Err(grammar("no control characters"));
        }
        if text
            .split('.')
            .any(|segment| segment.is_empty() || PROTOTYPE_NAMES.contains(&segment))
        {
            return Err(grammar("non-empty segments that are not prototype names"));
        }
        let overlaps = |owned: &&str| text == *owned || nested(text, owned) || nested(owned, text);
        if MODELLED_META_PATHS.iter().any(overlaps) {
            return Err(grammar(
                "not a path a modelled project detail owns, nor one above or below it",
            ));
        }
        Ok(Self(text.into()))
    }
}

/// True when dotted path `inner` lies below `outer`.
fn nested(inner: &str, outer: &str) -> bool {
    inner
        .strip_prefix(outer)
        .is_some_and(|rest| rest.starts_with('.'))
}

/// tC4's normative decision identity, so decisions round-trip with tC3 and tC4 tools.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckKey {
    pub tool: ToolId,
    pub check_id: CheckId,
    pub target: CheckTarget,
    pub occurrence: u32,
}

impl CheckKey {
    pub fn book(&self) -> Option<BookId> {
        match &self.target {
            CheckTarget::Verse { book, .. } => Some(*book),
            CheckTarget::Frame { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolId {
    TranslationWords,
    TranslationNotes,
    TranslationQuestions,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckTarget {
    /// Chapter and verse are tC4's strings: "1", "9-10", "front". Compared as strings.
    Verse {
        book: BookId,
        chapter: RefPart,
        verse: RefPart,
    },
    Frame {
        story: StoryNo,
        frame: FrameNo,
    },
}

/// `resources.json` is projected from five groups. A gateway change rewrites `Primary` and
/// `ExtraScripture` in one batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PinGroup {
    Primary,
    Fallback,
    OriginalLanguage,
    Lexicon,
    ExtraScripture,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetaField {
    Name,
    Abbreviation,
    Description,
    Languages,
    Copyright,
    Extra(MetaPath),
}

/// The key of a record kind from a newer minor format: its kind name and the canonical
/// JSON of its key fields (every member of the record object except `value`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownKey {
    pub kind: Box<str>,
    pub key_json: Box<str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_grammars_refuse_rather_than_repair() {
        assert!(CheckId::parse("a9p2").is_ok());
        assert!(CheckId::parse("").is_err());
        assert!(CheckId::parse("a|b").is_err());
        assert!(CheckId::parse("a:b").is_err());
        assert!(matches!(
            CheckId::parse("e\u{301}"),
            Err(IdentityError::NotNfc { .. })
        ));
        // tC4 keys a check on the cell as written: a comma list, a space, a dash that is not
        // ASCII, other digits (fixtures/check/reference-forms-vectors.json).
        for ok in [
            "1",
            "9-10",
            "front",
            "intro",
            "1,3,8,12",
            "front ",
            " 3",
            "1\u{2013}3",
            "\u{663}",
            &"9".repeat(33),
        ] {
            assert!(RefPart::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["", "1:2", "a|b", "e\u{301}"] {
            assert!(RefPart::parse(bad).is_err(), "{bad}");
        }
        let part = |s: &str| RefPart::parse(s).unwrap();
        assert_eq!(part("12").number(), Some(12));
        assert_eq!(part("9-10").span(), Some((9, 10)));
        assert_eq!(part("4").span(), Some((4, 4)));
        for none in ["1,3,8,12", "front", "1a", "-3", "3-", "\u{663}"] {
            assert_eq!(part(none).span(), None, "{none}");
        }
        assert!(SettingKey::parse("checkCategories").is_ok());
        for bad in ["", "__proto__", "constructor", "prototype"] {
            assert!(SettingKey::parse(bad).is_err(), "{bad}");
        }
        for ok in ["translationNotes", "obs-tn", "obs-images", "a-b"] {
            assert!(SlotName::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "_x",
            "-x",
            "1x",
            "a b",
            "a.b",
            "constructor",
            &"a".repeat(65),
        ] {
            assert!(SlotName::parse(bad).is_err(), "{bad}");
        }
        assert!(MetaPath::parse("localizedNames").is_ok());
        assert!(MetaPath::parse("identification.primary.ebible").is_ok());
        assert!(MetaPath::parse("identification.upstream").is_ok());
        assert!(MetaPath::parse("languagesExtra").is_ok());
        for bad in [
            "",
            "a..b",
            ".a",
            "a.__proto__",
            "languages",
            "identification.name",
            "identification.name.en",
            "identification",
            "languages.0",
            "a\nb",
        ] {
            assert!(MetaPath::parse(bad).is_err(), "{bad}");
        }
    }
}
