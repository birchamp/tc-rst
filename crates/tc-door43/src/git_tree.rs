//! The git tree id of a set of files, computed the way git computes it.
//!
//! A blob id is `sha1("blob <len>\0" + bytes)`. A tree lists its entries as
//! `<mode> <name>\0<20-byte id>`, sorted by name with a folder's name compared as if it
//! ended in `/`, and its id is `sha1("tree <len>\0" + entries)`. When the files of an
//! archive hash to the tree a commit names, the archive holds exactly that commit's
//! content, byte for byte.

use crate::error::VerificationError;
use crate::unpack::{FileMode, UnpackedFile};
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, BTreeSet};
use tc_core::GitOid;

/// The id git gives an object of `kind` holding `content`.
fn object_id(kind: &str, content: &[u8]) -> GitOid {
    let mut hasher = Sha1::new();
    hasher.update(format!("{kind} {}\0", content.len()).as_bytes());
    hasher.update(content);
    GitOid::from_bytes(hasher.finalize().into())
}

enum Node {
    Blob { mode: FileMode, id: GitOid },
    Tree(BTreeMap<String, Node>),
}

/// The tree id of `files`. `folders` are the folders the archive names; git keeps no
/// empty folder, so an empty one means the archive is not a commit's content (a
/// submodule appears this way in `git archive` output).
pub(crate) fn tree_id(
    files: &[UnpackedFile],
    folders: &BTreeSet<String>,
) -> Result<GitOid, VerificationError> {
    let mut root = BTreeMap::new();
    for file in files {
        insert(&mut root, file)?;
    }
    for folder in folders {
        let has_content = files.iter().any(|f| {
            f.path.starts_with(folder.as_str()) && f.path[folder.len()..].starts_with('/')
        });
        if !has_content {
            return Err(VerificationError::NotGitContent {
                path: folder.clone(),
                reason: "an empty folder, which git cannot hold (a submodule?)",
            });
        }
    }
    Ok(hash_tree(&root))
}

fn insert(root: &mut BTreeMap<String, Node>, file: &UnpackedFile) -> Result<(), VerificationError> {
    let clash = || VerificationError::NotGitContent {
        path: file.path.clone(),
        reason: "a file and a folder share this name",
    };
    let mut segments: Vec<&str> = file.path.split('/').collect();
    let Some(name) = segments.pop() else {
        return Err(clash());
    };
    let mut tree = root;
    for segment in segments {
        let node = tree
            .entry(segment.to_owned())
            .or_insert_with(|| Node::Tree(BTreeMap::new()));
        tree = match node {
            Node::Tree(children) => children,
            Node::Blob { .. } => return Err(clash()),
        };
    }
    let blob = Node::Blob {
        mode: file.mode,
        id: object_id("blob", &file.bytes),
    };
    if tree.insert(name.to_owned(), blob).is_some() {
        return Err(clash());
    }
    Ok(())
}

fn hash_tree(tree: &BTreeMap<String, Node>) -> GitOid {
    let mut entries: Vec<(Vec<u8>, &'static str, GitOid, &str)> = tree
        .iter()
        .map(|(name, node)| {
            let (mode, id, sort_suffix) = match node {
                Node::Blob { mode, id } => (blob_mode(*mode), *id, ""),
                Node::Tree(children) => ("40000", hash_tree(children), "/"),
            };
            let mut key = name.as_bytes().to_vec();
            key.extend_from_slice(sort_suffix.as_bytes());
            (key, mode, id, name.as_str())
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut content = Vec::new();
    for (_, mode, id, name) in entries {
        content.extend_from_slice(mode.as_bytes());
        content.push(b' ');
        content.extend_from_slice(name.as_bytes());
        content.push(0);
        content.extend_from_slice(id.as_bytes());
    }
    object_id("tree", &content)
}

fn blob_mode(mode: FileMode) -> &'static str {
    match mode {
        FileMode::Regular => "100644",
        FileMode::Executable => "100755",
        FileMode::Symlink => "120000",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, mode: FileMode, bytes: &[u8]) -> UnpackedFile {
        UnpackedFile {
            path: path.into(),
            mode,
            bytes: bytes.to_vec(),
        }
    }

    // Ids below were checked with git 2.43 (`git hash-object`).
    #[test]
    fn a_blob_hashes_like_git() {
        assert_eq!(
            object_id("blob", b"hello\n").to_string(),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn the_empty_tree_hashes_like_git() {
        assert_eq!(
            tree_id(&[], &BTreeSet::new()).unwrap().to_string(),
            "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
        );
    }

    #[test]
    fn a_folder_sorts_as_if_it_ended_in_a_slash() {
        // `a.b` sorts before the folder `a` (`a/`), because '.' < '/'; a plain name sort
        // would put `a` first and give another id.
        let files = [
            file("a/x", FileMode::Regular, b"x"),
            file("a.b", FileMode::Regular, b"y"),
        ];
        let by_git_order = tree_id(&files, &BTreeSet::new()).unwrap();
        let reversed = [files[1].clone(), files[0].clone()];
        assert_eq!(tree_id(&reversed, &BTreeSet::new()).unwrap(), by_git_order);
    }

    #[test]
    fn an_empty_folder_is_not_git_content() {
        let folders = BTreeSet::from(["vendor/lib".to_owned()]);
        assert!(matches!(
            tree_id(&[file("README.md", FileMode::Regular, b"x")], &folders),
            Err(VerificationError::NotGitContent { .. })
        ));
    }

    #[test]
    fn the_mode_changes_the_tree() {
        let regular = tree_id(&[file("run", FileMode::Regular, b"x")], &BTreeSet::new());
        let exec = tree_id(&[file("run", FileMode::Executable, b"x")], &BTreeSet::new());
        let link = tree_id(&[file("run", FileMode::Symlink, b"x")], &BTreeSet::new());
        assert_ne!(regular, exec);
        assert_ne!(regular, link);
        assert_ne!(exec, link);
    }
}
