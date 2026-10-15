//! Answer bodies in the swagger's shapes (`User`, `Organization`, `Repository`, `Tag`,
//! `Release`, `Commit`, `CatalogEntry`, `CatalogStage`).

use crate::git::{CommitInfo, TagInfo};
use crate::{ReleaseKind, ReleaseRec, RepoRec};
use serde_json::{Value, json};

/// RFC 3339 in UTC for a unix time (Howard Hinnant's civil-from-days).
pub(crate) fn rfc3339(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

pub(crate) fn user(base: &str, id: u64, login: &str, email: &str) -> Value {
    json!({
        "id": id,
        "login": login,
        "login_name": "",
        "source_id": 0,
        "full_name": login,
        "email": email,
        "avatar_url": format!("{base}/avatars/{id}"),
        "html_url": format!("{base}/{login}"),
        "language": "en-US",
        "is_admin": false,
        "last_login": rfc3339(crate::CLOCK_START),
        "created": rfc3339(crate::CLOCK_START),
        "restricted": false,
        "active": true,
        "prohibit_login": false,
        "location": "",
        "website": "",
        "description": "",
        "visibility": "public",
        "followers_count": 0,
        "following_count": 0,
        "starred_repos_count": 0,
        "repo_languages": [],
        "repo_subjects": [],
        "repo_metadata_types": [],
    })
}

pub(crate) fn organization(base: &str, id: u64, name: &str, full_name: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "username": name,
        "full_name": full_name,
        "email": "",
        "avatar_url": format!("{base}/avatars/{id}"),
        "description": "",
        "website": "",
        "location": "",
        "visibility": "public",
        "repo_admin_change_team_access": false,
        "repo_languages": [],
        "repo_subjects": [],
        "repo_metadata_types": [],
    })
}

/// A commit a stage points at: a release's tag, or the default branch.
pub(crate) struct StagePoint {
    pub ref_name: String,
    pub commit: String,
    pub unix: i64,
    pub release_id: Option<u64>,
}

pub(crate) fn stage(base: &str, repo: &RepoRec, point: &StagePoint) -> Value {
    let full = format!("{}/{}", repo.owner, repo.name);
    json!({
        "branch_or_tag_name": point.ref_name,
        "release_url": point.release_id.map(|id| format!("{base}/api/v1/repos/{full}/releases/{id}")),
        "commit_sha": point.commit,
        "released": rfc3339(point.unix),
        "zipball_url": format!("{base}/{full}/archive/{}.zip", point.ref_name),
        "tarball_url": format!("{base}/{full}/archive/{}.tar.gz", point.ref_name),
        "git_trees_url": format!("{base}/api/v1/repos/{full}/git/trees/{}?recursive=1&per_page=99999", point.ref_name),
        "contents_url": format!("{base}/api/v1/repos/{full}/contents?ref={}", point.ref_name),
    })
}

pub(crate) struct RepoFacts<'a> {
    pub owner: Value,
    pub empty: bool,
    pub can_push: bool,
    pub prod: Option<&'a StagePoint>,
    pub preprod: Option<&'a StagePoint>,
    pub latest: Option<&'a StagePoint>,
    pub metadata: Option<&'a Value>,
}

pub(crate) fn repository(base: &str, repo: &RepoRec, facts: &RepoFacts<'_>) -> Value {
    let full = format!("{}/{}", repo.owner, repo.name);
    let stage_of = |point: Option<&StagePoint>| point.map(|p| stage(base, repo, p));
    let meta = MetadataFacts::of(facts.metadata, &repo.name, &repo.language);
    json!({
        "id": repo.id,
        "owner": facts.owner,
        "name": repo.name,
        "full_name": full,
        "description": repo.description,
        "empty": facts.empty,
        "private": repo.private,
        "fork": false,
        "template": false,
        "parent": null,
        "mirror": false,
        "size": 0,
        "language": meta.language,
        "languages_url": format!("{base}/api/v1/repos/{full}/languages"),
        "html_url": format!("{base}/{full}"),
        "url": format!("{base}/api/v1/repos/{full}"),
        "link": "",
        "ssh_url": "",
        "clone_url": format!("{base}/{full}.git"),
        "original_url": "",
        "website": "",
        "stars_count": 0,
        "forks_count": 0,
        "watchers_count": 0,
        "open_issues_count": 0,
        "open_pr_counter": 0,
        "release_counter": repo.releases.len(),
        "default_branch": repo.default_branch,
        "archived": false,
        "created_at": rfc3339(repo.created_unix),
        "updated_at": rfc3339(repo.created_unix),
        "archived_at": rfc3339(0),
        "permissions": { "admin": facts.can_push, "push": facts.can_push, "pull": true },
        "has_issues": true,
        "has_wiki": false,
        "has_pull_requests": true,
        "has_projects": false,
        "projects_mode": "all",
        "external_tracker": null,
        "external_wiki": null,
        "internal_tracker": null,
        "has_releases": true,
        "has_packages": false,
        "has_actions": false,
        "ignore_whitespace_conflicts": false,
        "allow_merge_commits": true,
        "allow_rebase": true,
        "allow_rebase_explicit": true,
        "allow_squash_merge": true,
        "allow_fast_forward_only_merge": false,
        "allow_rebase_update": true,
        "default_delete_branch_after_merge": false,
        "default_merge_style": "merge",
        "default_allow_maintainer_edit": false,
        "avatar_url": "",
        "internal": false,
        "mirror_interval": "",
        "object_format_name": "sha1",
        "mirror_updated": rfc3339(0),
        "repo_transfer": null,
        "topics": repo.topics,
        "licenses": [],
        "title": meta.title,
        "subject": repo.subject,
        "abbreviation": meta.abbreviation,
        "language_title": meta.language_title,
        "language_direction": meta.language_direction,
        "language_is_gl": false,
        "metadata_type": meta.metadata_type,
        "metadata_version": meta.metadata_version,
        "flavor_type": meta.flavor_type,
        "flavor": meta.flavor,
        "content_format": "",
        "checking_level": 3,
        "ingredients": meta.ingredients,
        "catalog": {
            "prod": stage_of(facts.prod),
            "preprod": stage_of(facts.preprod),
            "latest": stage_of(facts.latest),
        },
        "healthcheck_severity": "",
        "healthcheck_url": "",
    })
}

