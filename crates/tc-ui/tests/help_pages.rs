//! The help pages under `help/` are tc-website's `tc-help/` at the commit tC4 pins, byte for
//! byte (`vendor-help.sh`, APP-DESIGN AD29). `help-pages.txt` names each file by its git blob
//! id in tc-website, so `git ls-tree -r <commit> tc-help` there confirms the list (tC4 #741's
//! "help sha-list check").
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

/// The pin of tC4's `scripts/fetch-help.mjs` at d310bf3, the rebuild's reference.
const HELP_PIN: &str = "cdfdf9b7d534e7574beb3fcf44678973cdc5f9d7";

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn git_blob_id(bytes: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

struct List {
    repository: String,
    commit: String,
    files: Vec<(String, String)>,
}

fn list() -> List {
    let text = std::fs::read_to_string(crate_dir().join("help-pages.txt")).unwrap();
    let (mut repository, mut commit, mut files) = (None, None, Vec::new());
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        if let Some(r) = line.strip_prefix("repository ") {
            repository = Some(r.to_owned());
        } else if let Some(c) = line.strip_prefix("commit ") {
            commit = Some(c.to_owned());
        } else {
            let (blob, path) = line
                .split_once(' ')
                .unwrap_or_else(|| panic!("not `<blob> <path>`: {line:?}"));
            assert!(
                blob.len() == 40 && blob.bytes().all(|b| b.is_ascii_hexdigit()),
                "not a blob id: {line:?}"
            );
            files.push((blob.to_owned(), path.to_owned()));
        }
    }
    List {
        repository: repository.expect("a repository line"),
        commit: commit.expect("a commit line"),
        files,
    }
}

fn files_under(dir: &Path, base: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, base, out);
        } else {
            let rel = path.strip_prefix(base).unwrap().to_string_lossy();
            out.insert(rel.replace('\\', "/"));
        }
    }
}

#[test]
fn every_help_page_has_the_bytes_tc_website_has_at_the_pin() {
    let list = list();
    assert_eq!(
        list.repository,
        "https://github.com/unfoldingWord/tc-website.git"
    );
    assert_eq!(list.commit, HELP_PIN);
    let help = crate_dir().join("help");
    let mut listed = BTreeSet::new();
    for (blob, path) in &list.files {
        let bytes = std::fs::read(help.join(path))
            .unwrap_or_else(|e| panic!("help/{path} is listed but cannot be read: {e}"));
        assert_eq!(&git_blob_id(&bytes), blob, "help/{path} is not tc-website's");
        assert!(listed.insert(path.clone()), "help/{path} is listed twice");
    }
    let mut present = BTreeSet::new();
    files_under(&help, &help, &mut present);
    let unlisted: Vec<_> = present.difference(&listed).collect();
    assert!(
        unlisted.is_empty(),
        "files under help/ that help-pages.txt does not list: {unlisted:?}"
    );
    assert_eq!(list.files.len(), 180, "tc-help/ at the pin holds 180 files");
}

#[test]
fn the_help_home_is_the_pages_index() {
    let index = std::fs::read_to_string(crate_dir().join("help/index.html")).unwrap();
    assert!(index.contains("<title>Home · translationCore 4 Help</title>"));
    assert!(index.contains("assets/style.css"));
}

/// The pages load nothing from another origin, so opening help sends nothing over the
/// internet (tC4's help case b). A link to another site is a navigation, which the internet
/// question guards.
#[test]
fn help_pages_load_nothing_from_another_origin() {
    let remote = regex::Regex::new(
        r#"(?i)(<script[^>]*\ssrc\s*=\s*["']?(https?:)?//|<link[^>]*\shref\s*=\s*["']?(https?:)?//|<img[^>]*\ssrc\s*=\s*["']?(https?:)?//|url\(\s*["']?(https?:)?//|@import\s+["']?(https?:)?//)"#,
    )
    .unwrap();
    let help = crate_dir().join("help");
    let mut all = BTreeSet::new();
    files_under(&help, &help, &mut all);
    let mut checked = 0;
    for path in all {
        if !(path.ends_with(".html") || path.ends_with(".css") || path.ends_with(".js")) {
            continue;
        }
        let text = std::fs::read_to_string(help.join(&path)).unwrap();
        assert!(
            !remote.is_match(&text),
            "help/{path} loads something from another origin"
        );
        checked += 1;
    }
    assert!(checked > 50, "only {checked} pages and style sheets checked");
}
