// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]
// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use tc_door43::{
    Credentials, Door43Client, GitOid, InternetSwitch, Limits, Password, RepoRef, TagName,
    TokenName,
};
use tc_door43_fake::{FakeDoor43, seeded};
use tokio::process::Command;

/// A client for the fake with the internet switch on.
pub fn client(fake: &FakeDoor43) -> Door43Client {
    client_with(fake, Limits::default())
}

pub fn client_with(fake: &FakeDoor43, limits: Limits) -> Door43Client {
    let switch = InternetSwitch::new();
    switch.turn_on();
    Door43Client::with_limits(fake.url().parse().unwrap(), switch, limits).unwrap()
}

pub fn repo(path: &str) -> RepoRef {
    path.parse().unwrap()
}

pub fn tag(name: &str) -> TagName {
    name.parse().unwrap()
}

pub fn sha(hex: &str) -> GitOid {
    hex.parse().unwrap()
}

pub fn token_name() -> TokenName {
    "translationCore".parse().unwrap()
}

pub fn credentials() -> Credentials {
    Credentials::new(seeded::USER, Password::new(seeded::PASSWORD.to_owned())).unwrap()
}

/// The names of the files in `folder`: what downloads left there.
pub fn left_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub fn fixture(path: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    std::fs::read(root.join(path)).unwrap()
}

/// A git command isolated from the user's configuration.
pub fn git(dir: &Path, home: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.org")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.org")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    command
}

pub async fn run(command: &mut Command) -> String {
    let output = command.output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
