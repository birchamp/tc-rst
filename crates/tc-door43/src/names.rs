//! Typed identifiers for Door43: servers, accounts, repositories, tags, commits, tokens.
//!
//! Each type is parsed once, at the boundary, and is valid from then on. A value that
//! passes these rules is also safe as a URL path segment, so no route is built from an
//! unchecked string.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error(
        "{what} {value:?} is not a Door43 name: use letters, digits, '.', '_' or '-', \
         starting and ending with a letter or digit, at most 100 characters"
    )]
    InvalidName { what: &'static str, value: String },
    #[error("{0:?} is not a release tag this client accepts")]
    InvalidTag(String),
    #[error("{0:?} is not a full 40-character hexadecimal commit sha")]
    InvalidCommitSha(String),
    #[error("{0:?} is not a token name: use 1 to 100 printable characters")]
    InvalidTokenName(String),
    #[error("the sign-in name must be non-empty and contain no ':' or control characters")]
    InvalidLogin,
    #[error("the server address {value:?} is refused: {reason}")]
    InvalidServerUrl { value: String, reason: &'static str },
}

const MAX_NAME_LEN: usize = 100;

/// Door43's rule for account and repository names (tC4 `door43Api.ts` `assertName`).
fn is_door43_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    let edge_ok = |b: &u8| b.is_ascii_alphanumeric();
    !s.is_empty()
        && s.len() <= MAX_NAME_LEN
        && bytes.first().is_some_and(edge_ok)
        && bytes.last().is_some_and(edge_ok)
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

macro_rules! string_serde {
    ($ty:ty) => {
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                raw.parse().map_err(serde::de::Error::custom)
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

/// A Door43 account: a user or an organization. Door43 compares names without regard to
/// case; this type keeps the casing it was given.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountName(String);

impl AccountName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AccountName {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if is_door43_name(s) {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::InvalidName {
                what: "account",
                value: s.to_owned(),
            })
        }
    }
}

string_serde!(AccountName);

/// A repository name within an account.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoName(String);

impl RepoName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RepoName {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        // Gitea refuses these suffixes, and `<name>.git` must name exactly one repository
        // in a clone URL.
        let reserved = lower.ends_with(".git") || lower.ends_with(".wiki");
        if is_door43_name(s) && !reserved {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::InvalidName {
                what: "repository",
                value: s.to_owned(),
            })
        }
    }
}

string_serde!(RepoName);

/// `owner/name` of one repository.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner: AccountName,
    pub name: RepoName,
}

impl RepoRef {
    pub fn new(owner: AccountName, name: RepoName) -> Self {
        Self { owner, name }
    }
}

impl fmt::Display for RepoRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl FromStr for RepoRef {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (owner, name) = s.split_once('/').ok_or_else(|| ParseError::InvalidName {
            what: "repository path",
            value: s.to_owned(),
        })?;
        Ok(Self::new(owner.parse()?, name.parse()?))
    }
}

/// A release tag such as `v86` or `v0.34`. Slashes are allowed between segments, as in git.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagName(String);

impl TagName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `/`-separated segments, for building a URL path.
    pub(crate) fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

impl FromStr for TagName {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let segment_ok = |seg: &str| {
            !seg.is_empty()
                && !seg.starts_with('.')
                && !seg.ends_with('.')
                && !seg.ends_with(".lock")
                && !seg.contains("..")
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
        };
        if s.len() <= 255 && !s.starts_with('-') && s.split('/').all(segment_ok) {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::InvalidTag(s.to_owned()))
        }
    }
}

string_serde!(TagName);

/// A full SHA-1 commit id, stored in lowercase. It is the identity of a pinned resource;
/// a tag is only a label for it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitSha(String);

impl CommitSha {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CommitSha {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Self(s.to_ascii_lowercase()))
        } else {
            Err(ParseError::InvalidCommitSha(s.to_owned()))
        }
    }
}

string_serde!(CommitSha);

/// The name of an access token on the user's account. Door43 refuses a second token with
/// the same name, so minting replaces the old one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenName(String);

