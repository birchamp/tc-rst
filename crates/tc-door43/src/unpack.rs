//! Reading a downloaded zip into memory, with the checks every download gets.
//!
//! Every entry is decompressed in full, which makes the zip reader check its CRC-32, so a
//! truncated or damaged download fails here. Entry names are checked before anything else
//! looks at them, and a bad name is refused, never repaired, because repairing could merge
//! two entries into one path:
//!
//! - a name that could escape the archive's folder on any platform ([`escapes_root`]);
//! - a name Windows cannot write, or would write under another name ([`tc_core::check_path`]);
//! - two names that Windows or macOS would store as one file ([`collision_key`]).
//!
//! [`check_names`] applies all three to any list of paths, so a crate that writes files
//! from another source (an install into a store) refuses the same names.

use crate::error::VerificationError;
use icu_normalizer::ComposingNormalizerBorrowed;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Seek};

/// How a file is stored in git: the zip's unix mode, as `git archive` writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileMode {
    Regular,
    Executable,
    /// The file's bytes are the link target, as git stores a symbolic link. Never create
    /// such an entry as a link on disk: its target can point anywhere, outside the
    /// resource folder. Write the target as a plain file, or leave the entry out.
    Symlink,
}

/// One file from an archive. `path` is relative to the unpacked root and uses `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpackedFile {
    pub path: String,
    /// See [`FileMode::Symlink`] before writing a file to disk.
    pub mode: FileMode,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// `git archive` output (a Gitea archive): every entry sits under one top folder, which
    /// is removed, and every entry is content, so none is dropped.
    GitArchive,
    /// A burrito zip: the DCS export, or one a person made and picked from a USB drive.
    /// Debris ([`is_burrito_debris`]) is dropped first; then the top folder is removed when
    /// every entry shares one and the root holds no `metadata.json` (tC4 `unwrapExport`).
    Burrito,
}

#[derive(Debug)]
pub(crate) struct Unpacked {
    /// Files sorted by path.
    pub files: Vec<UnpackedFile>,
    /// Folders the archive names explicitly, relative to the unpacked root, without the
    /// trailing `/`.
    pub folders: BTreeSet<String>,
    /// The end-of-central-directory comment.
    pub comment: Vec<u8>,
}

struct RawEntry {
    name: String,
    folder: bool,
    mode: FileMode,
    bytes: Vec<u8>,
}

/// Unzip the zip `zip` reads, checking every entry. `max_unpacked` caps the total
/// decompressed size. The zip itself is never read into memory whole.
pub(crate) fn unpack_from<R: Read + Seek>(
    zip: R,
    layout: Layout,
    max_unpacked: u64,
) -> Result<Unpacked, VerificationError> {
    let not_a_zip = |e: zip::result::ZipError| VerificationError::NotAZip(e.to_string());
    let archive = zip::ZipArchive::new(zip).map_err(not_a_zip)?;
    // The zip reader keeps one entry per name, so a repeated name would vanish here.
    let (start, len) = (archive.central_directory_start(), archive.len());
    let mut zip = archive.into_inner();
    let records = tc_core::central_directory_records(&mut zip, start, len + 1)
        .map_err(|e| not_a_zip(e.into()))?;
    if records != len {
        return Err(VerificationError::NotAZip(
            "two entries have one name".to_owned(),
        ));
    }
    let mut archive = zip::ZipArchive::new(zip).map_err(not_a_zip)?;
    let comment = archive.comment().to_vec();
    let mut entries = Vec::with_capacity(archive.len());
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| VerificationError::NotAZip(e.to_string()))?;
        let name = std::str::from_utf8(file.name_raw())
            .map_err(|_| VerificationError::NonUtf8Name)?
            .to_owned();
        if escapes_root(&name) {
            return Err(VerificationError::EscapesRoot(name));
        }
        if layout == Layout::Burrito && is_burrito_debris(&name) {
            continue;
        }
        let folder = name.ends_with('/');
        let mode = mode_of(file.unix_mode());
        let mut content = Vec::new();
        if !folder {
            let remaining = max_unpacked.saturating_sub(total);
            // One byte past the budget tells "exactly at the limit" from "over it".
            (&mut file)
                .take(remaining.saturating_add(1))
                .read_to_end(&mut content)
                .map_err(|e| VerificationError::CorruptEntry {
                    path: name.clone(),
                    reason: e.to_string(),
                })?;
            total = total.saturating_add(content.len() as u64);
            if total > max_unpacked {
                return Err(VerificationError::TooLargeUnpacked {
                    limit: max_unpacked,
                });
            }
            // Reading to the end is what makes the zip reader check the CRC; a reader
            // stopped by `take` has not reached it yet.
            let mut rest = Vec::new();
            file.read_to_end(&mut rest)
                .map_err(|e| VerificationError::CorruptEntry {
                    path: name.clone(),
                    reason: e.to_string(),
                })?;
            if !rest.is_empty() {
                return Err(VerificationError::TooLargeUnpacked {
                    limit: max_unpacked,
                });
            }
        }
        entries.push(RawEntry {
            name,
            folder,
            mode,
            bytes: content,
        });
    }
    strip_and_collect(entries, layout, comment)
}

