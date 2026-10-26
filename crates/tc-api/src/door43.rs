//! Open a project from Door43 (M6, unit B9b; tC4 #677): the `door43/open` job and the rules of
//! its form, which the dialog and the server share so that the two cannot disagree about a
//! repository or a project name (`tC4:src/data/share/syncOperation.ts`, `openSourceOf`,
//! `openDefaultName`, `openRepoPathOf`, `repoPathInUse`).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{IdError, ProjectRef, RequestId};

/// `door43/open`: get a full copy of the repository and add it as the project `name`. The name
/// is checked against the projects on this computer first, without letter case, and a name in
/// use is refused (`open-door43.name-exists`) before anything is sent. The job ends with
/// `JobOutcome::Created` naming the new project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenDoor43 {
    pub request: RequestId,
    pub repo: RepoSource,
    pub name: ProjectName,
}

/// `door43/sync`: one sync of a shared project with its Door43 repository, the press of its
/// card's "Sync with Door43" (tC4 `syncRun`, #678). Only this press syncs: nothing syncs at
/// launch or while idle. The job ends with `JobOutcome::Synced`, or fails with
/// `Refused(Sync(..))`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncProject {
    pub request: RequestId,
    pub project: ProjectRef,
}

/// `door43/lastSync`: the project's last sync result on this computer since the app started,
/// so what a teammate's newer versions replaced stays readable after its toast (owner ruling
/// H5); `None` before the first sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LastSync {
    pub project: ProjectRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct LastSyncView {
    pub view: Option<crate::jobs::SyncResultView>,
}

/// A repository on the app's Door43 server: `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoSource {
    owner: String,
    name: String,
}

/// The longest owner or repository name a source carries; Gitea's own limit is 100.
const MAX_PART: usize = 100;

fn part_ok(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= MAX_PART
        && part != "."
        && part != ".."
        && !part
            .chars()
            .any(|c| c == '/' || c == ':' || c == '\\' || c.is_whitespace() || c.is_control())
}

impl RepoSource {
    pub fn new(owner: &str, name: &str) -> Result<Self, IdError> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        if !part_ok(owner) || !part_ok(name) {
            return Err(IdError {
                what: "Door43 repository",
                reason: "not owner/repository",
            });
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    /// What the dialog's repository box holds, read as tC4's `openSourceOf` reads it:
    /// `owner/repository`, or the address of a repository on `host`, the app's Door43 server
    /// (`signIn.server`'s host). A link to another server, one that carries a user name or a
    /// password, or one that is not exactly `/owner/repository` is no source.
    pub fn from_input(input: &str, host: &str) -> Option<Self> {
        let text = input.trim().trim_end_matches('/');
        if let Some((owner, name)) = text.split_once('/')
            && !text.contains("://")
            && !name.contains('/')
        {
            return Self::new(owner, name).ok();
        }
        let rest = text
            .strip_prefix("https://")
            .or_else(|| text.strip_prefix("http://"))?;
        let (authority, path) = rest.split_once('/')?;
        if authority.contains('@') || !authority.eq_ignore_ascii_case(host) {
            return None;
        }
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let (owner, name) = path.split_once('/')?;
        if name.contains('/') {
            return None;
        }
        Self::new(owner, name).ok()
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for RepoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl TryFrom<String> for RepoSource {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        let (owner, name) = text.split_once('/').ok_or(IdError {
            what: "Door43 repository",
            reason: "not owner/repository",
        })?;
        Self::new(owner, name)
    }
}

impl From<RepoSource> for String {
    fn from(source: RepoSource) -> Self {
        source.to_string()
    }
}

/// The name a project opened from Door43 gets: its folder on this computer. Letters, digits,
/// hyphens and underscores only, as tC4's `openRepoPathOf` allows (`openDoor43.nameBad`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectName(String);

impl ProjectName {
    /// tC4 caps a repo path's segment at 255 bytes, a folder name's limit.
    pub const MAX_BYTES: usize = 255;

    /// The typed name, without the spaces around it.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let name = text.trim();
        if name.is_empty()
            || name.len() > Self::MAX_BYTES
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(IdError {
                what: "project name",
                reason: "letters, numbers, hyphens and underscores only",
            });
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The project folder the name makes.
    pub fn folder(&self) -> ProjectRef {
        // Letters, digits, `-` and `_` are one path segment on every system, and the name is
        // never empty or only dots.
        ProjectRef::parse(&self.0).unwrap_or_else(|_| unreachable!("a checked project name"))
    }

    /// Whether a folder of `listing` has this name, without letter case: the disk of macOS
    /// and Windows holds one folder for two names that differ only in case.
    pub fn in_use<'a>(&self, listing: impl IntoIterator<Item = &'a str>) -> bool {
        listing
            .into_iter()
            .any(|folder| folder.eq_ignore_ascii_case(&self.0))
    }
}

impl TryFrom<String> for ProjectName {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        Self::parse(&text)
    }
}

impl From<ProjectName> for String {
    fn from(name: ProjectName) -> Self {
        name.0
    }
}

/// The project name a repository name suggests (tC4 `openDefaultName`): each run of signs a
/// project name cannot hold becomes one `-`, and no `-` stays at either end.
pub fn default_project_name(repo: &str) -> String {
    let mut out = String::with_capacity(repo.len());
    let mut gap = false;
    for c in repo.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    out.trim_matches('-').to_owned()
}

#[cfg(feature = "proptest")]
mod arbitrary {
    use proptest::arbitrary::Arbitrary;
    use proptest::strategy::{BoxedStrategy, Strategy};

    use super::{ProjectName, RepoSource};

    fn pattern(re: &str) -> impl Strategy<Value = String> {
        // The patterns are literals that compile.
        proptest::string::string_regex(re).unwrap_or_else(|_| unreachable!("a valid regex"))
    }

    impl Arbitrary for RepoSource {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            (
                pattern("[A-Za-z0-9_-]{1,12}"),
                pattern("[A-Za-z0-9_.-]{1,12}"),
            )
                .prop_filter_map("a valid source", |(owner, name)| {
                    RepoSource::new(&owner, &name).ok()
                })
                .boxed()
        }
    }

    impl Arbitrary for ProjectName {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;
        fn arbitrary_with((): ()) -> Self::Strategy {
            pattern("[A-Za-z0-9_-]{1,20}").prop_map(ProjectName).boxed()
        }
    }
}