impl TokenName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for TokenName {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.is_empty() && s.chars().count() <= 100 && !s.chars().any(char::is_control) {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseError::InvalidTokenName(s.to_owned()))
        }
    }
}

impl fmt::Display for TokenName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A Gitea token scope. A write scope includes the read of its group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenScope {
    ReadRepository,
    WriteRepository,
    ReadOrganization,
    WriteOrganization,
    ReadUser,
    WriteUser,
}

impl TokenScope {
    /// What sharing a project needs: push (`write:repository`), create a repository in an
    /// organization (`write:organization`) or in the user's account (`write:user`). Door43
    /// refused repository creation with the read scopes (tC4 issue #467).
    pub const FOR_SHARING: [TokenScope; 3] = [
        TokenScope::WriteRepository,
        TokenScope::WriteOrganization,
        TokenScope::WriteUser,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TokenScope::ReadRepository => "read:repository",
            TokenScope::WriteRepository => "write:repository",
            TokenScope::ReadOrganization => "read:organization",
            TokenScope::WriteOrganization => "write:organization",
            TokenScope::ReadUser => "read:user",
            TokenScope::WriteUser => "write:user",
        }
    }
}

impl fmt::Display for TokenScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who can see a repository the app creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Public,
    Private,
}

/// The Door43 server a client talks to: an origin with no path, credentials, query or
/// fragment. Plain `http` is accepted only for a loopback host, so a token never crosses
/// a network unencrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerUrl(Url);

impl ServerUrl {
    pub const PRODUCTION: &'static str = "https://git.door43.org";
    /// Door43's QA server, reset weekly. tC4 sends account and write calls there in
    /// development builds.
    pub const QA: &'static str = "https://qa.door43.org";

    pub fn production() -> Self {
        Self::parse_known(Self::PRODUCTION)
    }

    pub fn qa() -> Self {
        Self::parse_known(Self::QA)
    }

    fn parse_known(s: &str) -> Self {
        match s.parse() {
            Ok(url) => url,
            // Both constants are fixed https origins that the parser accepts; the unit
            // test `known_servers_parse` proves it.
            Err(_) => unreachable!("built-in server address {s} must parse"),
        }
    }

    pub fn as_url(&self) -> &Url {
        &self.0
    }

    pub(crate) fn is_loopback(&self) -> bool {
        is_loopback_host(&self.0)
    }

    /// Whether `url` has this server's scheme, host and port and carries no credentials.
    pub fn is_same_origin(&self, url: &Url) -> bool {
        url.origin() == self.0.origin() && url.username().is_empty() && url.password().is_none()
    }

    /// This server with the given path segments, each percent-encoded as needed.
    pub(crate) fn url_with_path<'a>(&self, segments: impl IntoIterator<Item = &'a str>) -> Url {
        let mut url = self.0.clone();
        // Only a cannot-be-a-base URL has no path segments, and the parser admits only
        // http and https, which always have them.
        if let Ok(mut path) = url.path_segments_mut() {
            path.clear().extend(segments);
        }
        url
    }
}

fn is_loopback_host(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        None => false,
    }
}

impl FromStr for ServerUrl {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let refuse = |reason| ParseError::InvalidServerUrl {
            value: s.to_owned(),
            reason,
        };
        let url = Url::parse(s).map_err(|_| refuse("not an absolute URL"))?;
        if url.host().is_none() {
            return Err(refuse("no host"));
        }
        match url.scheme() {
            "https" => {}
            "http" if is_loopback_host(&url) => {}
            "http" => return Err(refuse("plain http is allowed only for a loopback host")),
            _ => return Err(refuse("the scheme must be https")),
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(refuse("it must not carry a user name or password"));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(refuse("it must not carry a query or fragment"));
        }
        if url.path() != "/" {
            return Err(refuse("it must be an origin with no path"));
        }
        Ok(Self(url))
    }
}

