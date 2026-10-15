//! Zip rewriting for the burrito export and for injected archive faults.

use crate::FakeError;
use serde_json::{Value, json};
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

fn zip_error(e: zip::result::ZipError) -> FakeError {
    FakeError::Invalid(format!("zip: {e}"))
}

fn deflated() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

/// Replace the end-of-central-directory comment, keeping every entry.
pub(crate) fn with_comment(zip: Vec<u8>, comment: &str) -> Result<Vec<u8>, FakeError> {
    let mut writer = ZipWriter::new_append(Cursor::new(zip)).map_err(zip_error)?;
    writer.set_comment(comment).map_err(zip_error)?;
    Ok(writer.finish().map_err(zip_error)?.into_inner())
}

/// Add one file, keeping the entries and the comment.
pub(crate) fn with_extra_file(
    zip: Vec<u8>,
    path: &str,
    bytes: &[u8],
) -> Result<Vec<u8>, FakeError> {
    let mut writer = ZipWriter::new_append(Cursor::new(zip)).map_err(zip_error)?;
    writer.start_file(path, deflated()).map_err(zip_error)?;
    writer.write_all(bytes)?;
    Ok(writer.finish().map_err(zip_error)?.into_inner())
}

/// What the DCS export holds beyond the repository's files.
pub(crate) struct SbExport<'a> {
    /// `<owner>/<repo>`, the key DCS writes under `identification.primary.dcs`.
    pub dcs_key: &'a str,
    pub revision: &'a str,
    pub timestamp: &'a str,
    /// Paths, relative to the burrito root, whose bytes are altered after the metadata.
    pub corrupt: &'a [String],
}

/// The burrito export of a commit, built from `git archive` output whose entries sit
/// under `<prefix>/`: the metadata's DCS revision is set to the commit, as DCS sets it,
/// and the export's `.DS_Store` and `.git/` debris is added. `None` when the commit holds
/// no `metadata.json`.
pub(crate) fn sb_export(
    archive: &[u8],
    prefix: &str,
    export: &SbExport<'_>,
) -> Result<Option<Vec<u8>>, FakeError> {
    let mut reader = ZipArchive::new(Cursor::new(archive)).map_err(zip_error)?;
    let metadata_name = format!("{prefix}/metadata.json");
    if reader.index_for_name(&metadata_name).is_none() {
        return Ok(None);
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..reader.len() {
        let mut entry = reader.by_index(index).map_err(zip_error)?;
        let name = entry.name().to_owned();
        if entry.is_dir() {
            writer.add_directory(name, deflated()).map_err(zip_error)?;
            continue;
        }
        let mode = entry.unix_mode();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        if name == metadata_name {
            bytes = patched_metadata(&bytes, export)?;
        }
        let relative = name.strip_prefix(&format!("{prefix}/")).unwrap_or(&name);
        if export.corrupt.iter().any(|c| c == relative) {
            bytes.extend_from_slice(b"!");
        }
        // Modes are kept as `git archive` wrote them, links included, so a client sees
        // what the commit holds.
        if mode.is_some_and(|m| m & 0o170_000 == 0o120_000) {
            let target = String::from_utf8(bytes)
                .map_err(|_| FakeError::Invalid(format!("{name}: a link target is not UTF-8")))?;
            writer
                .add_symlink(name, target, deflated())
                .map_err(zip_error)?;
            continue;
        }
        let options = match mode {
            Some(mode) => deflated().unix_permissions(mode & 0o777),
            None => deflated(),
        };
        writer.start_file(name, options).map_err(zip_error)?;
        writer.write_all(&bytes)?;
    }
    writer
        .start_file(format!("{prefix}/.DS_Store"), deflated())
        .map_err(zip_error)?;
    writer.write_all(b"\0\0\0\x01Bud1")?;
    writer
        .start_file(format!("{prefix}/.git/HEAD"), deflated())
        .map_err(zip_error)?;
    writer.write_all(b"ref: refs/heads/master\n")?;
    Ok(Some(writer.finish().map_err(zip_error)?.into_inner()))
}

fn patched_metadata(bytes: &[u8], export: &SbExport<'_>) -> Result<Vec<u8>, FakeError> {
    let mut metadata: Value = serde_json::from_slice(bytes)
        .map_err(|e| FakeError::Invalid(format!("metadata.json: {e}")))?;
    let dcs = json!({
        export.dcs_key: { "revision": export.revision, "timestamp": export.timestamp }
    });
    let Some(root) = metadata.as_object_mut() else {
        return Err(FakeError::Invalid("metadata.json is not an object".into()));
    };
    let identification = root.entry("identification").or_insert_with(|| json!({}));
    if let Some(identification) = identification.as_object_mut() {
        let primary = identification.entry("primary").or_insert_with(|| json!({}));
        if let Some(primary) = primary.as_object_mut() {
            primary.insert("dcs".to_owned(), dcs);
        }
    }
    serde_json::to_vec_pretty(&metadata).map_err(|e| FakeError::Invalid(e.to_string()))
}
