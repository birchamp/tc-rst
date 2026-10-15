//! The git commands the fake runs. Every repository is a real bare repository, so
//! archives, tags and smart HTTP behave as they do behind Gitea, which runs the same
//! commands.

use crate::FakeError;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Variables that would point git at another repository, index or config.
const INHERITED_GIT_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
];

/// How a seeded file is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedMode {
    Regular,
    Executable,
    /// The bytes are the link target.
    Symlink,
}

impl SeedMode {
    fn git_mode(self) -> &'static str {
        match self {
            SeedMode::Regular => "100644",
            SeedMode::Executable => "100755",
            SeedMode::Symlink => "120000",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CommitInfo {
    pub sha: String,
    pub tree: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub author_date: String,
    pub committer_name: String,
    pub committer_email: String,
    pub committer_date: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TagInfo {
    pub name: String,
    /// The tag object for an annotated tag, else the commit.
    pub object: String,
    pub commit: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Git {
    /// `GIT_PROJECT_ROOT`: repositories live at `<root>/<owner>/<repo>.git`, lowercased.
    pub root: PathBuf,
    /// A private `HOME` so no user config applies.
    home: PathBuf,
}

impl Git {
    pub(crate) async fn new(base: &Path) -> Result<Self, FakeError> {
        let root = base.join("repos");
        let home = base.join("home");
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(&home)?;
        let git = Self { root, home };
        let version = git.command(None).arg("--version").output().await;
        match version {
            Ok(out) if out.status.success() => Ok(git),
            _ => Err(FakeError::GitMissing),
        }
    }

    pub(crate) fn repo_dir(&self, owner: &str, name: &str) -> PathBuf {
        self.root
            .join(owner.to_lowercase())
            .join(format!("{}.git", name.to_lowercase()))
    }

    pub(crate) fn command(&self, git_dir: Option<&Path>) -> Command {
        let mut command = Command::new("git");
        for var in INHERITED_GIT_VARS {
            command.env_remove(var);
        }
        command
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .kill_on_drop(true);
        if let Some(dir) = git_dir {
            command.arg("--git-dir").arg(dir);
        }
        command
    }

    pub(crate) async fn run(
        &self,
        git_dir: Option<&Path>,
        args: &[&str],
        stdin: Option<&[u8]>,
        env: &[(&str, &str)],
    ) -> Result<Vec<u8>, FakeError> {
        let mut command = self.command(git_dir);
        command.args(args).envs(env.iter().copied());
        command
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(bytes).await?;
            drop(pipe);
        }
        let output = child.wait_with_output().await?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(FakeError::Git {
                args: args.join(" "),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            })
        }
    }

    pub(crate) async fn init_bare(&self, dir: &Path, branch: &str) -> Result<(), FakeError> {
        let dir_str = dir.to_string_lossy().into_owned();
        self.run(
            None,
            &[
                "init",
                "--quiet",
                "--bare",
                "--initial-branch",
                branch,
                &dir_str,
            ],
            None,
            &[],
        )
        .await?;
        Ok(())
    }

    /// Commit a snapshot: the tree holds exactly `files`. Returns the commit sha.
    pub(crate) async fn commit_snapshot(
        &self,
        dir: &Path,
        branch: &str,
        files: &[(String, SeedMode, Vec<u8>)],
        message: &str,
        unix_time: i64,
    ) -> Result<String, FakeError> {
        let mut index_lines = String::new();
        for (path, mode, bytes) in files {
            let blob = self
                .run(
                    Some(dir),
                    &["hash-object", "-w", "--stdin"],
                    Some(bytes),
                    &[],
                )
                .await?;
            let blob = String::from_utf8_lossy(&blob).trim().to_owned();
            index_lines.push_str(&format!("{} {blob}\t{path}\n", mode.git_mode()));
        }
        let index_dir = tempfile::Builder::new()
            .prefix("index-")
            .tempdir_in(&self.home)?;
        let index_path = index_dir
            .path()
            .join("index")
            .to_string_lossy()
            .into_owned();
        let index_env = [("GIT_INDEX_FILE", index_path.as_str())];
        self.run(Some(dir), &["read-tree", "--empty"], None, &index_env)
            .await?;
        self.run(
            Some(dir),
            &["update-index", "--add", "--index-info"],
            Some(index_lines.as_bytes()),
            &index_env,
        )
        .await?;
        let tree = self
            .run(Some(dir), &["write-tree"], None, &index_env)
            .await?;
        let tree = String::from_utf8_lossy(&tree).trim().to_owned();

        let parent = self
            .resolve(dir, &format!("refs/heads/{branch}"))
            .await
            .ok();
        let mut args = vec!["commit-tree", tree.as_str(), "-m", message];
        if let Some(parent) = &parent {
            args.extend(["-p", parent.as_str()]);
        }
        let date = format!("{unix_time} +0000");
        let env = [
            ("GIT_AUTHOR_NAME", "Door43 Fake"),
            ("GIT_AUTHOR_EMAIL", "fake@door43.invalid"),
            ("GIT_AUTHOR_DATE", date.as_str()),
            ("GIT_COMMITTER_NAME", "Door43 Fake"),
            ("GIT_COMMITTER_EMAIL", "fake@door43.invalid"),
            ("GIT_COMMITTER_DATE", date.as_str()),
        ];
        let commit = self.run(Some(dir), &args, None, &env).await?;
        let commit = String::from_utf8_lossy(&commit).trim().to_owned();
        self.run(
            Some(dir),
            &["update-ref", &format!("refs/heads/{branch}"), &commit],
            None,
            &[],
        )
        .await?;
        Ok(commit)
    }

    pub(crate) async fn tag(&self, dir: &Path, name: &str, commit: &str) -> Result<(), FakeError> {
        self.run(Some(dir), &["tag", name, commit], None, &[])
            .await?;
        Ok(())
    }

    /// The commit `rev` names, or an error when it names none.
    pub(crate) async fn resolve(&self, dir: &Path, rev: &str) -> Result<String, FakeError> {
        let spec = format!("{rev}^{{commit}}");
        let out = self
            .run(
                Some(dir),
                &["rev-parse", "--verify", "--quiet", &spec],
                None,
                &[],
            )
            .await?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Like [`Git::resolve`], for any object (`<sha>^{tree}`, a blob path).
    pub(crate) async fn rev_parse(&self, dir: &Path, rev: &str) -> Result<String, FakeError> {
        let out = self
            .run(
                Some(dir),
                &["rev-parse", "--verify", "--quiet", rev],
                None,
                &[],
            )
            .await?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }

    pub(crate) async fn tags(&self, dir: &Path) -> Result<Vec<TagInfo>, FakeError> {
        let out = self
            .run(
                Some(dir),
                &[
                    "for-each-ref",
                    "--sort=-creatordate",
                    "--format=%(refname:strip=2)%00%(objectname)%00%(*objectname)",
                    "refs/tags",
                ],
                None,
                &[],
            )
            .await?;
        Ok(String::from_utf8_lossy(&out)
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\0');
                let name = parts.next()?.to_owned();
                let object = parts.next()?.to_owned();
                let peeled = parts.next().unwrap_or_default();
                let commit = if peeled.is_empty() {
                    object.clone()
                } else {
                    peeled.to_owned()
                };
                Some(TagInfo {
                    name,
                    object,
                    commit,
                })
            })
            .collect())
    }

    pub(crate) async fn commit_info(&self, dir: &Path, sha: &str) -> Result<CommitInfo, FakeError> {
        let out = self
            .run(
                Some(dir),
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%T%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B",
                    sha,
                ],
                None,
                &[],
            )
            .await?;
        let text = String::from_utf8_lossy(&out).into_owned();
        let parts: Vec<&str> = text.splitn(10, '\0').collect();
        if parts.len() < 10 {
            return Err(FakeError::Invalid(format!("unreadable commit {sha}")));
        }
        Ok(CommitInfo {
            sha: parts[0].to_owned(),
            tree: parts[1].to_owned(),
            parents: parts[2].split_whitespace().map(str::to_owned).collect(),
            author_name: parts[3].to_owned(),
            author_email: parts[4].to_owned(),
            author_date: parts[5].to_owned(),
            committer_name: parts[6].to_owned(),
            committer_email: parts[7].to_owned(),
            committer_date: parts[8].to_owned(),
            message: parts[9].trim_end_matches('\n').to_owned() + "\n",
        })
    }

