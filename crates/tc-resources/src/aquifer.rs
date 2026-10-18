//! Aquifer helps from the BibleAquifer GitHub releases (research `aquifer.md`, R3, R4).
//!
//! Each BibleAquifer repository publishes, on date tags such as `v2026-10-12`, one
//! `<Language>-json.zip` asset per language: `metadata.json` and `json/*.content.json` at
//! the zip's root. A pin names the repository, the tag (a label), the commit the tag
//! named, the language and the asset's SHA-256. GitHub lets a maintainer re-upload an
//! asset or move a lightweight tag, so the digest is what binds the bytes: an asset is
//! installed only when its SHA-256 is the pin's. The REST API of `api.aquifer.bible` is
//! not used: it needs a key, has no pinning, and serves another database.
//!
//! Every request goes through [`AquiferClient`], which reads the app's internet switch
//! before it sends anything and between the chunks of a download, as tc-door43's client
//! does (D95). It follows redirects only to GitHub's own hosts, and only to the test
//! server's origin when it talks to one on loopback.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{ACCEPT, HeaderMap};
use serde::Deserialize;
use tc_core::BookId;
use tc_door43::{GitOid, InternetSwitch, TagName, UnreachableCause, check_names};
use tc_helps::HelpRef;
use tc_helps::aquifer::{
    AquiferHelpsKind, academy_from_aquifer, notes_from_aquifer, parse_aquifer_articles,
    parse_aquifer_metadata, questions_from_aquifer, words_from_aquifer,
};
use url::Url;

use crate::error::{AquiferError, InstallError, Refusal};
use crate::pin::{AquiferLanguage, AquiferPin, AquiferRepo, AssetName, BookCoverage, Sha256};
use crate::store::{Installed, Origin, ResourceStore, Staged};

const USER_AGENT: &str = concat!("tc-resources/", env!("CARGO_PKG_VERSION"));
const GITHUB: &str = "https://github.com";
const GITHUB_API: &str = "https://api.github.com";
/// Hosts a release download may be redirected to.
const ASSET_HOSTS: [&str; 4] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
];
const MAX_JSON_BYTES: u64 = 16 << 20;
const MAX_REDIRECTS: usize = 5;

/// Timeouts and size caps. The English notes asset is 16.2 MB zipped and about 150 MB
/// unzipped (`aquifer.md`, R6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferLimits {
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    /// 64 MiB by default.
    pub max_download_bytes: u64,
    /// 512 MiB by default.
    pub max_unpacked_bytes: u64,
}

impl Default for AquiferLimits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(20),
            read_timeout: Duration::from_secs(60),
            max_download_bytes: 64 << 20,
            max_unpacked_bytes: 512 << 20,
        }
    }
}

/// A release as GitHub lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferRelease {
    pub tag: TagName,
    pub published_at: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

impl AquiferRelease {
    pub fn asset(&self, name: &AssetName) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|a| a.name == *name)
    }

    /// The per-language JSON assets, the ones this crate installs.
    pub fn json_assets(&self) -> impl Iterator<Item = &ReleaseAsset> {
        self.assets.iter().filter(|a| a.name.is_json())
    }
}

/// One asset of a release. `sha256` is GitHub's digest of the asset; older releases
/// have none, and such an asset cannot be pinned without trusting its own bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: AssetName,
    pub size: u64,
    pub sha256: Option<Sha256>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
struct RawCommit {
    sha: String,
}

/// The one HTTP client for GitHub. See the module documentation.
#[derive(Clone)]
pub struct AquiferClient {
    http: reqwest::Client,
    web: Url,
    api: Url,
    internet: InternetSwitch,
    limits: AquiferLimits,
}

impl std::fmt::Debug for AquiferClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The reqwest client is left out: its Debug lists proxies, whose URLs can carry
        // credentials.
        f.debug_struct("AquiferClient")
            .field("web", &self.web.as_str())
            .field("api", &self.api.as_str())
            .field("internet_on", &self.internet.is_on())
            .finish_non_exhaustive()
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

impl AquiferClient {
    /// A client for github.com.
    pub fn new(internet: InternetSwitch) -> Result<Self, AquiferError> {
        let parse = |s: &str| {
            Url::parse(s).map_err(|e| AquiferError::Setup {
                reason: e.to_string(),
            })
        };
        Self::with_servers(
            parse(GITHUB)?,
            parse(GITHUB_API)?,
            internet,
            AquiferLimits::default(),
        )
    }