pub(crate) fn tag(base: &str, repo: &RepoRec, tag: &TagInfo, commit_unix: i64) -> Value {
    let full = format!("{}/{}", repo.owner, repo.name);
    json!({
        "name": tag.name,
        "message": "",
        "id": tag.object,
        "commit": {
            "url": format!("{base}/api/v1/repos/{full}/git/commits/{}", tag.commit),
            "sha": tag.commit,
            "created": rfc3339(commit_unix),
        },
        "zipball_url": format!("{base}/{full}/archive/{}.zip", tag.name),
        "tarball_url": format!("{base}/{full}/archive/{}.tar.gz", tag.name),
    })
}

pub(crate) fn release(base: &str, repo: &RepoRec, release: &ReleaseRec, author: Value) -> Value {
    let full = format!("{}/{}", repo.owner, repo.name);
    json!({
        "id": release.id,
        "tag_name": release.tag,
        "target_commitish": repo.default_branch,
        "name": release.tag,
        "body": "",
        "url": format!("{base}/api/v1/repos/{full}/releases/{}", release.id),
        "html_url": format!("{base}/{full}/releases/tag/{}", release.tag),
        "tarball_url": format!("{base}/{full}/archive/{}.tar.gz", release.tag),
        "zipball_url": format!("{base}/{full}/archive/{}.zip", release.tag),
        "upload_url": format!("{base}/api/v1/repos/{full}/releases/{}/assets", release.id),
        "draft": release.kind == ReleaseKind::Draft,
        "prerelease": release.kind == ReleaseKind::PreRelease,
        "created_at": rfc3339(release.created_unix),
        "published_at": rfc3339(release.created_unix),
        "author": author,
        "assets": [],
        "door43_metadata": null,
    })
}

pub(crate) fn commit(base: &str, repo: &RepoRec, info: &CommitInfo) -> Value {
    let full = format!("{}/{}", repo.owner, repo.name);
    let commit_url = |sha: &str| format!("{base}/api/v1/repos/{full}/git/commits/{sha}");
    json!({
        "url": commit_url(&info.sha),
        "sha": info.sha,
        "created": info.committer_date,
        "html_url": format!("{base}/{full}/commit/{}", info.sha),
        "commit": {
            "url": commit_url(&info.sha),
            "author": { "name": info.author_name, "email": info.author_email, "date": info.author_date },
            "committer": { "name": info.committer_name, "email": info.committer_email, "date": info.committer_date },
            "message": info.message,
            "tree": {
                "url": format!("{base}/api/v1/repos/{full}/git/trees/{}", info.tree),
                "sha": info.tree,
                "created": info.committer_date,
            },
            "verification": {
                "verified": false,
                "reason": "gpg.error.not_signed_commit",
                "signature": "",
                "signer": null,
                "payload": "",
            },
        },
        "author": null,
        "committer": null,
        "parents": info.parents.iter().map(|p| json!({
            "url": commit_url(p),
            "sha": p,
            "created": info.committer_date,
        })).collect::<Vec<_>>(),
        "files": null,
        "stats": null,
    })
}

/// What DCS derives from a burrito's `metadata.json` for its catalog fields.
pub(crate) struct MetadataFacts {
    pub title: String,
    pub abbreviation: String,
    pub language: String,
    pub language_title: String,
    pub language_direction: String,
    pub metadata_type: String,
    pub metadata_version: String,
    pub flavor_type: String,
    pub flavor: String,
    pub books: Vec<String>,
    pub ingredients: Vec<Value>,
}