/// Unzips a zip held in memory whose files sit at its root or under one top folder, as a
/// burrito or a resource release does, with every check [`unpack_from`] makes. Debris
/// ([`is_burrito_debris`]) is dropped; the top folder is removed when every entry shares
/// one and the root holds no `metadata.json`. Files come back sorted by path. A symbolic
/// link comes back as a file with [`FileMode::Symlink`]: a caller that refuses links
/// checks the mode.
pub fn unpack_zip(bytes: &[u8], max_unpacked: u64) -> Result<Vec<UnpackedFile>, VerificationError> {
    Ok(unpack_from(Cursor::new(bytes), Layout::Burrito, max_unpacked)?.files)
}

/// Entries that are never part of a burrito: the `.git/` folder and `.DS_Store` files the
/// DCS export ships, and what macOS Finder adds to a zip it makes (a `__MACOSX/` folder of
/// AppleDouble `._*` files).
fn is_burrito_debris(name: &str) -> bool {
    let body = name.strip_suffix('/').unwrap_or(name);
    let first = body.split('/').next().unwrap_or_default();
    let last = body.rsplit('/').next().unwrap_or_default();
    first == "__MACOSX"
        || body.split('/').any(|segment| segment == ".git")
        || last == ".DS_Store"
        || last.starts_with("._")
}

fn strip_and_collect(
    entries: Vec<RawEntry>,
    layout: Layout,
    comment: Vec<u8>,
) -> Result<Unpacked, VerificationError> {
    if !entries.iter().any(|e| !e.folder) {
        return Err(VerificationError::Empty);
    }
    let top_of = |name: &str| name.split('/').next().unwrap_or_default().to_owned();
    let tops: BTreeSet<String> = entries.iter().map(|e| top_of(&e.name)).collect();
    let single_top = tops.len() == 1
        && entries
            .iter()
            .filter(|e| !e.folder)
            .all(|e| e.name.contains('/'));
    let prefix = match layout {
        Layout::GitArchive | Layout::Burrito if single_top => tops.into_iter().next(),
        Layout::GitArchive => return Err(VerificationError::NotWrapped),
        Layout::Burrito => None,
    };

    let mut files = Vec::new();
    let mut folders = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        let relative = match &prefix {
            Some(top) => match entry.name.strip_prefix(top.as_str()) {
                Some(rest) => rest.strip_prefix('/').unwrap_or(rest).to_owned(),
                None => return Err(VerificationError::NotWrapped),
            },
            None => entry.name.clone(),
        };
        let relative = relative.trim_end_matches('/').to_owned();
        if relative.is_empty() {
            if entry.folder {
                continue;
            }
            return Err(VerificationError::NotWrapped);
        }
        if !seen.insert(relative.clone()) {
            return Err(VerificationError::DuplicateEntry(relative));
        }
        if let Some(problem) = name_problem(&relative) {
            return Err(problem.into());
        }
        if entry.folder {
            folders.insert(relative);
        } else {
            files.push(UnpackedFile {
                path: relative,
                mode: entry.mode,
                bytes: entry.bytes,
            });
        }
    }
    let names: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    find_collision(&names, &folders)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Unpacked {
        files,
        folders,
        comment,
    })
}