    /// A client for other servers: `web` serves release downloads and `api` the REST
    /// API. Plain `http` is accepted only on loopback, for tests.
    pub fn with_servers(
        web: Url,
        api: Url,
        internet: InternetSwitch,
        limits: AquiferLimits,
    ) -> Result<Self, AquiferError> {
        let loopback = is_loopback(&web) && is_loopback(&api);
        for url in [&web, &api] {
            let ok = url.scheme() == "https" || (url.scheme() == "http" && loopback);
            if !ok || url.host().is_none() || !url.username().is_empty() || url.password().is_some()
            {
                return Err(AquiferError::Setup {
                    reason: format!("{url} is not an https origin"),
                });
            }
        }
        let origins = [web.origin(), api.origin()];
        let redirects = reqwest::redirect::Policy::custom(move |attempt| {
            let url = attempt.url();
            // The API redirects a renamed repository within itself; a download goes to
            // GitHub's asset hosts. A test server's redirects stay on its own origin.
            let allowed = origins.contains(&url.origin())
                || (!loopback
                    && url.scheme() == "https"
                    && url
                        .host_str()
                        .is_some_and(|h| ASSET_HOSTS.iter().any(|a| h.eq_ignore_ascii_case(a))));
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if allowed {
                attempt.follow()
            } else {
                attempt.error("a redirect off GitHub")
            }
        });
        let builder = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(limits.connect_timeout)
            .read_timeout(limits.read_timeout)
            .redirect(redirects)
            .referer(false);
        let builder = if loopback {
            builder.no_proxy()
        } else {
            builder.https_only(true)
        };
        let http = builder.build().map_err(|e| AquiferError::Setup {
            reason: e.to_string(),
        })?;
        Ok(Self {
            http,
            web,
            api,
            internet,
            limits,
        })
    }

    pub fn internet(&self) -> &InternetSwitch {
        &self.internet
    }

    fn url(base: &Url, segments: &[&str]) -> Url {
        let mut url = base.clone();
        // An http(s) URL always has path segments.
        if let Ok(mut path) = url.path_segments_mut() {
            path.clear().extend(segments);
        }
        url
    }

    fn repo_api(&self, repo: &AquiferRepo, rest: &[&str]) -> Url {
        let mut segments = vec!["repos", repo.owner.as_str(), repo.name.as_str()];
        segments.extend(rest);
        Self::url(&self.api, &segments)
    }

