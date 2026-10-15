//! What a project pins: a Door43 resource at one commit, or an Aquifer release asset.
//!
//! # How the stored records map to these types
//!
//! A project stores its pins as `Record::Pins { group, value: PinGroupValue }` (STORAGE
//! §2.2-§2.3), one record per [`crate::PinGroup`]. Each slot of a group's value is tC4's
//! §5.3 pin object, which this module reads as a [`ResourcePin`]:
//!
//! | tC4 field | Here | Notes |
//! |---|---|---|
//! | `repoPath` (`git.door43.org/<owner>/<repo>`) | [`ResourcePin::repo`] | compared without regard to ASCII case; the casing Door43 reports is kept |
//! | `sha` (40 hex) | [`ResourcePin::sha`] | the identity (D58); `ResourceRef::sha` in STORAGE |
//! | `version` | [`ResourcePin::version`] | a display label and the tag a download asks for; never compared |
//! | `flavor` | [`ResourcePin::flavor`] | `flavorType/flavor`, such as `parascriptural/x-bcvnotes` |
//! | `books` | [`ResourcePin::books`] | recorded coverage: book codes, or `["BIBLE"]` for the whole collection |
//!
//! A decision's `resource` (STORAGE `ResourceRef { repo_path, sha }`) is the same identity
//! without the label: [`ResourcePin::same_identity`] is the comparison the fold's
//! `resource_ok` makes.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256 as Sha256Hasher};
use tc_core::BookId;
use tc_door43::{AccountName, CommitSha, RepoName, RepoRef, TagName};

use crate::error::ParseError;

/// The Door43 host every resource is read from (tC4 `DCS_HOST`). Reads always go to
/// production; only account and write calls use the QA server in development.
pub const DCS_HOST: &str = "git.door43.org";

/// tC4's `repoPath`: `<host>/<owner>/<repo>`, such as `git.door43.org/unfoldingWord/en_tn`.
/// It compares, sorts and hashes without regard to ASCII case, as Door43 resolves names,
/// and keeps the casing it was given.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath {
    host: Host,
    repo: RepoRef,
}

/// A host name, kept in lowercase. It is part of the stored form only: an install is
/// identified by owner, repository and commit, wherever the bytes came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Host(String);

impl RepoPath {
    /// A repository on Door43 production.
    pub fn door43(repo: RepoRef) -> Self {
        Self {
            host: Host(DCS_HOST.to_owned()),
            repo,
        }
    }

    pub fn repo(&self) -> &RepoRef {
        &self.repo
    }

    pub fn owner(&self) -> &AccountName {
        &self.repo.owner
    }

    pub fn name(&self) -> &RepoName {
        &self.repo.name
    }

    pub fn host(&self) -> &str {
        &self.host.0
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.host.0, self.repo)
    }
}

impl FromStr for RepoPath {
    type Err = ParseError;

    /// Accepts `host/owner/repo`, with or without an `https://` prefix, as tC4 writes it.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || ParseError::RepoPath(s.to_owned());
        let rest = s
            .strip_prefix("https://")
            .or_else(|| s.strip_prefix("http://"))
            .unwrap_or(s);
        let mut parts = rest.split('/');
        let (Some(host), Some(owner), Some(name), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        let host_ok = !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'));
        if !host_ok {
            return Err(bad());
        }
        Ok(Self {
            host: Host(host.to_ascii_lowercase()),
            repo: RepoRef::new(
                owner.parse().map_err(|_| bad())?,
                name.parse().map_err(|_| bad())?,
            ),
        })
    }
}

impl Serialize for RepoPath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RepoPath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// The books a pinned commit holds, as tC4 records them in a pin's `books`. The list
/// keeps the order it was recorded in, so a pin read and written back is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BookCoverage {
    /// Never empty: an empty record is no record (tC4 `coverageFor`).
    Books(Vec<BookId>),
    /// `["BIBLE"]`: a resource that is not divided into books covers every book.
    WholeCollection,
}

/// tC4's whole-collection marker.
const WHOLE_COLLECTION: &str = "BIBLE";

