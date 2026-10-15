//! Identifiers (`docs/STORAGE.md` §2.1). Each one parses its text form strictly, so a value
//! has exactly one text and the canonical bytes of a change are unique.

use crate::text::{base32, hex, sha256, unbase32, unhex};
use std::fmt;
use std::num::NonZeroU16;
use unicode_normalization::is_nfc;

macro_rules! hex_id {
    ($(#[$doc:meta])* $name:ident, $len:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $len]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            /// Exactly the lowercase hex this type writes.
            pub fn parse(text: &str) -> Option<Self> {
                unhex::<$len>(text).map(Self)
            }

            pub fn to_hex(&self) -> String {
                hex(&self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.to_hex())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.to_hex())
            }
        }
    };
}

hex_id!(
    /// SHA-256 of a change's canonical JSON bytes. Text form: 64 lowercase hex.
    ChangeId,
    32
);
hex_id!(
    /// The first 16 bytes of a [`ChangeId`]. Used only in `BookPresence::Removed::seen`.
    ShortId,
    16
);
hex_id!(
    /// SHA-256 of a pack file's bytes.
    PackId,
    32
);
hex_id!(
    /// The first 16 bytes of a [`PackId`]: a pack's file name without `.ndjson`, and what a
    /// consolidated pack's `replaces` lists.
    PackName,
    16
);
hex_id!(
    /// One user action: a section save, a resolution, an upgrade, one ingested outside edit.
    BatchId,
    16
);
hex_id!(
    /// A user comment's identity, random at creation.
    CommentId,
    16
);
hex_id!(
    /// An MD5 digest; lowercase hex on the wire.
    Md5,
    16
);
hex_id!(
    /// A git object id; also the identity of a pinned resource version (a commit sha).
    GitOid,
    20
);

impl ChangeId {
    /// The id of a change whose canonical bytes are `raw`.
    pub fn of(raw: &[u8]) -> Self {
        Self(sha256(raw))
    }

    pub fn short(&self) -> ShortId {
        let mut out = [0u8; 16];
        out.copy_from_slice(&self.0[..16]);
        ShortId(out)
    }
}

impl PackId {
    /// The id of a pack whose bytes are `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha256(bytes))
    }

    pub fn name(&self) -> PackName {
        let mut out = [0u8; 16];
        out.copy_from_slice(&self.0[..16]);
        PackName(out)
    }
}

// ---------------------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------------------

/// What kind of writer a [`DeviceId`] stands for. The value is byte 0 of the id.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum DeviceKind {
    /// One installation writing in one project folder on one machine (§6.3).
    Local = 1,
    /// A tC4 actor: sha256("tc4-actor\0" ‖ actorId)[..15].
    Tc4Actor = 2,
    /// A tC3 user: sha256("tc3-user\0" ‖ projectId ‖ userName)[..15].
    Tc3User = 3,
    /// One file edited in one Door43 commit: sha256("door43-edit\0" ‖ commit oid ‖ path)[..15].
    Door43Edit = 4,
    /// One file edited inside one exchange file: sha256("exchange-edit\0" ‖ md5 of the
    /// file's metadata.json ‖ path)[..15].
    ExchangeEdit = 5,
    /// Deterministic import changes such as the tC4 reconcile:
    /// sha256("import\0" ‖ kind ‖ projectId)[..15].
    Importer = 6,
}

impl DeviceKind {
    pub fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::Local,
            2 => Self::Tc4Actor,
            3 => Self::Tc3User,
            4 => Self::Door43Edit,
            5 => Self::ExchangeEdit,
            6 => Self::Importer,
            _ => return None,
        })
    }
}

/// 16 bytes. Byte 0 is the [`DeviceKind`]. Bytes 1..16 are random (Local) or a hash (every
/// other kind). Text form: `d` + 26 lowercase RFC 4648 base32 characters, no padding, which
/// is always a valid ingredient path segment.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId([u8; 16]);