    /// The only place a request leaves this module.
    async fn get(
        &self,
        url: Url,
        accept: &str,
    ) -> Result<(String, reqwest::Response), AquiferError> {
        let route = format!("GET {}", url.path());
        if self.internet.check().is_err() {
            return Err(AquiferError::Offline { route });
        }
        let response = self
            .http
            .get(url)
            .header(ACCEPT, accept)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| transport_error(&route, e))?;
        let status = response.status();
        if status.is_success() {
            return Ok((route, response));
        }
        Err(classify(route, status, response.headers()))
    }

    async fn body(
        &self,
        route: &str,
        mut response: reqwest::Response,
        limit: u64,
    ) -> Result<Vec<u8>, AquiferError> {
        let too_large = || AquiferError::TooLarge {
            route: route.to_owned(),
            limit,
        };
        if response.content_length().is_some_and(|len| len > limit) {
            return Err(too_large());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| transport_error(route, e))?
        {
            if self.internet.check().is_err() {
                return Err(AquiferError::Offline {
                    route: route.to_owned(),
                });
            }
            if (body.len() + chunk.len()) as u64 > limit {
                return Err(too_large());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    async fn json<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T, AquiferError> {
        let (route, response) = self.get(url, "application/vnd.github+json").await?;
        let bytes = self.body(&route, response, MAX_JSON_BYTES).await?;
        serde_json::from_slice(&bytes).map_err(|e| AquiferError::UnexpectedResponse {
            route,
            reason: e.to_string(),
        })
    }

    /// Downloads a pinned asset. The bytes are not checked here: [`install_aquifer`]
    /// checks them against the pin's digest before anything is kept.
    pub async fn download_asset(&self, pin: &AquiferPin) -> Result<Vec<u8>, AquiferError> {
        let mut segments = vec![
            pin.repo.owner.as_str(),
            pin.repo.name.as_str(),
            "releases",
            "download",
        ];
        segments.extend(pin.tag.as_str().split('/'));
        segments.push(pin.asset.as_str());
        let url = Self::url(&self.web, &segments);
        let (route, response) = self.get(url, "application/octet-stream").await?;
        self.body(&route, response, self.limits.max_download_bytes)
            .await
    }

    /// The newest published release (`GET /repos/{owner}/{repo}/releases/latest`).
    pub async fn latest_release(&self, repo: &AquiferRepo) -> Result<AquiferRelease, AquiferError> {
        let url = self.repo_api(repo, &["releases", "latest"]);
        let route = format!("GET {}", url.path());
        let raw: RawRelease = self.json(url).await?;
        if raw.draft || raw.prerelease {
            return Err(AquiferError::UnexpectedResponse {
                route,
                reason: "the latest release is a draft or a pre-release".to_owned(),
            });
        }
        parse_release(&route, raw)
    }

    /// The commit a tag names (`GET /repos/{owner}/{repo}/commits/{tag}`), which
    /// dereferences an annotated tag.
    pub async fn tag_commit(
        &self,
        repo: &AquiferRepo,
        tag: &TagName,
    ) -> Result<GitOid, AquiferError> {
        let mut rest = vec!["commits"];
        rest.extend(tag.as_str().split('/'));
        let url = self.repo_api(repo, &rest);
        let route = format!("GET {}", url.path());
        let raw: RawCommit = self.json(url).await?;
        raw.sha
            .parse()
            .map_err(|_| AquiferError::UnexpectedResponse {
                route,
                reason: format!("{:?} is not a commit sha", raw.sha),
            })
    }
}

fn parse_release(route: &str, raw: RawRelease) -> Result<AquiferRelease, AquiferError> {
    let unexpected = |reason: String| AquiferError::UnexpectedResponse {
        route: route.to_owned(),
        reason,
    };
    let tag = raw
        .tag_name
        .parse()
        .map_err(|_| unexpected(format!("{:?} is not a tag", raw.tag_name)))?;
    let mut assets = Vec::new();
    for asset in raw.assets {
        // An asset with a name this crate cannot use is simply not offered.
        let Ok(name) = asset.name.parse() else {
            continue;
        };
        let sha256 = match asset.digest {
            Some(digest) => Some(
                digest
                    .parse()
                    .map_err(|_| unexpected(format!("{digest:?} is not a SHA-256 digest")))?,
            ),
            None => None,
        };
        assets.push(ReleaseAsset {
            name,
            size: asset.size,
            sha256,
        });
    }
    Ok(AquiferRelease {
        tag,
        published_at: raw.published_at,
        assets,
    })
}

fn transport_error(route: &str, error: reqwest::Error) -> AquiferError {
    if error.is_redirect() {
        return AquiferError::UnexpectedResponse {
            route: route.to_owned(),
            reason: "GitHub redirected the request off GitHub".to_owned(),
        };
    }
    let cause = if error.is_timeout() {
        UnreachableCause::Timeout
    } else if error.is_connect() {
        UnreachableCause::Connect
    } else if error.is_body() || error.is_decode() {
        UnreachableCause::Interrupted
    } else {
        UnreachableCause::Other(error.without_url().to_string())
    };
    AquiferError::Unreachable {
        route: route.to_owned(),
        cause,
    }
}

fn classify(route: String, status: StatusCode, headers: &HeaderMap) -> AquiferError {
    let exhausted = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        == Some("0");
    match status.as_u16() {
        404 => AquiferError::NotFound { route },
        429 => AquiferError::RateLimited { route, status: 429 },
        403 if exhausted => AquiferError::RateLimited { route, status: 403 },
        code => AquiferError::Http {
            route,
            status: code,
        },
    }
}

/// Pins a release asset for a first install: the commit GitHub names for the tag, and the
/// digest GitHub lists for the asset. The download is then checked against that digest,
/// so the asset never certifies itself. Refused when the release lists no digest.
pub async fn pin_aquifer_release(
    client: &AquiferClient,
    repo: &AquiferRepo,
    release: &AquiferRelease,
    asset: &AssetName,
    language: AquiferLanguage,
) -> Result<AquiferPin, InstallError> {
    let listed = release.asset(asset).ok_or_else(|| AquiferError::NotFound {
        route: format!("{repo} {} {asset}", release.tag),
    })?;
    let sha256 = listed.sha256.ok_or_else(|| InstallError::Refused {
        what: format!("{repo} {} {asset}", release.tag),
        refusal: Refusal::Unreadable("GitHub lists no SHA-256 digest for the asset".to_owned()),
    })?;
    let commit = client.tag_commit(repo, &release.tag).await?;
    Ok(AquiferPin {
        repo: repo.clone(),
        tag: release.tag.clone(),
        commit,
        language,
        asset: asset.clone(),
        sha256,
    })
}

/// Installs the pinned asset, or returns the install the store already has without
/// touching the network.
pub async fn install_aquifer(
    store: &ResourceStore,
    client: &AquiferClient,
    pin: &AquiferPin,
) -> Result<Installed, InstallError> {
    if let Some(installed) = store.aquifer(pin)? {
        return Ok(installed);
    }
    let bytes = client.download_asset(pin).await?;
    install_aquifer_zip(store, pin, &bytes, client.limits.max_unpacked_bytes)
}

/// Installs an asset already at hand (from a USB drive, or by an installer building its
/// bundled store): checks its digest, unpacks it, and checks that tc-helps reads it as the
/// pinned resource and language before anything is kept.
pub fn install_aquifer_zip(
    store: &ResourceStore,
    pin: &AquiferPin,
    bytes: &[u8],
    max_unpacked: u64,
) -> Result<Installed, InstallError> {
    if let Some(installed) = store.aquifer(pin)? {
        return Ok(installed);
    }
    let refused = |refusal| InstallError::Refused {
        what: pin.to_string(),
        refusal,
    };
    let found = Sha256::of(bytes);
    if found != pin.sha256 {
        return Err(refused(Refusal::Sha256Mismatch {
            expected: pin.sha256,
            found,
        }));
    }
    let files = unpack(bytes, max_unpacked).map_err(refused)?;
    let check = check_release(pin, &files).map_err(refused)?;
    store.install(Staged {
        origin: Origin::Aquifer {
            pin: pin.clone(),
            refused_articles: check.refused_articles,
            book_files: check.book_files,
        },
        books: check.books,
        files,
    })
}

/// What reading a release found.
struct ReleaseCheck {
    books: Option<BookCoverage>,
    book_files: BTreeMap<BookId, Vec<String>>,
    refused_articles: usize,
}

/// Reads the release as tc-helps will: the metadata names the pinned resource and
/// language, every article file parses, and the converter for its kind turns the
/// articles into helps. For notes and questions it records which files hold each book's
/// items, by the passage the converter places each at: a release files some notes in
/// another book's file (`index_reference` `00000000`, see tc-helps' `aquifer` module),
/// so a file's name does not say which books it holds. A file that refused an article is
/// also listed under its own book, where the loader names the refusal.
fn check_release(pin: &AquiferPin, files: &[(String, Vec<u8>)]) -> Result<ReleaseCheck, Refusal> {
    let text = |path: &str| -> Result<&str, Refusal> {
        let (_, bytes) = files
            .iter()
            .find(|(p, _)| p == path)
            .ok_or_else(|| Refusal::NotAquifer(format!("there is no {path}")))?;
        std::str::from_utf8(bytes).map_err(|_| Refusal::NotAquifer(format!("{path} is not UTF-8")))
    };
    let metadata = parse_aquifer_metadata(text("metadata.json")?)
        .map_err(|e| Refusal::NotAquifer(e.to_string()))?;
    if !metadata
        .resource_code
        .eq_ignore_ascii_case(pin.repo.name.as_str())
    {
        return Err(Refusal::WrongRepository {
            expected: pin.repo.name.to_string(),
            found: metadata.resource_code,
        });
    }
    if metadata.language != pin.language.as_str() {
        return Err(Refusal::WrongLanguage {
            expected: pin.language.to_string(),
            found: metadata.language,
        });
    }
    let Some(kind) = metadata.kind() else {
        return Err(Refusal::NotAquifer(format!(
            "{} is not one of the unfoldingWord helps Aquifer mirrors",
            metadata.resource_code
        )));
    };
    let by_book = is_by_book(kind);
    let mut refused_articles = 0;
    let mut any_article = false;
    let mut converted = 0;
    let mut placed = BTreeSet::new();
    let mut book_files: BTreeMap<BookId, Vec<String>> = BTreeMap::new();
    for (path, _) in files.iter().filter(|(p, _)| is_article_file(p)) {
        let loaded = parse_aquifer_articles(text(path)?)
            .map_err(|e| Refusal::NotAquifer(format!("{path}: {e}")))?;
        any_article |= !loaded.items.is_empty();
        // Converted as the loader will, so an asset whose articles convert to nothing is
        // refused here rather than found empty later.
        let (items, refused, books) = match kind {
            AquiferHelpsKind::Notes => {
                let notes = notes_from_aquifer(&loaded.items);
                let books = notes.items.iter().filter_map(|n| bible_book(&n.reference));
                (notes.items.len(), notes.refused.len(), books.collect())
            }
            AquiferHelpsKind::Questions => {
                let questions = questions_from_aquifer(&loaded.items);
                let books = questions
                    .items
                    .iter()
                    .filter_map(|q| bible_book(&q.reference));
                (
                    questions.items.len(),
                    questions.refused.len(),
                    books.collect(),
                )
            }
            AquiferHelpsKind::Words => (
                words_from_aquifer(&loaded.items).articles.len(),
                0,
                BTreeSet::new(),
            ),
            AquiferHelpsKind::Academy => (
                academy_from_aquifer(&loaded.items).len(),
                0,
                BTreeSet::new(),
            ),
        };
        converted += items;
        let refused = refused + loaded.refused.len();
        refused_articles += refused;
        if by_book {
            placed.extend(books.iter().copied());
            let own = canonical_book(path).filter(|_| refused > 0);
            for book in books.into_iter().chain(own) {
                let list = book_files.entry(book).or_default();
                if !list.contains(path) {
                    list.push(path.clone());
                }
            }
        }
    }
    if !any_article {
        return Err(Refusal::NotAquifer("it holds no article".to_owned()));
    }
    if converted == 0 {
        return Err(Refusal::NotAquifer(
            "tc-helps converts none of its articles".to_owned(),
        ));
    }
    let books = if by_book {
        BookCoverage::books(placed)
    } else {
        Some(BookCoverage::WholeCollection)
    };
    Ok(ReleaseCheck {
        books,
        book_files,
        refused_articles,
    })
}

/// Whether the loader reads this kind one book at a time: notes and questions are
/// indexed by passage; words and academy are alphabetical and read whole.
pub(crate) fn is_by_book(kind: AquiferHelpsKind) -> bool {
    matches!(kind, AquiferHelpsKind::Notes | AquiferHelpsKind::Questions)
}

pub(crate) fn bible_book(reference: &HelpRef) -> Option<BookId> {
    match reference {
        HelpRef::Bible(r) => Some(r.book),
        HelpRef::Obs(_) => None,
    }
}

pub(crate) fn is_article_file(path: &str) -> bool {
    path.strip_prefix("json/")
        .is_some_and(|name| !name.contains('/') && name.ends_with(".content.json"))
}

/// `json/56.content.json` holds Titus: canonical files are numbered by canonical order,
/// 01 to 66.
pub(crate) fn canonical_book(path: &str) -> Option<BookId> {
    let number = path.strip_prefix("json/")?.strip_suffix(".content.json")?;
    if number.len() != 2 || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    BookId::from_ordinal(number.parse().ok()?)
}

/// Unpacks a release zip. Every entry is read in full, which checks its CRC-32; a name
/// that could leave the folder, a name Windows would write under another name, a symbolic
/// link or two names one file system would merge is refused, never repaired. macOS Finder
/// debris is dropped. When the root holds no `metadata.json` and every entry shares one top
/// folder, that folder is removed.
fn unpack(bytes: &[u8], max_unpacked: u64) -> Result<Vec<(String, Vec<u8>)>, Refusal> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Refusal::NotAZip(e.to_string()))?;
    let mut files = Vec::new();
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| Refusal::NotAZip(e.to_string()))?;
        let name = entry.name().replace('\\', "/");
        if name.starts_with('/')
            || name.contains(':')
            || name
                .split('/')
                .any(|s| !s.is_empty() && s.chars().all(|c| c == '.'))
        {
            return Err(Refusal::EscapesRoot(name));
        }
        if entry.is_dir() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(Refusal::Symlink(name));
        }
        let file_name = name.rsplit('/').next().unwrap_or(&name);
        if name.starts_with("__MACOSX/") || file_name == ".DS_Store" || file_name.starts_with("._")
        {
            continue;
        }
        if name.split('/').any(str::is_empty) {
            return Err(Refusal::EscapesRoot(name));
        }
        let mut body = Vec::new();
        let limit = max_unpacked.saturating_sub(total).saturating_add(1);
        (&mut entry)
            .take(limit)
            .read_to_end(&mut body)
            .map_err(|e| Refusal::CorruptEntry {
                path: name.clone(),
                reason: e.to_string(),
            })?;
        total += body.len() as u64;
        if total > max_unpacked {
            return Err(Refusal::TooLargeUnpacked {
                limit: max_unpacked,
            });
        }
        files.push((name, body));
    }
    if !files.iter().any(|(p, _)| p == "metadata.json") {
        let top = files
            .first()
            .and_then(|(p, _)| p.split_once('/'))
            .map(|(t, _)| format!("{t}/"));
        if let Some(top) = top
            && files.iter().all(|(p, _)| p.starts_with(&top))
        {
            for (path, _) in &mut files {
                *path = path[top.len()..].to_owned();
            }
        }
    }
    check_names(files.iter().map(|(path, _)| path.as_str()))?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn unpack_keeps_the_release_layout_and_unwraps_one_top_folder() {
        let flat = unpack(
            &zip(&[("json/001.content.json", b"[]"), ("metadata.json", b"{}")]),
            1 << 20,
        )
        .unwrap();
        assert_eq!(flat[1].0, "metadata.json");
        let wrapped = unpack(
            &zip(&[
                ("por/metadata.json", b"{}"),
                ("por/json/56.content.json", b"[]"),
            ]),
            1 << 20,
        )
        .unwrap();
        assert_eq!(wrapped[0].0, "metadata.json");
        assert_eq!(wrapped[1].0, "json/56.content.json");
        let debris = unpack(
            &zip(&[
                ("metadata.json", b"{}"),
                ("__MACOSX/._x", b""),
                ("json/.DS_Store", b""),
            ]),
            1 << 20,
        )
        .unwrap();
        assert_eq!(debris.len(), 1);
    }

    #[test]
    fn unpack_refuses_what_could_escape_merge_or_overflow() {
        for name in ["../x", "/x", "C:x", "a/../../x", "a\\..\\x", "...", "a//b"] {
            assert!(
                matches!(
                    unpack(&zip(&[(name, b"x")]), 1 << 20),
                    Err(Refusal::EscapesRoot(_))
                ),
                "{name}"
            );
        }
        for name in [
            "notes./x",
            "x/a ",
            "x/a.",
            "PROGRA~1.JSN",
            "x/abcdef~1.jsn",
            "A~9",
            "CON.json",
            "nul.json",
        ] {
            // With a root `metadata.json`, no top folder is removed before the names are
            // checked: a removed folder's name is never written.
            assert!(
                matches!(
                    unpack(&zip(&[("metadata.json", b"{}"), (name, b"x")]), 1 << 20),
                    Err(Refusal::NotPortable { .. })
                ),
                "{name}"
            );
        }
        for pair in [
            ["a.json", "A.json"],
            ["σ.json", "ς.json"],
            ["café.md", "cafe\u{301}.md"],
            ["Notes/a.json", "notes/b.json"],
            ["A", "a/x"],
        ] {
            assert!(
                matches!(
                    unpack(&zip(&[(pair[0], b"x"), (pair[1], b"y")]), 1 << 20),
                    Err(Refusal::DuplicateEntry(_))
                ),
                "{pair:?}"
            );
        }
        // `ı` stays apart from `i`, and only the shape of an 8.3 short name is refused.
        let names = [
            "notes~2024-draft.md",
            "notes~1.json",
            "abcdef~12x",
            "a~1.tar.gz",
            "ı",
            "i",
        ];
        let entries: Vec<(&str, &[u8])> = names.iter().map(|name| (*name, &b"x"[..])).collect();
        let allowed = unpack(&zip(&entries), 1 << 20).unwrap();
        assert_eq!(
            allowed
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            names
        );
        assert!(matches!(
            unpack(&zip(&[("a.json", &[0u8; 100])]), 10),
            Err(Refusal::TooLargeUnpacked { .. })
        ));
        assert!(matches!(unpack(b"not a zip", 10), Err(Refusal::NotAZip(_))));
        let whole = zip(&[("metadata.json", &[b'x'; 4096])]);
        assert!(unpack(&whole[..whole.len() / 2], 1 << 20).is_err());
    }

    #[test]
    fn canonical_files_name_their_book() {
        assert_eq!(canonical_book("json/56.content.json"), Some(BookId::Titus));
        assert_eq!(
            canonical_book("json/01.content.json"),
            Some(BookId::Genesis)
        );
        assert_eq!(canonical_book("json/001.content.json"), None);
        assert_eq!(canonical_book("json/67.content.json"), None);
        assert!(is_article_file("json/tit-1-1.content.json"));
        assert!(!is_article_file("md/56.content.md"));
    }
}
