//! The DCS catalog: `GET /api/v1/catalog/search` (swagger `catalogSearch`).

use crate::names::{AccountName, CommitSha, RepoName, RepoRef, TagName};
use crate::repos::nullable;
use serde::Deserialize;

/// The stage filter. A stage includes the stages above it: `Preprod` lists production
/// and pre-production releases, `Latest` adds the default branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Prod,
    Preprod,
    Latest,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Prod => "prod",
            Stage::Preprod => "preprod",
            Stage::Latest => "latest",
        }
    }
}

/// A catalog search. Values within one field are ORed; fields are ANDed. Matching is
/// exact and ignores case. An empty field does not filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogQuery {
    /// Language tags, such as `en` or `es-419`.
    pub languages: Vec<String>,
    /// Subjects as the catalog spells them, such as `Aligned Bible` or
    /// `TSV Translation Notes` (not the docs' `Translation Notes`).
    pub subjects: Vec<String>,
    /// `None` leaves the server's default, which is `Prod`.
    pub stage: Option<Stage>,
    pub owners: Vec<AccountName>,
    pub repos: Vec<RepoName>,
    /// Repository topics, such as `tc-ready`.
    pub topics: Vec<String>,
    /// Every release, not only each repository's newest.
    pub include_history: bool,
    /// 1-based page; with `limit`. Without a limit the server returns every match.
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

impl CatalogQuery {
    pub(crate) fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        pairs.extend(self.languages.iter().map(|v| ("lang", v.clone())));
        pairs.extend(self.subjects.iter().map(|v| ("subject", v.clone())));
        pairs.extend(self.stage.map(|s| ("stage", s.as_str().to_owned())));
        pairs.extend(self.owners.iter().map(|v| ("owner", v.to_string())));
        pairs.extend(self.repos.iter().map(|v| ("repo", v.to_string())));
        pairs.extend(self.topics.iter().map(|v| ("topic", v.clone())));
        if self.include_history {
            pairs.push(("includeHistory", "true".to_owned()));
        }
        pairs.extend(self.page.map(|p| ("page", p.to_string())));
        pairs.extend(self.limit.map(|l| ("limit", l.to_string())));
        pairs
    }
}

/// The git ref a catalog entry was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogRef {
    Tag(TagName),
    Branch(String),
}

/// One released resource in the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub repo: RepoRef,
    pub title: String,
    pub subject: String,
    pub abbreviation: String,
    pub language: String,
    pub language_title: String,
    /// `ltr` or `rtl`, as DCS reports it.
    pub language_direction: String,
    pub language_is_gl: bool,
    pub stage: String,
    pub git_ref: CatalogRef,
    pub commit: CommitSha,
    pub released: Option<String>,
    /// `rc`, `sb`, `tc` or `ts`.
    pub metadata_type: String,
    pub metadata_version: String,
    pub flavor_type: String,
    pub flavor: String,
    pub content_format: String,
    pub books: Vec<String>,
    pub ingredients: Vec<Ingredient>,
    pub topics: Vec<String>,
}

/// An RC project or SB ingredient as DCS lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ingredient {
    #[serde(default, deserialize_with = "nullable")]
    pub identifier: String,
    #[serde(default, deserialize_with = "nullable")]
    pub path: String,
    #[serde(default, deserialize_with = "nullable")]
    pub title: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default, deserialize_with = "nullable")]
    pub categories: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct RawSearchResults {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub data: Vec<RawCatalogEntry>,
}

#[derive(Deserialize)]
pub(crate) struct RawCatalogEntry {
    owner: String,
    name: String,
    #[serde(default, deserialize_with = "nullable")]
    title: String,
    #[serde(default, deserialize_with = "nullable")]
    subject: String,
    #[serde(default, deserialize_with = "nullable")]
    abbreviation: String,
    #[serde(default, deserialize_with = "nullable")]
    language: String,
    #[serde(default, deserialize_with = "nullable")]
    language_title: String,
    #[serde(default, deserialize_with = "nullable")]
    language_direction: String,
    #[serde(default, deserialize_with = "nullable")]
    language_is_gl: bool,
    #[serde(default, deserialize_with = "nullable")]
    stage: String,
    ref_type: String,
    branch_or_tag_name: String,
    commit_sha: String,
    #[serde(default)]
    released: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    metadata_type: String,
    #[serde(default, deserialize_with = "nullable")]
    metadata_version: String,
    #[serde(default, deserialize_with = "nullable")]
    flavor_type: String,
    #[serde(default, deserialize_with = "nullable")]
    flavor: String,
    #[serde(default, deserialize_with = "nullable")]
    content_format: String,
    #[serde(default)]
    books: Option<Vec<String>>,
    #[serde(default)]
    ingredients: Option<Vec<Ingredient>>,
    #[serde(default)]
    repo: Option<RawTopics>,
}

#[derive(Deserialize)]
struct RawTopics {
    #[serde(default)]
    topics: Option<Vec<String>>,
}

impl RawCatalogEntry {
    pub(crate) fn parse(self) -> Result<CatalogEntry, String> {
        let repo = RepoRef::new(
            self.owner.parse().map_err(|e| format!("{e}"))?,
            self.name.parse().map_err(|e| format!("{e}"))?,
        );
        let git_ref = match self.ref_type.as_str() {
            "tag" => CatalogRef::Tag(
                self.branch_or_tag_name
                    .parse()
                    .map_err(|e| format!("{repo}: {e}"))?,
            ),
            "branch" => CatalogRef::Branch(self.branch_or_tag_name),
            other => return Err(format!("{repo}: unknown ref_type {other:?}")),
        };
        Ok(CatalogEntry {
            commit: self
                .commit_sha
                .parse()
                .map_err(|e| format!("{repo}: {e}"))?,
            repo,
            title: self.title,
            subject: self.subject,
            abbreviation: self.abbreviation,
            language: self.language,
            language_title: self.language_title,
            language_direction: self.language_direction,
            language_is_gl: self.language_is_gl,
            stage: self.stage,
            git_ref,
            released: self.released,
            metadata_type: self.metadata_type,
            metadata_version: self.metadata_version,
            flavor_type: self.flavor_type,
            flavor: self.flavor,
            content_format: self.content_format,
            books: self.books.unwrap_or_default(),
            ingredients: self.ingredients.unwrap_or_default(),
            topics: self.repo.and_then(|r| r.topics).unwrap_or_default(),
        })
    }
}