impl DeviceId {
    /// A Local device from 15 random bytes.
    pub fn local(random: [u8; 15]) -> Self {
        Self::with_kind(DeviceKind::Local, random)
    }

    /// A derived device: the first 15 bytes of sha256(`input`). The caller builds `input`
    /// as the kind's formula says (see [`DeviceKind`]).
    pub fn derived(kind: DeviceKind, input: &[u8]) -> Self {
        let hash = sha256(input);
        let mut rest = [0u8; 15];
        rest.copy_from_slice(&hash[..15]);
        Self::with_kind(kind, rest)
    }

    pub fn with_kind(kind: DeviceKind, rest: [u8; 15]) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0] = kind as u8;
        bytes[1..].copy_from_slice(&rest);
        Self(bytes)
    }

    pub fn kind(&self) -> DeviceKind {
        // Every constructor and the parser store a known kind in byte 0.
        DeviceKind::from_byte(self.0[0]).unwrap_or(DeviceKind::Local)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Exactly the text [`fmt::Display`] writes, with a known kind.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = unbase32::<16>(text.strip_prefix('d')?)?;
        DeviceKind::from_byte(bytes[0])?;
        Some(Self(bytes))
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "d{}", base32(&self.0))
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({self})")
    }
}

/// Local devices: 1, 2, 3, … strictly increasing, one per change. Other kinds: the index of
/// the change inside its batch. Used to detect two machines writing as one Local device and
/// to name files; never used for ordering or survival.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DeviceSeq(pub u64);

/// Milliseconds since the Unix epoch, UTC, from the author's clock. Display and provisional
/// choice only (I3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct WallTime(pub i64);

/// Every change names its batch and the batch's size. A batch applies only when all `n`
/// changes are present (§7.2 F2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BatchRef {
    pub id: BatchId,
    n: u32,
}

impl BatchRef {
    /// `None` when `n` is zero: a batch holds at least one change.
    pub fn new(id: BatchId, n: u32) -> Option<Self> {
        (n > 0).then_some(Self { id, n })
    }

    pub fn n(&self) -> u32 {
        self.n
    }
}

/// Random UUID v4 at project creation; derived for a tC4 import. Text form: lowercase
/// hyphenated.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectId(uuid::Uuid);

