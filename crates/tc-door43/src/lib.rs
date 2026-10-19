//! Door43 (DCS, a Gitea fork) HTTP client, and the app's session internet switch.
//!
//! This is one of the two crates allowed to use the network (the other is
//! `tc-resources`). Everything it sends goes through [`Door43Client`], which refuses
//! every request while the [`InternetSwitch`] is off. The switch starts off in each
//! session and is never stored (decision D95).
//!
//! What the client does, and the Door43 routes it uses:
//!
//! | Task | Route |
//! |---|---|
//! | Catalog search | `GET /api/v1/catalog/search` |
//! | Repository info, existence | `GET /api/v1/repos/{owner}/{repo}` |
//! | Tags; a tag's commit | `GET /api/v1/repos/{owner}/{repo}/tags[/{tag}]` |
//! | Releases; the latest | `GET /api/v1/repos/{owner}/{repo}/releases[/latest]` |
//! | Archive at a commit | `GET /api/v1/repos/{owner}/{repo}/git/commits/{sha}`, then `.../archive/{sha}.zip` |
//! | Burrito export of a tag | `GET /{owner}/{repo}/sb/{tag}.zip` (a web route, not in the swagger) |
//! | Mint a token | `GET /api/v1/user`, `GET`/`DELETE`/`POST /api/v1/users/{login}/tokens` with Basic auth |
//! | Organizations and permission | `GET /api/v1/user/orgs`, `GET /api/v1/users/{login}/orgs/{org}/permissions` |
//! | Create a repository | `POST /api/v1/user/repos` or `POST /api/v1/orgs/{org}/repos` |
//! | Read a refused push | [`refused_push`], from what libgit2 reports of `POST .../git-receive-pack` |
//!
//! Request and answer shapes follow the DCS swagger (`1.23.7+dcs`). Where Door43's prose
//! docs disagree with it, the swagger and tC4's verified behavior win: the new token's
//! secret is in `sha1`, not `token`; creating a repository needs `write:user` or
//! `write:organization`, not the read scopes; repositories are created with
//! `auto_init: false`; the notes subject is `TSV Translation Notes`.
//!
//! Names Door43 reports are read by Gitea's rules, so any repository or tag it lists can be
//! named; names the app creates follow tC4's stricter rule ([`NewRepoName`]). Account and
//! repository names compare without regard to case, as Door43 resolves them. A listing
//! (catalog search, tags, releases) sets aside an entry it cannot read in
//! [`Listing::skipped`] instead of failing whole.
//!
//! Both network clients (this one and `tc-resources`' `AquiferClient`) are built by
//! [`Http`]: https only, no proxy for a server on this computer, and the system's proxy
//! for every other, asked for only while the internet switch is on ([`http`]).
//!
//! Downloads stream to a file in a folder the caller names, report their bytes and can be
//! cancelled between chunks ([`download`]); they are verified before they are returned
//! ([`verify`]). An error message never holds a secret the request carried, whole or in
//! part.

mod catalog;
mod client;
pub mod download;
mod error;
mod git_tree;
pub mod http;
mod internet;
mod listing;
mod names;
mod push;
mod repos;
mod secret;
mod unpack;
pub mod verify;
#[cfg(windows)]
mod winproxy;

pub use catalog::{CatalogEntry, CatalogQuery, CatalogRef, Ingredient, Stage};
pub use client::{Door43Client, Limits};
pub use download::{DownloadTo, DownloadWatch, DownloadedZip, Unwatched};
pub use error::{AuthStatus, Door43Error, UnreachableCause, VerificationError};
pub use git_tree::ObjectId;
pub use http::{Http, HttpSpec, ProxyAnswer, ProxyFinder, system_proxy_finder};
pub use internet::{InternetSwitch, Offline};
pub use listing::{Listing, Skipped};
pub use names::{
    AccountName, GitOid, NewRepoName, ParseError, RepoName, RepoRef, ServerUrl, TagName, TokenName,
    TokenScope, Visibility,
};
pub use push::refused_push;
pub use repos::{
    CreatedRepo, NewRepoOwner, Organization, Release, ReleaseState, RepoInfo, StageRelease,
    StageReleases, Tag,
};
pub use secret::{Credentials, OneTimeCode, Password, Session, Token};
pub use unpack::{FileMode, NameProblem, UnpackedFile, check_names};
pub use verify::{VerifiedArchive, VerifiedSbExport, verify_archive, verify_sb_export};
#[cfg(windows)]
pub use winproxy::WinHttpFinder;
