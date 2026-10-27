// Crate test target: tc-sync, `--test sync`. Append to crates/tc-sync/tests/sync/door43.rs
// (it uses that module's private helpers), run
//   cargo test -p tc-sync --test sync -- repro_821 --nocapture
// then restore door43.rs.
//
// tC4 #821: Open from Door43 of (1) a repository in tC4's team format (a journal, no
// ingredients/tc/format.json) and (2) a rebuild repository whose ingredients/vrs.json is gone.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repro_821_open_from_door43_of_a_tc4_repo_and_of_one_without_vrs() {
    let dir = tempfile::tempdir().unwrap();
    for (name, case) in [("titus-821-tc4", 1), ("titus-821-novrs", 2)] {
        let door43 = door43(name).await;
        let ana = computer(dir.path(), &format!("ana{case}"));
        let folder = titus(&ana);
        let a = open(&ana, &folder);
        save(&a, 1, 1, "Paul");
        connect(&a, &door43, name);
        synced(&sync(&a, &door43).await);
        a.close().unwrap();
        if case == 1 {
            door43_commit(&door43, name, "ingredients/tc/format.json", None);
            door43_commit(&door43, name, "ingredients/checking/journal/0001.json", Some(b"{}"));
        } else {
            door43_commit(&door43, name, "ingredients/vrs.json", None);
        }
        let juan = computer(dir.path(), &format!("juan{case}"));
        let before: Vec<_> = std::fs::read_dir(&juan.projects).unwrap().map(|e| e.unwrap().file_name()).filter(|n| !n.to_string_lossy().starts_with(".")).collect(); // a staging folder is removed at the next start (§6.4 N1)
        let repo: tc_door43::RepoRef = format!("{}/{name}", seeded::USER).parse().unwrap();
        let joined = tc_sync::join(&juan.store, &door43.client, Some(&door43.session), &repo, "Titus").await;
        let after: Vec<_> = std::fs::read_dir(&juan.projects).unwrap().map(|e| e.unwrap().file_name()).filter(|n| !n.to_string_lossy().starts_with(".")).collect(); // a staging folder is removed at the next start (§6.4 N1)
        println!("CASE {case} ({name}): join -> {joined:?}; projects before {before:?} after {after:?}");
        if case == 1 {
            assert!(matches!(joined, Err(tc_sync::JoinError::Blocked(_))), "{joined:?}");
            assert_eq!(before, after, "nothing placed");
        } else if let Ok(folder_b) = joined {
            let b = open(&juan, &folder_b);
            println!("CASE 2: opened; 1:1 = {:?}; vrs.json on disk now: {}", words(&b, 1, 1), folder_b.join("ingredients/vrs.json").exists());
            save(&b, 1, 2, "Juan was here");
            println!("CASE 2: saved 1:2 = {:?}", words(&b, 1, 2));
            b.close().unwrap();
            let problems = tc_burrito::validate_dir(&folder_b);
            println!("CASE 2: burrito problems after the open: {problems:?}");
        }
    }
}