impl MetadataFacts {
    pub fn of(metadata: Option<&Value>, repo_name: &str, language: &str) -> Self {
        let text = |pointer: &str| {
            metadata
                .and_then(|m| m.pointer(pointer))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let first_localized = |pointer: &str| {
            metadata
                .and_then(|m| m.pointer(pointer))
                .and_then(Value::as_object)
                .and_then(|o| o.values().next())
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let books: Vec<String> = metadata
            .and_then(|m| m.pointer("/type/flavorType/currentScope"))
            .and_then(Value::as_object)
            .map(|scope| scope.keys().map(|k| k.to_lowercase()).collect())
            .unwrap_or_default();
        let ingredients: Vec<Value> = metadata
            .and_then(|m| m.get("ingredients"))
            .and_then(Value::as_object)
            .map(|table| {
                table
                    .iter()
                    .enumerate()
                    .map(|(sort, (path, entry))| {
                        let identifier = entry
                            .get("scope")
                            .and_then(Value::as_object)
                            .and_then(|s| s.keys().next())
                            .map(|k| k.to_lowercase())
                            .unwrap_or_default();
                        json!({
                            "identifier": identifier,
                            "path": format!("./{path}"),
                            "title": "",
                            "sort": sort,
                            "versification": "",
                            "categories": null,
                            "size": entry.get("size").cloned().unwrap_or(Value::Null),
                            "alignment_count": 0,
                            "exists": true,
                            "is_dir": false,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            title: first_localized("/identification/name").unwrap_or_else(|| repo_name.to_owned()),
            abbreviation: first_localized("/identification/abbreviation")
                .map(|a| a.to_lowercase())
                .unwrap_or_default(),
            language: text("/languages/0/tag").unwrap_or_else(|| language.to_owned()),
            language_title: first_localized("/languages/0/name").unwrap_or_default(),
            language_direction: text("/languages/0/scriptDirection")
                .unwrap_or_else(|| "ltr".into()),
            metadata_type: if metadata.is_some() { "sb" } else { "" }.to_owned(),
            metadata_version: text("/meta/version").unwrap_or_default(),
            flavor_type: text("/type/flavorType/name").unwrap_or_default(),
            flavor: text("/type/flavorType/flavor/name").unwrap_or_default(),
            books,
            ingredients,
        }
    }
}

pub(crate) struct EntryFacts<'a> {
    pub point: &'a StagePoint,
    pub stage: &'a str,
    pub is_tag: bool,
    pub release: Value,
    pub repo: Value,
    pub metadata: Option<&'a Value>,
}

pub(crate) fn catalog_entry(base: &str, repo: &RepoRec, facts: &EntryFacts<'_>) -> Value {
    let full = format!("{}/{}", repo.owner, repo.name);
    let meta = MetadataFacts::of(facts.metadata, &repo.name, &repo.language);
    let r = &facts.point.ref_name;
    json!({
        "id": facts.point.release_id.unwrap_or(repo.id),
        "url": format!("{base}/api/v1/catalog/entry/{full}/{r}"),
        "name": repo.name,
        "owner": repo.owner,
        "full_name": full,
        "title": meta.title,
        "subject": repo.subject,
        "abbreviation": meta.abbreviation,
        "language": meta.language,
        "language_title": meta.language_title,
        "language_direction": meta.language_direction,
        "language_is_gl": false,
        "stage": facts.stage,
        "ref_type": if facts.is_tag { "tag" } else { "branch" },
        "branch_or_tag_name": r,
        "commit_sha": facts.point.commit,
        "released": rfc3339(facts.point.unix),
        "zipball_url": format!("{base}/{full}/archive/{r}.zip"),
        "tarbar_url": format!("{base}/{full}/archive/{r}.tar.gz"),
        "contents_url": format!("{base}/api/v1/repos/{full}/contents?ref={r}"),
        "git_trees_url": format!("{base}/api/v1/repos/{full}/git/trees/{r}?recursive=1&per_page=99999"),
        "metadata_url": format!("{base}/api/v1/catalog/metadata/{full}/{r}"),
        "metadata_json_url": format!("{base}/{full}/raw/{}/{r}/metadata.json", if facts.is_tag { "tag" } else { "branch" }),
        "metadata_api_contents_url": format!("{base}/api/v1/repos/{full}/contents/metadata.json?ref={r}"),
        "metadata_type": meta.metadata_type,
        "metadata_version": meta.metadata_version,
        "content_format": "",
        "flavor_type": meta.flavor_type,
        "flavor": meta.flavor,
        "books": meta.books,
        "ingredients": meta.ingredients,
        "is_valid": true,
        "validation_errors_url": format!("{base}/api/v1/catalog/validation/{full}/{r}"),
        "release": facts.release,
        "repo": facts.repo,
    })
}

#[cfg(test)]
mod tests {
    use super::rfc3339;

    #[test]
    fn rfc3339_matches_known_instants() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(crate::CLOCK_START), "2026-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_785_769_555), "2026-08-03T15:05:55Z");
    }
}
