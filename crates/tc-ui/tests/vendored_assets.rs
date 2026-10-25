//! The files under `assets/` are tC4's, byte for byte (`vendor-tc4.sh`): the design system's
//! CSS, fonts and images and the interface catalogs. A file that differs from tC4's draws or
//! says something tC4 does not, as an old `base.css` without the #737 `::selection` rule would
//! paint text selected in Check's "Your translation" line with the yellow of a selected word.
//! `vendored-from-tc4.txt` names each file by its git blob id in tC4, so
//! `git -C <tC4> ls-tree -r <commit>` confirms the list.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// tC4's path of a vendored file -> its path under `assets/`, as `vendor-tc4.sh` places it.
fn asset_path(tc4_path: &str) -> String {
    if tc4_path == "src/ui.css" {
        return "ui.css".into();
    }
    if tc4_path == "LICENSE" || tc4_path == "COPYING" {
        return format!("about/{tc4_path}");
    }
    for (from, to) in [
        ("src/ds/", "ds/"),
        ("src/i18n/", "i18n/"),
        ("public/assets/", "images/"),
    ] {
        if let Some(rest) = tc4_path.strip_prefix(from) {
            return format!("{to}{rest}");
        }
    }
    panic!("vendored-from-tc4.txt names {tc4_path}, which has no place under assets/");
}

/// The id git gives a file's bytes: SHA-1 of `blob <length>\0` and the bytes.
fn git_blob_id(bytes: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// (blob id, asset path) for each line of the list.
fn vendored() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(crate_dir().join("vendored-from-tc4.txt"))
        .expect("vendored-from-tc4.txt is readable");
    let mut commit = None;
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        if let Some(c) = line.strip_prefix("commit ") {
            commit = Some(c.to_string());
            continue;
        }
        let (blob, path) = line
            .split_once(' ')
            .unwrap_or_else(|| panic!("not `<blob> <path>`: {line:?}"));
        assert!(
            blob.len() == 40 && blob.bytes().all(|b| b.is_ascii_hexdigit()),
            "not a blob id: {line:?}"
        );
        out.push((blob.to_string(), asset_path(path)));
    }
    assert!(
        commit.is_some_and(|c| c.len() == 40),
        "the list names the tC4 commit"
    );
    out
}

fn files_under(dir: &Path, base: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).expect("a readable folder") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            files_under(&path, base, out);
        } else {
            let rel = path.strip_prefix(base).expect("under assets/");
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn every_vendored_file_has_the_bytes_tc4_has() {
    let assets = crate_dir().join("assets");
    let changed: Vec<_> = vendored()
        .into_iter()
        .filter(|(blob, path)| {
            let bytes = std::fs::read(assets.join(path))
                .unwrap_or_else(|e| panic!("assets/{path} is missing: {e}"));
            git_blob_id(&bytes) != *blob
        })
        .map(|(_, path)| path)
        .collect();
    assert!(
        changed.is_empty(),
        "these files differ from tC4's; copy them again with vendor-tc4.sh: {changed:?}"
    );
}

/// The rebuild's own interface text, beside tC4's catalogs (`assets/i18n-rebuild/README.md`).
const REBUILD_TEXT: &str = "i18n-rebuild/";

/// The OFL must ship with the Awami Nastaliq fonts, and tC4 vendors them without it; the text is
/// the fonts' own licence record (name ID 13), so it is SIL's, not the rebuild's.
const AWAMI_OFL: &str = "ds/assets/fonts/OFL-AwamiNastaliq.txt";

#[test]
fn assets_hold_only_the_vendored_files() {
    let assets = crate_dir().join("assets");
    let mut on_disk = BTreeSet::new();
    files_under(&assets, &assets, &mut on_disk);
    on_disk.retain(|path| !path.starts_with(REBUILD_TEXT) && path != AWAMI_OFL);
    let listed: BTreeSet<String> = vendored().into_iter().map(|(_, p)| p).collect();
    let unlisted: Vec<_> = on_disk.difference(&listed).collect();
    assert!(
        unlisted.is_empty(),
        "files under assets/ that tC4 does not have: {unlisted:?}"
    );
    // The two that draw the #737 rule and every word on screen.
    assert!(listed.contains("ds/tokens/base.css"));
    assert!(listed.contains("i18n/en.json"));
}

#[test]
fn the_blob_id_is_gits() {
    // `printf 'hello\n' | git hash-object --stdin`
    assert_eq!(
        git_blob_id(b"hello\n"),
        "ce013625030ba8dba906f756967f9e9ca394464a"
    );
    assert_eq!(git_blob_id(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
}
