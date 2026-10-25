//! Repositories, tags, releases and organizations, as Door43 answers them, parsed into
//! the types this crate hands out.

use crate::listing::text;
use crate::names::{AccountName, GitOid, RepoRef, ServerUrl, TagName, Visibility};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use url::Url;

/// Read `null` as the type's default. Gitea writes `null` for some empty fields.
pub(crate) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// A repository as `GET /repos/{owner}/{repo}` answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    /// With the casing Door43 reports, which can differ from the request's.
    pub repo: RepoRef,
    pub default_branch: String,
    /// True for a repository with no commits yet.
    pub empty: bool,
    pub visibility: Visibility,
    /// Both addresses were checked to be on the client's server with no credentials in
    /// them, so the clone URL is safe to send credentials to.
    pub html_url: Url,
    pub clone_url: Url,
    pub title: String,
    pub subject: String,
    pub language: String,
    pub topics: Vec<String>,
    /// DCS's newest release per stage: the one-call answer to "latest release and sha".
    pub catalog: StageReleases,
    /// The signed-in account may push (Gitea's `permissions.push`). Read anonymously, it is
    /// false. A sync asks before it fetches, so an account that cannot push changes nothing
    /// (Q8).
    pub can_push: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageReleases {
    pub prod: Option<StageRelease>,
    pub preprod: Option<StageRelease>,
    pub latest: Option<StageRelease>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRelease {
    /// A tag for `prod` and `preprod`; `latest` may name a branch.
    pub ref_name: String,
    pub commit: GitOid,
    pub released: Option<String>,
}

/// A tag and the commit it points at (annotated tags are dereferenced by Door43).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: TagName,
    pub commit: GitOid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseState {
    Draft,
    PreRelease,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub id: u64,
    pub tag: TagName,
    pub title: String,
    pub state: ReleaseState,
    pub created_at: String,
    pub published_at: Option<String>,
}

/// An organization the signed-in user belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organization {
    pub name: AccountName,
    pub full_name: String,
    /// Organizations where this is false are shown but cannot be chosen.
    pub can_create_repository: bool,
}

/// Where to create a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewRepoOwner {
    SignedInUser,
    Organization(AccountName),
}

/// A repository Door43 created. Both addresses were checked to be on the configured
/// server, so the clone URL is safe to send credentials to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedRepo {
    pub repo: RepoRef,
    pub html_url: Url,
    pub clone_url: Url,
}

#[derive(Deserialize)]
pub(crate) struct RawUser {
    #[serde(default, deserialize_with = "nullable")]
    pub login: String,
}

#[derive(Deserialize)]
pub(crate) struct RawRepository {
    #[serde(default)]
    owner: Option<RawUser>,
    #[serde(default, deserialize_with = "nullable")]
    name: String,
    #[serde(default, deserialize_with = "nullable")]
    full_name: String,
    #[serde(default, deserialize_with = "nullable")]
    default_branch: String,
    #[serde(default, deserialize_with = "nullable")]
    empty: bool,
    #[serde(default, deserialize_with = "nullable")]
    private: bool,
    #[serde(default, deserialize_with = "nullable")]
    html_url: String,
    #[serde(default, deserialize_with = "nullable")]
    clone_url: String,
    #[serde(default, deserialize_with = "nullable")]
    title: String,
    #[serde(default, deserialize_with = "nullable")]
    subject: String,
    #[serde(default, deserialize_with = "nullable")]
    language: String,
    #[serde(default, deserialize_with = "nullable")]
    topics: Vec<String>,
    #[serde(default)]
    catalog: Option<RawStages>,
    #[serde(default)]
    permissions: Option<RawPermissions>,
}

#[derive(Deserialize)]
struct RawPermissions {
    #[serde(default, deserialize_with = "nullable")]
    push: bool,
}

#[derive(Deserialize)]
struct RawStages {
    #[serde(default)]
    prod: Option<RawStage>,
    #[serde(default)]
    preprod: Option<RawStage>,
    #[serde(default)]
    latest: Option<RawStage>,
}

#[derive(Deserialize)]
struct RawStage {
    #[serde(default, deserialize_with = "nullable")]
    branch_or_tag_name: String,
    #[serde(default, deserialize_with = "nullable")]
    commit_sha: String,
    #[serde(default)]
    released: Option<String>,
}

impl RawStage {
    fn parse(self) -> Result<StageRelease, String> {
        Ok(StageRelease {
            commit: self.commit_sha.parse().map_err(|e| format!("{e}"))?,
            ref_name: self.branch_or_tag_name,
            released: self.released,
        })
    }
}

impl RawRepository {
    pub(crate) fn repo_ref(&self) -> Result<RepoRef, String> {
        let owner = match &self.owner {
            Some(owner) if !owner.login.is_empty() => owner.login.as_str(),
            _ => self.full_name.split('/').next().unwrap_or_default(),
        };
        Ok(RepoRef::new(
            owner.parse().map_err(|e| format!("{e}"))?,
            self.name.parse().map_err(|e| format!("{e}"))?,
        ))
    }