impl BookCoverage {
    /// `None` for no books: an empty list records nothing. Each book is listed once, in
    /// the order given.
    pub fn books(books: impl IntoIterator<Item = BookId>) -> Option<Self> {
        let mut list: Vec<BookId> = Vec::new();
        for book in books {
            if !list.contains(&book) {
                list.push(book);
            }
        }
        (!list.is_empty()).then_some(Self::Books(list))
    }

    pub fn covers(&self, book: BookId) -> bool {
        match self {
            Self::Books(list) => list.contains(&book),
            Self::WholeCollection => true,
        }
    }

    /// The stored form: book codes as recorded, or `["BIBLE"]`.
    pub fn codes(&self) -> Vec<String> {
        match self {
            Self::Books(list) => list.iter().map(|b| b.code().to_owned()).collect(),
            Self::WholeCollection => vec![WHOLE_COLLECTION.to_owned()],
        }
    }

    /// Reads the stored form. `["BIBLE"]` alone is the whole collection; a marker beside
    /// book codes, an unknown code or an empty list is refused, so a malformed record is
    /// reported where it is read.
    pub fn from_codes<S: AsRef<str>>(codes: &[S]) -> Result<Self, ParseError> {
        let bad = || {
            ParseError::Coverage(
                codes
                    .iter()
                    .map(|c| c.as_ref())
                    .collect::<Vec<_>>()
                    .join(","),
            )
        };
        match codes {
            [] => Err(bad()),
            [one] if one.as_ref() == WHOLE_COLLECTION => Ok(Self::WholeCollection),
            _ => {
                let books = codes
                    .iter()
                    .map(|c| c.as_ref().parse::<BookId>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| bad())?;
                Ok(Self::Books(books))
            }
        }
    }
}

impl Serialize for BookCoverage {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.codes().serialize(s)
    }
}

impl<'de> Deserialize<'de> for BookCoverage {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let codes = Vec::<String>::deserialize(d)?;
        Self::from_codes(&codes).map_err(serde::de::Error::custom)
    }
}

/// A Door43 resource pinned to one commit: tC4's §5.3 pin. The commit is the identity;
/// the version is a label (D58).
///
/// tC4 requires a flavor of every pin it stores. A pin made from a release tag
/// ([`crate::pin_release`]) has none until its resource is installed; the pin a project
/// keeps is [`crate::Installed::dcs_pin`], which carries the flavor the resource
/// declares. Writing a pin without one is refused ([`crate::UnwritablePin`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePin {
    #[serde(rename = "repoPath")]
    pub repo: RepoPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<TagName>,
    pub sha: CommitSha,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,
    /// Recorded coverage. An empty list records nothing and reads as `None`, as tC4
    /// `coverageFor` reads it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "recorded_books"
    )]
    pub books: Option<BookCoverage>,
    /// Fields this version does not know (a newer app's), kept as read and written back.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn recorded_books<'de, D: Deserializer<'de>>(d: D) -> Result<Option<BookCoverage>, D::Error> {
    match Option::<Vec<String>>::deserialize(d)? {
        None => Ok(None),
        Some(codes) if codes.is_empty() => Ok(None),
        Some(codes) => BookCoverage::from_codes(&codes)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

impl ResourcePin {
    pub fn new(repo: RepoPath, sha: CommitSha) -> Self {
        Self {
            repo,
            version: None,
            sha,
            flavor: None,
            books: None,
            extra: Map::new(),
        }
    }

    pub fn with_version(mut self, version: TagName) -> Self {
        self.version = Some(version);
        self
    }

    pub fn with_flavor(mut self, flavor: impl Into<String>) -> Self {
        self.flavor = Some(flavor.into());
        self
    }

    pub fn with_books(mut self, books: BookCoverage) -> Self {
        self.books = Some(books);
        self
    }

    /// Repository (without regard to case) and commit: tC4's `pinKey`, the fold's
    /// `resource_ok`. The label, flavor and coverage are not compared.
    pub fn same_identity(&self, other: &ResourcePin) -> bool {
        self.repo.repo == other.repo.repo && self.sha == other.sha
    }

    /// Whether this pin is a commit of `repo`, at any commit.
    pub fn is_repo(&self, repo: &RepoRef) -> bool {
        self.repo.repo == *repo
    }
}

/// `unfoldingWord/en_tn v86 (c354b8a…)`.
impl fmt::Display for ResourcePin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.repo.repo)?;
        if let Some(v) = &self.version {
            write!(f, " {v}")?;
        }
        write!(f, " ({}…)", &self.sha.as_str()[..7])
    }
}