    pub(crate) async fn commit_time(&self, dir: &Path, sha: &str) -> Result<i64, FakeError> {
        let out = self
            .run(Some(dir), &["show", "-s", "--format=%ct", sha], None, &[])
            .await?;
        String::from_utf8_lossy(&out)
            .trim()
            .parse()
            .map_err(|_| FakeError::Invalid(format!("no commit time for {sha}")))
    }

    /// The bytes of `path` in `commit`, or `None` when the commit has no such file.
    pub(crate) async fn file_at(
        &self,
        dir: &Path,
        commit: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, FakeError> {
        let spec = format!("{commit}:{path}");
        if self.rev_parse(dir, &spec).await.is_err() {
            return Ok(None);
        }
        self.run(Some(dir), &["cat-file", "blob", &spec], None, &[])
            .await
            .map(Some)
    }

    /// `git archive`, as Gitea runs it: the files under `<prefix>/`, and for a zip the
    /// commit id in the archive comment.
    pub(crate) async fn archive(
        &self,
        dir: &Path,
        commit: &str,
        format: &str,
        prefix: &str,
    ) -> Result<Vec<u8>, FakeError> {
        let prefix_arg = format!("--prefix={prefix}/");
        let format_arg = format!("--format={format}");
        self.run(
            Some(dir),
            &["archive", &format_arg, &prefix_arg, commit],
            None,
            &[],
        )
        .await
    }
}