impl fmt::Display for ServerUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str().trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_door43_names_parse() {
        for owner in [
            "unfoldingWord",
            "es-419_gl",
            "uW",
            "translationCore-Create-BCS",
        ] {
            assert!(owner.parse::<AccountName>().is_ok(), "{owner}");
        }
        for repo in ["en_tn", "el-x-koine_ugnt", "en_obs-tn", "hbo_uhb", "ar_avd"] {
            assert!(repo.parse::<RepoName>().is_ok(), "{repo}");
        }
    }

    #[test]
    fn names_that_could_change_a_route_are_refused() {
        for bad in [
            "", "-x", "x-", ".", "..", "a/b", "a b", "a?b", "a%2Fb", "é", "a\nb",
        ] {
            assert!(bad.parse::<AccountName>().is_err(), "{bad:?}");
            assert!(bad.parse::<RepoName>().is_err(), "{bad:?}");
        }
        assert!("project.git".parse::<RepoName>().is_err());
        assert!("project.WIKI".parse::<RepoName>().is_err());
        assert!("x".repeat(101).parse::<AccountName>().is_err());
    }

    #[test]
    fn real_release_tags_parse_and_unsafe_ones_do_not() {
        for tag in [
            "v86",
            "v0.34",
            "v3.0.0",
            "v60.1",
            "v0.0.2",
            "v2.0",
            "release/v1",
        ] {
            assert!(tag.parse::<TagName>().is_ok(), "{tag}");
        }
        for bad in [
            "", "../x", "a/../b", "/v1", "v1/", "a//b", ".hidden", "v1.lock", "v 1", "v1?", "v1#x",
            "-v1", "v1.",
        ] {
            assert!(bad.parse::<TagName>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn commit_shas_are_full_and_lowercased() {
        let sha: CommitSha = "C354B8AE66A23C485BF6F38FD35BD8F7EF81E4E5".parse().unwrap();
        assert_eq!(sha.as_str(), "c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5");
        assert!("c354b8ae".parse::<CommitSha>().is_err());
        assert!(
            "g354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5"
                .parse::<CommitSha>()
                .is_err()
        );
    }

    #[test]
    fn known_servers_parse() {
        assert_eq!(
            ServerUrl::production().to_string(),
            "https://git.door43.org"
        );
        assert_eq!(ServerUrl::qa().to_string(), "https://qa.door43.org");
    }

    #[test]
    fn server_urls_that_could_leak_credentials_are_refused() {
        for bad in [
            "http://git.door43.org",
            "https://user:pass@git.door43.org",
            "https://git.door43.org/sub",
            "https://git.door43.org/?token=x",
            "ftp://git.door43.org",
            "git.door43.org",
        ] {
            assert!(bad.parse::<ServerUrl>().is_err(), "{bad}");
        }
        for ok in [
            "http://127.0.0.1:3000",
            "http://localhost:8080/",
            "http://[::1]:9",
            "https://qa.door43.org/",
        ] {
            assert!(ok.parse::<ServerUrl>().is_ok(), "{ok}");
        }
    }

    #[test]
    fn same_origin_requires_scheme_host_port_and_no_credentials() {
        let server: ServerUrl = "https://git.door43.org".parse().unwrap();
        let ok = Url::parse("https://git.door43.org/me/project.git").unwrap();
        assert!(server.is_same_origin(&ok));
        for bad in [
            "http://git.door43.org/me/project.git",
            "https://git.door43.org:444/me/project.git",
            "https://evil.example/me/project.git",
            "https://me:t@git.door43.org/me/project.git",
        ] {
            assert!(!server.is_same_origin(&Url::parse(bad).unwrap()), "{bad}");
        }
    }

    #[test]
    fn serde_round_trips_through_the_parser() {
        let repo: RepoRef = "unfoldingWord/en_tn".parse().unwrap();
        let json = serde_json::to_string(&repo).unwrap();
        assert_eq!(json, r#"{"owner":"unfoldingWord","name":"en_tn"}"#);
        assert_eq!(serde_json::from_str::<RepoRef>(&json).unwrap(), repo);
        assert!(serde_json::from_str::<CommitSha>(r#""abc""#).is_err());
    }
}