fn mode_of(unix_mode: Option<u32>) -> FileMode {
    const FILE_TYPE: u32 = 0o170_000;
    const SYMLINK: u32 = 0o120_000;
    match unix_mode {
        Some(mode) if mode & FILE_TYPE == SYMLINK => FileMode::Symlink,
        Some(mode) if mode & 0o111 != 0 => FileMode::Executable,
        _ => FileMode::Regular,
    }
}

/// Whether a zip entry name could write outside the unpacked folder on any platform
/// ([`tc_core::PathProblem::leaves_folder`]). A folder's one trailing `/` is allowed.
fn escapes_root(name: &str) -> bool {
    let body = name.strip_suffix('/').unwrap_or(name);
    tc_core::check_path(body).is_err_and(tc_core::PathProblem::leaves_folder)
}

/// The first name in a list of relative paths that cannot be written to every file system
/// the app runs on. See [`check_names`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameProblem {
    /// `path` could name a file outside the folder: see [`tc_core::PathProblem::leaves_folder`].
    EscapesRoot(String),
    /// Windows cannot write `path` as it is named, or would write it under another name,
    /// or would open another file through it. `reason` says which.
    NotPortable { path: String, reason: &'static str },
    /// The same path is named twice, or names both a file and a folder.
    Duplicate(String),
    /// The names differ, but a case-insensitive or normalization-insensitive file system
    /// (the Windows and macOS defaults) would make them one file or folder, and writing
    /// both would lose one. A file also collides with a folder of the same folded name,
    /// and folders that a file's path implies count: `Notes/a` and `notes/b` collide on
    /// `Notes` and `notes`.
    Colliding { first: String, second: String },
}

impl From<NameProblem> for VerificationError {
    fn from(problem: NameProblem) -> Self {
        match problem {
            NameProblem::EscapesRoot(path) => VerificationError::EscapesRoot(path),
            NameProblem::NotPortable { path, reason } => {
                VerificationError::NotPortable { path, reason }
            }
            NameProblem::Duplicate(path) => VerificationError::DuplicateEntry(path),
            NameProblem::Colliding { first, second } => {
                VerificationError::CollidingNames { first, second }
            }
        }
    }
}

/// Checks the names of files about to be written into one folder, and returns the first
/// problem found. Every path is relative and uses `/`, as [`UnpackedFile::path`] does.
///
/// Refused, never repaired, because repairing could merge two files into one:
///
/// - a name Windows cannot write as it is named: a forbidden character, a segment of only
///   dots and spaces, a trailing dot or space (Windows strips it), a device name such as
///   `CON` or `nul.json`, or the shape of an 8.3 short name such as `PROGRA~1`;
/// - two names that Windows or macOS would store as one file or folder: they differ only
///   in letter case or Unicode normalization, or a file's name is a folder's (`A` and
///   `a/x`). The folders a path implies are compared too.
///
/// A path that could leave the folder (`..`, a drive letter, a backslash, a leading `/`)
/// is [`NameProblem::EscapesRoot`]; each path is checked by [`tc_core::check_path`].
pub fn check_names<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<(), NameProblem> {
    let paths: Vec<&str> = paths.into_iter().collect();
    if let Some(problem) = paths.iter().find_map(|path| name_problem(path)) {
        return Err(problem);
    }
    find_collision(&paths, &BTreeSet::new())
}