    /// Both addresses, refused unless they are on `server` with no credentials in them
    /// (tC4 `shareOperation.ts` `remoteAllowed`).
    fn addresses(&self, server: &ServerUrl) -> Result<(Url, Url), String> {
        let on_server = |raw: &str, what: &str| -> Result<Url, String> {
            let url = Url::parse(raw).map_err(|_| format!("the {what} is not a URL"))?;
            if server.is_same_origin(&url) {
                Ok(url)
            } else {
                Err(format!("the {what} is not on {server}"))
            }
        };
        Ok((
            on_server(&self.html_url, "page address")?,
            on_server(&self.clone_url, "clone address")?,
        ))
    }

    pub(crate) fn parse(self, server: &ServerUrl) -> Result<RepoInfo, String> {
        let repo = self.repo_ref()?;
        let (html_url, clone_url) = self.addresses(server)?;
        let stage = |s: Option<RawStage>| s.map(RawStage::parse).transpose();
        let catalog = match self.catalog {
            Some(stages) => StageReleases {
                prod: stage(stages.prod)?,
                preprod: stage(stages.preprod)?,
                latest: stage(stages.latest)?,
            },
            None => StageReleases::default(),
        };
        Ok(RepoInfo {
            repo,
            default_branch: self.default_branch,
            empty: self.empty,
            visibility: if self.private {
                Visibility::Private
            } else {
                Visibility::Public
            },
            html_url,
            clone_url,
            title: self.title,
            subject: self.subject,
            language: self.language,
            topics: self.topics,
            catalog,
            can_push: self.permissions.is_some_and(|p| p.push),
        })
    }

    /// A created repository, refused unless both addresses are on `server`.
    pub(crate) fn parse_created(self, server: &ServerUrl) -> Result<CreatedRepo, String> {
        let repo = self.repo_ref()?;
        let (html_url, clone_url) = self.addresses(server)?;
        Ok(CreatedRepo {
            repo,
            html_url,
            clone_url,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct RawTag {
    name: String,
    commit: Option<RawCommitMeta>,
}

#[derive(Deserialize)]
pub(crate) struct RawCommitMeta {
    #[serde(default, deserialize_with = "nullable")]
    pub sha: String,
}

impl RawTag {
    pub(crate) fn label(value: &Value) -> String {
        text(value, "name").to_owned()
    }

    pub(crate) fn parse(self) -> Result<Tag, String> {
        let name: TagName = self.name.parse().map_err(|e| format!("{e}"))?;
        let sha = self.commit.map(|c| c.sha).unwrap_or_default();
        Ok(Tag {
            commit: sha.parse().map_err(|e| format!("tag {name}: {e}"))?,
            name,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct RawRelease {
    id: u64,
    tag_name: String,
    #[serde(default, deserialize_with = "nullable")]
    name: String,
    #[serde(default, deserialize_with = "nullable")]
    draft: bool,
    #[serde(default, deserialize_with = "nullable")]
    prerelease: bool,
    #[serde(default, deserialize_with = "nullable")]
    created_at: String,
    #[serde(default)]
    published_at: Option<String>,
}

impl RawRelease {
    pub(crate) fn label(value: &Value) -> String {
        let id = value
            .get("id")
            .map_or_else(|| "?".to_owned(), Value::to_string);
        format!("release {id} ({})", text(value, "tag_name"))
    }

    pub(crate) fn parse(self) -> Result<Release, String> {
        let state = match (self.draft, self.prerelease) {
            (true, _) => ReleaseState::Draft,
            (false, true) => ReleaseState::PreRelease,
            (false, false) => ReleaseState::Published,
        };
        Ok(Release {
            id: self.id,
            tag: self.tag_name.parse().map_err(|e| format!("{e}"))?,
            title: self.name,
            state,
            created_at: self.created_at,
            published_at: self.published_at,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct RawOrganization {
    pub username: String,
    #[serde(default, deserialize_with = "nullable")]
    pub full_name: String,
}

#[derive(Deserialize)]
pub(crate) struct RawOrgPermissions {
    #[serde(default, deserialize_with = "nullable")]
    pub can_create_repository: bool,
}

#[derive(Deserialize)]
pub(crate) struct RawAccessToken {
    pub id: u64,
    #[serde(default, deserialize_with = "nullable")]
    pub name: String,
}

/// The create-token answer. The secret is in `sha1`, not `token` as Door43's docs say.
#[derive(Deserialize)]
pub(crate) struct RawCreatedToken {
    #[serde(default, deserialize_with = "nullable")]
    pub sha1: String,
}

#[derive(Deserialize)]
pub(crate) struct RawCommit {
    #[serde(default, deserialize_with = "nullable")]
    pub sha: String,
    pub commit: RawRepoCommit,
}

#[derive(Deserialize)]
pub(crate) struct RawRepoCommit {
    pub tree: RawCommitMeta,
}