impl ProjectId {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(uuid::Uuid::from_bytes(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }

    /// Exactly the lowercase hyphenated text [`fmt::Display`] writes.
    pub fn parse(text: &str) -> Option<Self> {
        let id = uuid::Uuid::try_parse(text).ok()?;
        (id.hyphenated().to_string() == text).then_some(Self(id))
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

impl fmt::Debug for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProjectId({self})")
    }
}

// ---------------------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------------------

/// A chapter number, 1 to 65,535.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ChapterNo(NonZeroU16);

/// A verse number, 1 to 65,535.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VerseNo(NonZeroU16);

macro_rules! nonzero_number {
    ($name:ident) => {
        impl $name {
            pub fn new(n: u16) -> Option<Self> {
                NonZeroU16::new(n).map(Self)
            }

            pub fn get(self) -> u16 {
                self.0.get()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}
nonzero_number!(ChapterNo);
nonzero_number!(VerseNo);

/// An Open Bible Stories story, 1 to 50.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StoryNo(u8);

impl StoryNo {
    pub fn new(n: u8) -> Option<Self> {
        (1..=50).contains(&n).then_some(Self(n))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

/// A frame of a story; frame 0 is the story's title.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FrameNo(pub u16);

// ---------------------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------------------

/// Characters §3.3 refuses in a path on top of what `tc_burrito::IngredientPath` refuses.
const PATH_FORBIDDEN: &[char] = &['~', '&', '+', '#', '%', '{', '}', '$', '!', '\''];

/// A path relative to the repository root, `/`-separated, in NFC, that
/// `tc_burrito::IngredientPath` accepts and that holds none of `~ & + # % { } $ ! '`
/// (§3.3). Two paths collide when their [`RepoPath::collision_form`]s are equal.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath(Box<str>);

/// Why a string is not a [`RepoPath`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("the path is not in Unicode normalization form C")]
    NotNfc,
    #[error("{0}")]
    Ingredient(String),
    #[error("the path contains {0:?}")]
    Forbidden(char),
}

impl RepoPath {
    pub fn parse(text: &str) -> Result<Self, PathError> {
        if !is_nfc(text) {
            return Err(PathError::NotNfc);
        }
        tc_burrito::IngredientPath::parse(text)
            .map_err(|e| PathError::Ingredient(e.to_string()))?;
        if let Some(c) = text.chars().find(|c| PATH_FORBIDDEN.contains(c)) {
            return Err(PathError::Forbidden(c));
        }
        Ok(Self(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The form under which Windows and macOS see two names as one file (§3.3).
    pub fn collision_form(&self) -> String {
        tc_burrito::collision_form(&self.0)
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RepoPath({:?})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_ids_have_one_text_and_a_known_kind() {
        let id = DeviceId::local([7; 15]);
        let text = id.to_string();
        assert!(text.starts_with('d') && text.len() == 27, "{text}");
        assert_eq!(DeviceId::parse(&text), Some(id));
        assert_eq!(id.kind(), DeviceKind::Local);
        assert!(tc_burrito::IngredientPath::parse(&text).is_ok());
        assert_eq!(DeviceId::parse(&text.to_uppercase()), None);
        assert_eq!(DeviceId::parse(&text[1..]), None);
        let mut unknown = *id.as_bytes();
        unknown[0] = 9;
        assert_eq!(DeviceId::parse(&format!("d{}", base32(&unknown))), None);
        let derived = DeviceId::derived(DeviceKind::Tc4Actor, b"tc4-actor\0abc");
        assert_eq!(derived.kind(), DeviceKind::Tc4Actor);
    }

    #[test]
    fn project_ids_are_lowercase_hyphenated() {
        let text = "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50";
        let id = ProjectId::parse(text).unwrap();
        assert_eq!(id.to_string(), text);
        assert_eq!(ProjectId::parse(&text.to_uppercase()), None);
        assert_eq!(ProjectId::parse(&text.replace('-', "")), None);
        assert_eq!(ProjectId::parse(&format!("{{{text}}}")), None);
    }

    #[test]
    fn repo_paths_follow_the_ingredient_grammar_and_section_3_3() {
        for ok in [
            "ingredients/audio/a.wav",
            "LICENSE.md",
            "ingredients/Jonás.usfm",
        ] {
            assert!(RepoPath::parse(ok).is_ok(), "{ok}");
        }
        assert_eq!(
            RepoPath::parse("ingredients/Jona\u{301}s.usfm"),
            Err(PathError::NotNfc)
        );
        assert_eq!(
            RepoPath::parse("ingredients/a&b.wav"),
            Err(PathError::Forbidden('&'))
        );
        assert_eq!(
            RepoPath::parse("ingredients/Tom's.wav"),
            Err(PathError::Forbidden('\''))
        );
        assert!(matches!(
            RepoPath::parse("ingredients/a b.wav"),
            Err(PathError::Ingredient(_))
        ));
        assert!(RepoPath::parse(".gitignore").is_err());
        assert_eq!(
            RepoPath::parse("ingredients/TIT.usfm")
                .unwrap()
                .collision_form(),
            RepoPath::parse("Ingredients/tit.USFM")
                .unwrap()
                .collision_form()
        );
    }

    #[test]
    fn numbers_refuse_zero_and_out_of_range_stories() {
        assert!(ChapterNo::new(0).is_none() && VerseNo::new(0).is_none());
        assert!(StoryNo::new(0).is_none() && StoryNo::new(51).is_none());
        assert_eq!(StoryNo::new(50).map(StoryNo::get), Some(50));
        assert!(BatchRef::new(BatchId::from_bytes([0; 16]), 0).is_none());
    }
}