/// Why `path` cannot be written as it is named into a folder ([`tc_core::check_path`]),
/// in this crate's words.
fn name_problem(path: &str) -> Option<NameProblem> {
    use tc_core::PathProblem as P;
    let reason = match tc_core::check_path(path).err()? {
        P::Empty
        | P::Absolute
        | P::Backslash
        | P::DrivePrefix
        | P::Nul
        | P::EmptySegment
        | P::CurrentDir
        | P::ParentDir => return Some(NameProblem::EscapesRoot(path.to_owned())),
        P::Control(_) | P::ForbiddenChar(_) => {
            "it holds a character Windows does not allow in a name"
        }
        P::TrailingDot | P::TrailingSpace => {
            "Windows removes a trailing dot or space, so the name would change"
        }
        P::ShortName => "Windows can open another file through this short name (8.3)",
        P::ReservedName => "Windows reserves this name for a device",
    };
    Some(NameProblem::NotPortable {
        path: path.to_owned(),
        reason,
    })
}

/// The name a case-insensitive, normalization-insensitive file system sees: each segment in
/// Unicode NFC (macOS stores NFD), lowercased, then upper-cased wherever that gives one
/// character, because macOS folds `σ` and `ς`, `ſ` and `s`, `µ` and `μ` into one letter.
/// `ı` (dotless i) stays apart from `i`: no supported file system merges them, though `ı`
/// upper-cases to `I`. Segments ending in a dot or space never get here:
/// [`tc_core::check_path`] refuses them.
pub(crate) fn collision_key(path: &str) -> String {
    const DOTLESS_I: char = '\u{131}';
    let nfc = ComposingNormalizerBorrowed::new_nfc();
    path.split('/')
        .map(|segment| {
            let folded: String = nfc
                .normalize(segment)
                .to_lowercase()
                .chars()
                .map(|c| {
                    let mut upper = c.to_uppercase();
                    match (upper.next(), upper.next()) {
                        (Some(single), None) if c != DOTLESS_I => single,
                        _ => c,
                    }
                })
                .collect();
            nfc.normalize(&folded).into_owned()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Finds two paths, files or folders, that differ but share a [`collision_key`], a file
/// named twice, and a file whose key is a folder's. Folders include those implied by a
/// file's path.
fn find_collision(files: &[&str], folders: &BTreeSet<String>) -> Result<(), NameProblem> {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        File,
        Folder,
    }
    let mut by_key: BTreeMap<String, (&str, Kind)> = BTreeMap::new();
    let implied = files
        .iter()
        .flat_map(|f| f.match_indices('/').map(|(at, _)| &f[..at]));
    let all = files
        .iter()
        .map(|f| (*f, Kind::File))
        .chain(folders.iter().map(|f| (f.as_str(), Kind::Folder)))
        .chain(implied.map(|f| (f, Kind::Folder)));
    for (path, kind) in all {
        let key = collision_key(path);
        match by_key.get(&key) {
            None => {
                by_key.insert(key, (path, kind));
            }
            // Every file under a folder implies it again.
            Some((first, Kind::Folder)) if *first == path && kind == Kind::Folder => {}
            Some((first, _)) if *first == path => {
                return Err(NameProblem::Duplicate(path.to_owned()));
            }
            Some((first, _)) => {
                let (first, second) = if *first <= path {
                    (*first, path)
                } else {
                    (path, *first)
                };
                return Err(NameProblem::Colliding {
                    first: first.to_owned(),
                    second: second.to_owned(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn unpack(
        bytes: &[u8],
        layout: Layout,
        max_unpacked: u64,
    ) -> Result<Unpacked, VerificationError> {
        unpack_from(Cursor::new(bytes), layout, max_unpacked)
    }

    /// A zip with the given entries, in order. A name ending in `/` is a folder; a
    /// symlink's bytes are its UTF-8 target.
    pub(crate) fn zip_of(entries: &[(&str, FileMode, &[u8])], comment: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, mode, bytes) in entries {
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            if name.ends_with('/') {
                writer.add_directory(*name, options).unwrap();
                continue;
            }
            match mode {
                FileMode::Symlink => {
                    let target = std::str::from_utf8(bytes).unwrap();
                    writer.add_symlink(*name, target, options).unwrap();
                }
                FileMode::Regular | FileMode::Executable => {
                    let unix = if *mode == FileMode::Executable {
                        0o755
                    } else {
                        0o644
                    };
                    writer
                        .start_file(*name, options.unix_permissions(unix))
                        .unwrap();
                    writer.write_all(bytes).unwrap();
                }
            }
        }
        writer.set_comment(comment).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn a_wrapped_archive_loses_its_top_folder() {
        let zip = zip_of(
            &[
                ("en_tn/", FileMode::Regular, b""),
                ("en_tn/metadata.json", FileMode::Regular, b"{}"),
                ("en_tn/ingredients/", FileMode::Regular, b""),
                ("en_tn/ingredients/TIT.tsv", FileMode::Regular, b"x"),
                ("en_tn/run.sh", FileMode::Executable, b"#!"),
                ("en_tn/link", FileMode::Symlink, b"run.sh"),
            ],
            "c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5",
        );
        let unpacked = unpack(&zip, Layout::GitArchive, 1 << 20).unwrap();
        let paths: Vec<_> = unpacked.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["ingredients/TIT.tsv", "link", "metadata.json", "run.sh"]
        );
        assert_eq!(unpacked.files[1].mode, FileMode::Symlink);
        assert_eq!(unpacked.files[2].mode, FileMode::Regular);
        assert_eq!(unpacked.files[3].mode, FileMode::Executable);
        assert_eq!(unpacked.folders.iter().collect::<Vec<_>>(), ["ingredients"]);
        assert_eq!(
            unpacked.comment,
            b"c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5"
        );
    }

    #[test]
    fn an_unwrapped_burrito_keeps_its_root() {
        let zip = zip_of(
            &[
                ("metadata.json", FileMode::Regular, b"{}"),
                ("ingredients/TIT.tsv", FileMode::Regular, b"x"),
            ],
            "",
        );
        let unpacked = unpack(&zip, Layout::Burrito, 1 << 20).unwrap();
        assert_eq!(unpacked.files[1].path, "metadata.json");
        assert_eq!(
            unpack(&zip, Layout::GitArchive, 1 << 20).unwrap_err(),
            VerificationError::NotWrapped
        );
    }

    #[test]
    fn a_second_top_folder_is_not_a_wrapper() {
        let zip = zip_of(
            &[
                ("a/x", FileMode::Regular, b"1"),
                ("b/y", FileMode::Regular, b"2"),
            ],
            "",
        );
        assert_eq!(
            unpack(&zip, Layout::GitArchive, 1 << 20).unwrap_err(),
            VerificationError::NotWrapped
        );
    }

    #[test]
    fn tc4_zip_slip_cases_are_refused() {
        for name in [
            "../evil", "/abs", "C:x", "C:/x", "c:\\x", "a/.. /b", "a/.../b", "a\\..\\b", "a\\b",
            "a//b", "./a", "a/./b", "a\0b",
        ] {
            assert!(escapes_root(name), "{name:?}");
        }
        // A colon behind more than one letter, or deeper in the path, names no drive. Such
        // a name cannot escape, though Windows cannot write it (see `tc_core::check_path`).
        for name in ["a/b", "a/", "a/b.c", "a/..b", "a/b..", "xy:z/c:d"] {
            assert!(!escapes_root(name), "{name:?}");
        }
    }

    fn wrapped(names: &[&str]) -> Vec<u8> {
        let entries: Vec<(String, FileMode, &[u8])> = names
            .iter()
            .map(|n| (format!("repo/{n}"), FileMode::Regular, &b"x"[..]))
            .collect();
        let entries: Vec<(&str, FileMode, &[u8])> = entries
            .iter()
            .map(|(n, m, b)| (n.as_str(), *m, *b))
            .collect();
        zip_of(&entries, "")
    }

    #[test]
    fn names_that_windows_or_macos_would_merge_are_refused() {
        let nfc = "notes/caf\u{e9}.tsv";
        let nfd = "notes/cafe\u{301}.tsv";
        for pair in [
            ["notes/TIT.tsv", "notes/tit.tsv"],
            ["notes/σ", "notes/ς"],
            ["notes/µ", "notes/μ"],
            [nfc, nfd],
            ["Notes/x", "notes/y"],
            ["notes", "Notes/y"],
            ["A", "a/x"],
            ["ÉTÉ/x", "été/y"],
        ] {
            assert!(
                matches!(
                    unpack(&wrapped(&pair), Layout::GitArchive, 1 << 20),
                    Err(VerificationError::CollidingNames { .. })
                ),
                "{pair:?}"
            );
        }
        let error = unpack(
            &wrapped(&["notes/TIT.tsv", "notes/tit.tsv"]),
            Layout::Burrito,
            1 << 20,
        )
        .unwrap_err();
        assert_eq!(
            error,
            VerificationError::CollidingNames {
                first: "notes/TIT.tsv".into(),
                second: "notes/tit.tsv".into()
            }
        );
        // Names that stay apart everywhere are kept.
        let apart = [
            "notes/a",
            "notes/a.b",
            "notes/ab",
            "notes/b/a",
            nfc,
            "Ñandú/x",
            // No supported file system merges the dotless ı with i.
            "notes/ı",
            "notes/i",
        ];
        assert!(unpack(&wrapped(&apart), Layout::GitArchive, 1 << 20).is_ok());
    }

    #[test]
    fn the_public_check_refuses_what_unpacking_refuses() {
        let problem = |paths: &[&str]| check_names(paths.iter().copied()).unwrap_err();
        assert!(matches!(
            problem(&["notes/ok.json", "CON.json"]),
            NameProblem::NotPortable { path, .. } if path == "CON.json"
        ));
        assert!(matches!(
            problem(&["nul.json"]),
            NameProblem::NotPortable { .. }
        ));
        for pair in [
            ["café.md", "cafe\u{301}.md"],
            ["Notes/a.json", "notes/b.json"],
            ["A", "a/x"],
        ] {
            assert!(
                matches!(problem(&pair), NameProblem::Colliding { .. }),
                "{pair:?}"
            );
        }
        assert_eq!(
            problem(&["a/x", "a/x"]),
            NameProblem::Duplicate("a/x".into())
        );
        assert_eq!(
            problem(&["TIT.tsv", "tit.tsv"]),
            NameProblem::Colliding {
                first: "TIT.tsv".into(),
                second: "tit.tsv".into()
            }
        );
        assert_eq!(
            VerificationError::from(problem(&["TIT.tsv", "tit.tsv"])),
            VerificationError::CollidingNames {
                first: "TIT.tsv".into(),
                second: "tit.tsv".into()
            }
        );
        check_names(["a/x", "a/y", "a/b/z", "ı", "i", "notes~1.json"]).unwrap();
        check_names([]).unwrap();
    }

    #[test]
    fn names_windows_cannot_write_are_refused() {
        // Once the top folder is gone, `a:b` names drive A on Windows.
        assert_eq!(
            unpack(&wrapped(&["a:b"]), Layout::GitArchive, 1 << 20).unwrap_err(),
            VerificationError::EscapesRoot("a:b".into())
        );
        for name in [
            "ab:c",
            "x/c:d",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            "a?b",
            "a*b",
            "a\u{1}b",
            "a\tb",
            "CON",
            "con",
            "nul.txt",
            "x/Aux.tsv",
            "COM1",
            "lpt9.md",
            "COM\u{b9}",
            "CONIN$",
            "con .txt",
            "PRN.",
            ". ",
            "x/. /y",
            "notes/a.",
            "notes/a ",
            "notes/a. .",
            "notes./x",
            "x/a ",
            "notes/book.tsv ",
            "PROGRA~1",
            "progra~1",
            "ABCDEF~1.JSN",
            "x/abcdef~1.tsv",
            "abcde~10",
            "A~9",
        ] {
            assert!(
                matches!(
                    unpack(&wrapped(&[name]), Layout::GitArchive, 1 << 20),
                    Err(VerificationError::NotPortable { .. })
                ),
                "{name:?}"
            );
        }
        for name in [
            "CONTENT.md",
            "com10",
            "lpt",
            "console/x",
            "nul_x",
            "a.b.c",
            ".hidden",
            "notes~2024-draft.md",
            "notes~1.json",
            "abcdef~12x",
            "abcdef~12",
            "a~1.tar.gz",
            "a~b",
            "abcdefg~1",
            "~1",
        ] {
            assert!(tc_core::check_path(name).is_ok(), "{name:?}");
        }
    }

    #[test]
    fn finder_and_export_debris_is_dropped_from_a_burrito_only() {
        let entries: [(&str, FileMode, &[u8]); 6] = [
            ("en_tn/metadata.json", FileMode::Regular, b"{}"),
            ("en_tn/ingredients/TIT.tsv", FileMode::Regular, b"x"),
            ("en_tn/.git/HEAD", FileMode::Regular, b"ref"),
            ("en_tn/ingredients/.DS_Store", FileMode::Regular, b"junk"),
            (
                "__MACOSX/en_tn/._metadata.json",
                FileMode::Regular,
                b"\0\x05\x16\x07",
            ),
            ("en_tn/._TIT.tsv", FileMode::Regular, b"\0\x05\x16\x07"),
        ];
        let zip = zip_of(&entries, "");
        let burrito = unpack(&zip, Layout::Burrito, 1 << 20).unwrap();
        let paths: Vec<_> = burrito.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["ingredients/TIT.tsv", "metadata.json"]);
        // In a git archive every entry is content, and these sit in two top folders.
        assert_eq!(
            unpack(&zip, Layout::GitArchive, 1 << 20).unwrap_err(),
            VerificationError::NotWrapped
        );
    }

    #[test]
    fn a_zip_slip_entry_refuses_the_whole_archive() {
        let zip = zip_of(
            &[
                ("repo/ok", FileMode::Regular, b"1"),
                ("repo/../../evil", FileMode::Regular, b"2"),
            ],
            "",
        );
        assert_eq!(
            unpack(&zip, Layout::GitArchive, 1 << 20).unwrap_err(),
            VerificationError::EscapesRoot("repo/../../evil".into())
        );
    }

    #[test]
    fn a_flipped_byte_fails_the_crc_check() {
        let content = vec![b'a'; 4000];
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("repo/file", stored).unwrap();
        writer.write_all(&content).unwrap();
        let mut zip = writer.finish().unwrap().into_inner();
        let at = zip.windows(100).position(|w| w == &content[..100]).unwrap() + 50;
        zip[at] = b'b';
        assert!(matches!(
            unpack(&zip, Layout::GitArchive, 1 << 20),
            Err(VerificationError::CorruptEntry { .. })
        ));
    }

    #[test]
    fn the_unpacked_size_is_capped() {
        let big = vec![0u8; 10_000];
        let zip = zip_of(&[("repo/big", FileMode::Regular, &big)], "");
        assert_eq!(
            unpack(&zip, Layout::GitArchive, 9_999).unwrap_err(),
            VerificationError::TooLargeUnpacked { limit: 9_999 }
        );
        assert!(unpack(&zip, Layout::GitArchive, 10_000).is_ok());
    }

    /// The zip crate keeps one entry per name, so `repo/b` renamed to `repo/a` in the bytes
    /// would vanish, or replace the first, unless the reader counts the records itself.
    #[test]
    fn a_name_given_twice_in_the_zip_is_refused() {
        let mut zip = zip_of(
            &[
                ("repo/a", FileMode::Regular, b"1"),
                ("repo/b", FileMode::Regular, b"2"),
            ],
            "",
        );
        for at in 0..zip.len() {
            if zip[at..].starts_with(b"PK\x01\x02") || zip[at..].starts_with(b"PK\x03\x04") {
                let name = at + if zip[at + 2] == 1 { 46 } else { 30 };
                if zip[name..].starts_with(b"repo/b") {
                    zip[name + 5] = b'a';
                }
            }
        }
        assert_eq!(
            unpack(&zip, Layout::GitArchive, 1 << 20).unwrap_err(),
            VerificationError::NotAZip("two entries have one name".into())
        );
    }

    #[test]
    fn duplicate_names_are_refused() {
        let zip = zip_of(
            &[
                ("repo/a", FileMode::Regular, b"1"),
                ("repo/a/", FileMode::Regular, b""),
            ],
            "",
        );
        assert_eq!(
            unpack(&zip, Layout::GitArchive, 1 << 20).unwrap_err(),
            VerificationError::DuplicateEntry("a".into())
        );
    }

    #[test]
    fn garbage_is_not_a_zip() {
        assert!(matches!(
            unpack(b"not a zip at all", Layout::GitArchive, 1 << 20),
            Err(VerificationError::NotAZip(_))
        ));
    }

    /// Lowercase segments with no trailing dot and no device name, so that no two distinct
    /// paths collide and every path is portable.
    fn segment() -> impl Strategy<Value = String> {
        "[a-z0-9_][a-z0-9_.-]{0,8}".prop_filter("portable and apart", |s| {
            !s.contains("..") && !s.ends_with('.') && tc_core::check_path(s).is_ok()
        })
    }

    fn file_tree() -> impl Strategy<Value = Vec<(String, FileMode, Vec<u8>)>> {
        let mode = prop_oneof![
            Just(FileMode::Regular),
            Just(FileMode::Executable),
            Just(FileMode::Symlink)
        ];
        proptest::collection::btree_map(
            proptest::collection::vec(segment(), 1..4).prop_map(|s| s.join("/")),
            (mode, proptest::collection::vec(any::<u8>(), 0..64)),
            1..12,
        )
        .prop_filter("no file is also a folder", |map| {
            map.keys()
                .all(|k| !map.keys().any(|other| other.starts_with(&format!("{k}/"))))
        })
        .prop_map(|map| {
            map.into_iter()
                .map(|(path, (mode, bytes))| {
                    // A zip symlink stores its target as text.
                    let bytes = if mode == FileMode::Symlink {
                        bytes
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>()
                            .into_bytes()
                    } else {
                        bytes
                    };
                    (path, mode, bytes)
                })
                .collect()
        })
    }

    proptest! {
        #[test]
        fn unpack_returns_exactly_what_was_zipped(files in file_tree()) {
            let named: Vec<(String, FileMode, Vec<u8>)> = files
                .iter()
                .map(|(p, m, b)| (format!("top/{p}"), *m, b.clone()))
                .collect();
            let entries: Vec<(&str, FileMode, &[u8])> =
                named.iter().map(|(p, m, b)| (p.as_str(), *m, b.as_slice())).collect();
            let unpacked = unpack(&zip_of(&entries, ""), Layout::GitArchive, 1 << 20).unwrap();
            let got: Vec<(String, FileMode, Vec<u8>)> = unpacked
                .files
                .into_iter()
                .map(|f| (f.path, f.mode, f.bytes))
                .collect();
            prop_assert_eq!(got, files);
        }

        #[test]
        fn a_path_and_its_uppercase_twin_are_refused(files in file_tree(), pick in any::<prop::sample::Index>()) {
            let (path, mode, bytes) = &files[pick.index(files.len())];
            let twin = path.to_uppercase();
            prop_assume!(twin != *path);
            let mut named: Vec<(String, FileMode, Vec<u8>)> = files
                .iter()
                .map(|(p, m, b)| (format!("top/{p}"), *m, b.clone()))
                .collect();
            named.push((format!("top/{twin}"), *mode, bytes.clone()));
            let entries: Vec<(&str, FileMode, &[u8])> =
                named.iter().map(|(p, m, b)| (p.as_str(), *m, b.as_slice())).collect();
            let refused = unpack(&zip_of(&entries, ""), Layout::GitArchive, 1 << 20);
            prop_assert!(
                matches!(refused, Err(VerificationError::CollidingNames { .. })),
                "{:?}", refused
            );
        }

        #[test]
        fn any_dot_dot_segment_is_refused(prefix in segment(), dots in "\\.\\.[. ]{0,3}", suffix in segment()) {
            let name = format!("{prefix}/{dots}/{suffix}");
            prop_assert!(escapes_root(&name));
        }
    }
}
