//! Which files from outside (a Door43 tree, an exchange file) are written to disk. Owner
//! ruling "carry, skip on disk": a name that could leave the folder refuses the whole join or
//! file; so does a file the app owns whose name Windows cannot write, or two owned names one
//! file system stores as one file. A foreign file with such a name stays in the history and
//! in the exchange file, byte for byte, but is written on no computer, so every computer's
//! copy matches; the caller lists it.

use std::collections::{BTreeMap, BTreeSet};
use tc_door43::NameProblem;

/// Why names from outside refuse a whole join or exchange file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameRefusal {
    /// The name could leave the folder, or it is the app's own and Windows cannot write it.
    Unsafe { path: String, problem: NameProblem },
    /// Two of the app's own names that Windows or macOS stores as one file.
    Colliding { first: String, second: String },
}

/// Whether the app owns `path` (relative to the project folder): `metadata.json`, the git
/// settings it writes, everything under `ingredients/` and its own `.tc*` folders.
/// `.tc-moved/` holds foreign entries (STORAGE §3.1).
pub(crate) fn owned(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or_default();
    matches!(path, "metadata.json" | ".gitignore" | ".gitattributes")
        || first == "ingredients"
        || (first.starts_with(".tc") && first != ".tc-moved")
}

/// The paths among `paths` that are not written on disk: each foreign file whose name
/// Windows cannot write as named, and every foreign file of a set of names Windows or macOS
/// stores as one (none of them is kept over the others).
pub(crate) fn not_written<'a>(
    paths: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeSet<String>, NameRefusal> {
    let mut skipped = BTreeSet::new();
    let mut kept = Vec::new();
    for path in paths {
        match tc_door43::name_problem(path) {
            None => kept.push(path),
            Some(problem @ NameProblem::EscapesRoot(_)) => {
                return Err(NameRefusal::Unsafe {
                    path: path.to_owned(),
                    problem,
                });
            }
            Some(problem) if owned(path) => {
                return Err(NameRefusal::Unsafe {
                    path: path.to_owned(),
                    problem,
                });
            }
            Some(_) => {
                skipped.insert(path.to_owned());
            }
        }
    }
    // Per collision key, each spelling (a file, or a folder a path implies) with the paths
    // that give it. Several paths may imply one folder spelled alike: that is no collision.
    let mut by_key: BTreeMap<String, BTreeMap<(&str, bool), Vec<&str>>> = BTreeMap::new();
    for path in kept {
        let folders = path.match_indices('/').map(|(at, _)| (&path[..at], true));
        for spelling in std::iter::once((path, false)).chain(folders) {
            by_key
                .entry(tc_door43::collision_key(spelling.0))
                .or_default()
                .entry(spelling)
                .or_default()
                .push(path);
        }
    }
    for spellings in by_key.values().filter(|s| s.len() > 1) {
        let owned_spellings: Vec<&(&str, bool)> = spellings
            .iter()
            .filter(|(_, paths)| paths.iter().any(|p| owned(p)))
            .map(|(spelling, _)| spelling)
            .collect();
        if let [first, second, ..] = owned_spellings[..] {
            return Err(NameRefusal::Colliding {
                first: first.0.to_owned(),
                second: second.0.to_owned(),
            });
        }
        for (spelling, paths) in spellings {
            if owned_spellings.first() != Some(&spelling) {
                skipped.extend(paths.iter().map(|p| (*p).to_owned()));
            }
        }
    }
    Ok(skipped)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "test code: a failed unwrap is a failed test"
)]
mod tests {
    use super::*;

    fn skipped(paths: &[&str]) -> Vec<String> {
        not_written(paths.iter().copied())
            .unwrap()
            .into_iter()
            .collect()
    }

    #[test]
    fn a_foreign_name_windows_cannot_write_is_skipped() {
        assert_eq!(
            skipped(&["metadata.json", ".gitea/con.txt", "notes~1.md", "README.md"]),
            [".gitea/con.txt", "notes~1.md"]
        );
    }

    #[test]
    fn colliding_foreign_names_are_all_skipped() {
        assert_eq!(skipped(&["A.md", "a.md", "b.md"]), ["A.md", "a.md"]);
        assert_eq!(
            skipped(&["Notes/x.md", "notes/y.md", "notes/z.md"]),
            ["Notes/x.md", "notes/y.md", "notes/z.md"]
        );
        // A foreign name that meets an owned one gives way to it.
        assert_eq!(
            skipped(&[
                "ingredients/TIT.usfm",
                "Ingredients/x.md",
                "METADATA.json",
                "metadata.json"
            ]),
            ["Ingredients/x.md", "METADATA.json"]
        );
    }

    #[test]
    fn owned_names_stay_strict() {
        assert!(matches!(
            not_written(["ingredients/con.usfm"]),
            Err(NameRefusal::Unsafe { path, problem: NameProblem::NotPortable { .. } })
                if path == "ingredients/con.usfm"
        ));
        assert_eq!(
            not_written(["ingredients/A.usfm", "ingredients/a.usfm"]),
            Err(NameRefusal::Colliding {
                first: "ingredients/A.usfm".into(),
                second: "ingredients/a.usfm".into(),
            })
        );
    }

    #[test]
    fn a_name_that_leaves_the_folder_refuses_foreign_or_owned() {
        for path in ["../x", ".gitea/../../x", "a\\b", "c:x", "/x", "a/.. /b"] {
            assert!(
                matches!(
                    not_written([path]),
                    Err(NameRefusal::Unsafe {
                        problem: NameProblem::EscapesRoot(_),
                        ..
                    })
                ),
                "{path:?}"
            );
        }
    }
}
