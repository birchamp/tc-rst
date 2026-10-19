//! The real exports of `bundle/en` installed into a store, with the code the packager uses
//! (`tc_resources::install_bundle`). Installing verifies the md5 of every ingredient, so a
//! store of the whole suite costs real time in a debug build: tests install the repositories
//! they read.

use std::io::{Read, Write};
use std::path::PathBuf;

use tc_resources::{ResourcePin, ResourceStore, install_bundle};

pub fn bundle() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en")
}

/// A store holding the repositories `repos` name (`en_tw`, `en_twl`, ...), each installed
/// from its export or archive in `bundle/en`.
pub fn store_with(repos: &[&str]) -> (tempfile::TempDir, ResourceStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path().join("resources")).unwrap();
    let wanted = |pin: &ResourcePin| repos.contains(&pin.repo.name().as_str());
    let installed = install_bundle(&bundle(), &store, &wanted).unwrap();
    assert_eq!(installed.installed.len(), repos.len(), "{installed:?}");
    (dir, store)
}

/// The repositories Psalms and Numbers need: the helps and the articles they link, the three
/// Bible panes and the original-language text of the Old Testament.
pub const BOOK_REPOS: &[&str] = &[
    "en_tw", "en_twl", "en_tn", "en_ta", "en_ult", "en_ust", "hbo_uhb",
];

/// The export `file` of `bundle/en` with `replace` applied to its entries: an entry whose
/// name `replace` returns bytes for gets them (and its `metadata.json` listing the new md5 and
/// size), so the copy still installs.
pub fn export_with(file: &str, root: &str, replace: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(bundle().join(file)).unwrap()).unwrap();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let metadata_name = format!("{root}/metadata.json");
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        if name.ends_with('/') {
            writer.add_directory(name, options).unwrap();
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == metadata_name {
            let mut metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            for (path, new) in replace {
                let listed = &mut metadata["ingredients"][*path];
                assert!(listed.is_object(), "{path} is listed in the metadata");
                listed["checksum"]["md5"] = tc_core::Md5::of(new).to_hex().into();
                listed["size"] = new.len().into();
            }
            bytes = serde_json::to_vec_pretty(&metadata).unwrap();
        } else if let Some((_, new)) = replace
            .iter()
            .find(|(path, _)| name == format!("{root}/{path}"))
        {
            bytes.clone_from(new);
        }
        writer.start_file(name, options).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