/// A SHA-256 digest, as GitHub reports a release asset's (`sha256:<hex>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256Hasher::digest(bytes).into())
    }

    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Sha256 {
    type Err = ParseError;

    /// 64 hex digits, optionally after `sha256:`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix("sha256:").unwrap_or(s);
        let bad = || ParseError::Sha256(s.to_owned());
        // `from_str_radix` alone would take a sign: "+a" reads as 0x0a.
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
        }
        Ok(Self(out))
    }
}

impl Serialize for Sha256 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// A GitHub account or repository name: letters, digits, `.`, `_` and `-`, at most 100
/// characters, not `.` or `..`. Compared without regard to ASCII case, as GitHub resolves
/// names.
#[derive(Debug, Clone)]
pub struct GithubName(String);

impl GithubName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for GithubName {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for GithubName {}

impl PartialOrd for GithubName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GithubName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let lower = |b: u8| b.to_ascii_lowercase();
        self.0.bytes().map(lower).cmp(other.0.bytes().map(lower))
    }
}

impl std::hash::Hash for GithubName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

impl fmt::Display for GithubName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for GithubName {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ok = !s.is_empty()
            && s.len() <= 100
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if ok {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::GithubName(s.to_owned()))
        }
    }
}

/// A BibleAquifer repository on GitHub, such as `BibleAquifer/UWTranslationNotes`. The
/// repository name is Aquifer's `resource_code`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AquiferRepo {
    pub owner: GithubName,
    pub name: GithubName,
}

/// The GitHub organization that publishes Aquifer.
pub const AQUIFER_OWNER: &str = "BibleAquifer";

impl AquiferRepo {
    /// A repository of the BibleAquifer organization.
    pub fn bible_aquifer(name: &str) -> Result<Self, ParseError> {
        Ok(Self {
            owner: AQUIFER_OWNER.parse()?,
            name: name.parse()?,
        })
    }
}

impl fmt::Display for AquiferRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl FromStr for AquiferRepo {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (owner, name) = s
            .split_once('/')
            .ok_or_else(|| ParseError::GithubName(s.to_owned()))?;
        Ok(Self {
            owner: owner.parse()?,
            name: name.parse()?,
        })
    }
}

impl Serialize for AquiferRepo {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AquiferRepo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// An Aquifer language: ISO 639-3, three lowercase letters, such as `eng` or `por`.
/// Door43 uses BCP 47 tags (`en`, `pt-br`) instead, so the two never compare equal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AquiferLanguage(String);

impl AquiferLanguage {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AquiferLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for AquiferLanguage {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 3 && s.bytes().all(|b| b.is_ascii_lowercase()) {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::AquiferLanguage(s.to_owned()))
        }
    }
}

impl Serialize for AquiferLanguage {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AquiferLanguage {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// The file name of a release asset, such as `Portuguese-json.zip`: one path segment of
/// printable ASCII without `/`, `\` or `..`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetName(String);

impl AssetName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The per-language JSON asset Aquifer publishes: `<Language>-json.zip`.
    pub fn is_json(&self) -> bool {
        self.0.ends_with("-json.zip")
    }
}

impl fmt::Display for AssetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for AssetName {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ok = !s.is_empty()
            && s.len() <= 255
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'\\' | b'?' | b'#' | b'%'));
        if ok {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::AssetName(s.to_owned()))
        }
    }
}

