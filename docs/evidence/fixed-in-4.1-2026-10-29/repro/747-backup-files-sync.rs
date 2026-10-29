//! #747 repro: a Door43 repository without the backup ignore rule, with a reserved tC4
//! ingredient (`ingredients/tc4-sync/`) and backup files left in the project folder by another
//! program. Does any `.bak` reach Door43's `main`? Control: the same with the standard rule.
//!
//! Goes at crates/tc-sync/tests/repro_747.rs. Run:
//!   tc-heavy cargo test -p tc-sync --test repro_747 repro_747 -- --nocapture

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code, unused_imports)]

#[path = "sync/door43.rs"]
mod door43;

use door43::*;
use std::path::Path;
use tc_door43_fake::seeded;

fn door43_commit(door43: &Door43, name: &str, path: &str, bytes: Option<&[u8]>) {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    let head = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let mut update = git2::build::TreeUpdateBuilder::new();
    match bytes {
        Some(bytes) => {
            let blob = bare.blob(bytes).unwrap();
            update.upsert(path, blob, git2::FileMode::Blob);
        }
        None => {
            update.remove(path);
        }
    }
    let tree = bare
        .find_tree(update.create_updated(&bare, &head.tree().unwrap()).unwrap())
        .unwrap();
    let sig = git2::Signature::now("owner", "owner@example.org").unwrap();
    bare.commit(Some("refs/heads/main"), &sig, &sig, "web edit", &tree, &[&head])
        .unwrap();
}

/// Every blob path on Door43's `main`, and `.gitignore`'s bytes.
fn door43_main(door43: &Door43, name: &str) -> (Vec<String>, Option<String>) {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    let tree = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    let mut paths = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            paths.push(format!("{dir}{}", entry.name().unwrap_or_default()));
        }
        git2::TreeWalkResult::Ok
    })
    .unwrap();
    let ignore = tree
        .get_path(Path::new(".gitignore"))
        .ok()
        .map(|e| String::from_utf8_lossy(bare.find_blob(e.id()).unwrap().content()).into_owned());
    (paths, ignore)
}

fn all_files(root: &Path, rel: &str, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(root.join(rel)).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() { name } else { format!("{rel}/{name}") };
        if path == ".git" {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            all_files(root, &path, out);
        } else {
            out.push(path);
        }
    }
}

async fn scenario(name: &str, strip_rule: bool) {
    let door43 = door43(name).await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, name);
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    // Another tool rewrites the repository: no backup ignore rule, and a reserved tC4
    // ingredient (tC4 4.0's exchange folder).
    if strip_rule {
        door43_commit(&door43, name, ".gitignore", Some(b"/.tc/\n"));
    }
    door43_commit(
        &door43,
        name,
        "ingredients/tc4-sync/0001.json",
        Some(b"{\"reserved\":true}"),
    );
    let (_, ignore) = door43_main(&door43, name);
    println!("#747 {name}: Door43 .gitignore before Juan opens: {ignore:?}");

    // Juan opens the project from Door43 (a second computer).
    let juan = computer(dir.path(), "juan");
    let repo: tc_door43::RepoRef = format!("{}/{name}", seeded::USER).parse().unwrap();
    let folder_b = tc_sync::join(&juan.store, &door43.client, Some(&door43.session), &repo, "Titus")
        .await
        .unwrap()
        .folder;
    let b = open(&juan, &folder_b);
    println!(
        "#747 {name}: Juan's .gitignore on disk: {:?}",
        std::fs::read_to_string(folder_b.join(".gitignore")).ok()
    );
    // Backup files another program leaves in the folder (an editor's, a tC4-style delete's).
    let baks = [
        "ingredients/TIT.usfm.bak",
        "ingredients/tc4-sync/0001.json.bak",
        "metadata.json.bak",
        "ingredients/checking/settings.json.bak",
    ];
    for bak in baks {
        let p = folder_b.join(bak);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, format!("backup bytes of {bak}")).unwrap();
    }
    tokio::task::block_in_place(|| b.focus().wait().unwrap());
    b.wait_idle();
    save(&b, 1, 1, "Paul, as Juan has it");
    let report = sync(&b, &door43).await;
    synced(&report);
    let (paths, ignore) = door43_main(&door43, name);
    let on_door43: Vec<&String> = paths.iter().filter(|p| p.ends_with(".bak")).collect();
    println!("#747 {name}: Door43 .gitignore after Juan's sync: {ignore:?}");
    println!("#747 {name}: .bak paths on Door43 main: {on_door43:?}");
    println!(
        "#747 {name}: tc4-sync on Door43 main: {:?}",
        paths.iter().filter(|p| p.contains("tc4-")).collect::<Vec<_>>()
    );
    let mut local = Vec::new();
    all_files(&folder_b, "", &mut local);
    let kept: Vec<&String> = local.iter().filter(|p| p.contains(".bak")).collect();
    println!("#747 {name}: backup files still on Juan's computer: {kept:?}");
    assert!(on_door43.is_empty(), "a backup file reached Door43: {on_door43:?}");
    assert_eq!(kept.len(), baks.len(), "every backup file is kept on the computer");
    assert_eq!(
        words(&b, 1, 1).as_deref(),
        Some("Paul, as Juan has it"),
        "the save is kept"
    );
    b.close().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repro_747_no_backup_rule_publishes_no_bak() {
    scenario("titus-norule", true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repro_747_control_standard_rule_publishes_no_bak() {
    scenario("titus-rule", false).await;
}