impl Serialize for AssetName {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AssetName {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// An Aquifer resource pinned to one release asset (research `aquifer.md`, R4): the
/// repository, the release tag (a label), the commit the tag named, the language of the
/// asset and the asset's SHA-256. The digest binds the bytes; GitHub lets a maintainer
/// re-upload an asset or move a lightweight tag, so neither the tag nor the commit alone
/// would.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AquiferPin {
    pub repo: AquiferRepo,
    pub tag: TagName,
    pub commit: CommitSha,
    pub language: AquiferLanguage,
    pub asset: AssetName,
    pub sha256: Sha256,
}

/// `BibleAquifer/UWTranslationNotes por v2026-10-12 (dbfb46e…)`.
impl fmt::Display for AquiferPin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} ({}…)",
            self.repo,
            self.language,
            self.tag,
            &self.commit.as_str()[..7]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_paths_read_tc4s_forms_and_compare_without_case() {
        let a: RepoPath = "git.door43.org/Es-419_gl/es-419_tn".parse().unwrap();
        let b: RepoPath = "https://git.door43.org/es-419_gl/ES-419_TN"
            .parse()
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_string(), "git.door43.org/Es-419_gl/es-419_tn");
        for bad in [
            "git.door43.org/unfoldingWord",
            "git.door43.org/a/b/c",
            "/a/b",
            "git.door43.org/../b",
        ] {
            assert!(bad.parse::<RepoPath>().is_err(), "{bad}");
        }
    }

    #[test]
    fn coverage_reads_codes_and_the_whole_collection_marker() {
        let c = BookCoverage::from_codes(&["TIT", "jon"]).unwrap();
        assert!(c.covers(BookId::Titus) && c.covers(BookId::Jonah));
        assert!(!c.covers(BookId::Ruth));
        assert_eq!(c.codes(), ["TIT", "JON"]);
        assert_eq!(
            BookCoverage::from_codes(&["BIBLE"]).unwrap(),
            BookCoverage::WholeCollection
        );
        for bad in [&["BIBLE", "TIT"][..], &[][..], &["TRANSLATE"][..]] {
            assert!(BookCoverage::from_codes(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn sha256_hex_round_trips_with_and_without_the_prefix() {
        let digest = Sha256::of(b"abc");
        let hex = digest.to_hex();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hex.parse::<Sha256>().unwrap(), digest);
        assert_eq!(format!("sha256:{hex}").parse::<Sha256>().unwrap(), digest);
        assert!("sha256:abc".parse::<Sha256>().is_err());
        // A sign is not a hex digit, though `from_str_radix` takes one.
        assert!("+a".repeat(32).parse::<Sha256>().is_err());
        assert!(
            format!("sha256:{}", "-0".repeat(32))
                .parse::<Sha256>()
                .is_err()
        );
        assert!("g".repeat(64).parse::<Sha256>().is_err());
    }

    #[test]
    fn an_empty_books_list_records_nothing_as_in_tc4() {
        let json = r#"{"repoPath":"git.door43.org/unfoldingWord/en_tn","sha":"e586762e330f482a60c52aedd1c7b3a2f155df8a","flavor":"parascriptural/x-bcvnotes","books":[]}"#;
        let pin: ResourcePin = serde_json::from_str(json).unwrap();
        assert_eq!(pin.books, None);
        let bad = json.replace("[]", r#"["BIBLE","TIT"]"#);
        assert!(serde_json::from_str::<ResourcePin>(&bad).is_err());
    }

    #[test]
    fn a_newer_apps_pin_fields_are_kept() {
        let json = r#"{"repoPath":"git.door43.org/unfoldingWord/en_tn","sha":"e586762e330f482a60c52aedd1c7b3a2f155df8a","flavor":"parascriptural/x-bcvnotes","x-future":{"a":[1,2]}}"#;
        let pin: ResourcePin = serde_json::from_str(json).unwrap();
        assert!(pin.extra.contains_key("x-future"));
        let back: Value = serde_json::to_value(&pin).unwrap();
        assert_eq!(back, serde_json::from_str::<Value>(json).unwrap());
    }

    #[test]
    fn a_pin_round_trips_through_tc4s_json_shape() {
        let json = r#"{"repoPath":"git.door43.org/unfoldingWord/en_tn","version":"v91","sha":"e586762e330f482a60c52aedd1c7b3a2f155df8a","flavor":"parascriptural/x-bcvnotes","books":["TIT"]}"#;
        let pin: ResourcePin = serde_json::from_str(json).unwrap();
        assert_eq!(pin.version.as_ref().unwrap().as_str(), "v91");
        assert_eq!(serde_json::to_string(&pin).unwrap(), json);
    }
}
