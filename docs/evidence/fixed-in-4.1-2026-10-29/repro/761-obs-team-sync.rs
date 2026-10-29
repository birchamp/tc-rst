//! #761 repro: an Open Bible Stories project syncs with Door43 teammates through the Bible
//! sync (tc_sync::sync / join, the same code path). Two actors edit different frames; both edit
//! one frame (Q2: a conflict for review); front matter edited by another program (Q3); a third
//! computer opens it from Door43; story files are byte-identical everywhere.
//!
//! Goes at crates/tc-sync/tests/repro_761.rs. Run:
//!   tc-heavy cargo test -p tc-sync --test repro_761 repro_761 -- --nocapture

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code, unused_imports)]

#[path = "sync/door43.rs"]
mod door43;

use door43::*;
use std::path::{Path, PathBuf};
use tc_door43_fake::seeded;
use tc_journal::command::Command;
use tc_journal::fold::ProjectKind;
use tc_journal::{MetaField, Record, RecordKey};
use tc_store::{NewProject, ProjectHandle};

fn stories(computer: &Computer) -> PathBuf {
    let json = |t: &str| tc_journal::json::parse(t.as_bytes()).unwrap();
    let mut records = vec![
        Record::ProjectMeta {
            field: MetaField::Name,
            value: Some(json(r#"{"en":"Stories"}"#)),
        },
        Record::ProjectMeta {
            field: MetaField::Languages,
            value: Some(json(
                r#"[{"name":{"en":"English"},"scriptDirection":"ltr","tag":"en"}]"#,
            )),
        },
    ];
    for story in tc_obs::ids::StoryNo::all() {
        records.push(Record::StoryBase {
            story: tc_journal::StoryNo::new(story.get()).unwrap(),
            value: tc_obs::story::seed_story(tc_obs::template::template_story(story)).unwrap(),
        });
    }
    computer
        .store
        .create(&NewProject {
            name: "Stories".into(),
            kind: ProjectKind::Stories,
            records,
            books: Vec::new(),
        })
        .unwrap()
}

fn frame_key(story: u8, frame: u16) -> RecordKey {
    RecordKey::Frame(
        tc_journal::StoryNo::new(story).unwrap(),
        tc_journal::FrameNo::new(frame),
    )
}

fn save_frame(handle: &ProjectHandle, story: u8, frame: u16, text: &str) {
    let shown = handle.mint(&frame_key(story, frame), None).unwrap();
    handle
        .submit(Command::SaveFrame {
            shown,
            text: text.to_owned(),
        })
        .wait()
        .unwrap();
}

fn frame_text(handle: &ProjectHandle, story: u8, frame: u16) -> String {
    match handle.snapshot().fold().projected(&frame_key(story, frame)) {
        Some(Record::Frame { value, .. }) => format!("{value:?}"),
        other => format!("{other:?}"),
    }
}

fn content(folder: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    fn walk(root: &Path, rel: &str, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(root.join(rel)).unwrap() {
            let e = e.unwrap();
            let p = format!("{rel}/{}", e.file_name().to_string_lossy());
            if e.file_type().unwrap().is_dir() {
                walk(root, &p, out);
            } else {
                let bytes = std::fs::read(root.join(&p)).unwrap();
                out.insert(p, bytes);
            }
        }
    }
    walk(folder, "ingredients/content", &mut out);
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repro_761_obs_team_sync_round_2() {
    let name = "en_obs_team";
    let door43 = door43(name).await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = stories(&ana);
    let a = open(&ana, &folder_a);
    let front = "ingredients/content/front/intro.md";
    println!(
        "#761 a new story project has front matter on disk: {}",
        folder_a.join(front).exists()
    );
    connect(&a, &door43, name);
    synced(&sync(&a, &door43).await);
    let repo: tc_door43::RepoRef = format!("{}/{name}", seeded::USER).parse().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = tc_sync::join(&juan.store, &door43.client, Some(&door43.session), &repo, "Stories")
        .await
        .unwrap()
        .folder;
    let b = open(&juan, &folder_b);
    assert_eq!(b.facts().kind, ProjectKind::Stories);

    // Different frames.
    save_frame(&a, 1, 3, "Frame 3, as Ana has it.");
    save_frame(&b, 1, 4, "Frame 4, as Juan has it.");
    // The same frame, two ways (Q2).
    save_frame(&a, 1, 5, "Frame 5, as Ana has it.");
    save_frame(&b, 1, 5, "Frame 5, as Juan has it.");
    // Front matter, edited by another program on Ana's computer (Q3).
    let intro = b"# Introduction, as Ana wrote it\n".to_vec();
    std::fs::create_dir_all(folder_a.join(front).parent().unwrap()).unwrap();
    std::fs::write(folder_a.join(front), &intro).unwrap();
    tokio::task::block_in_place(|| a.focus().wait().unwrap());
    a.wait_idle();

    synced(&sync(&a, &door43).await);
    let rb = sync(&b, &door43).await;
    synced(&rb);
    println!("#761 Juan's sync: new_conflicts={:?}", rb.new_conflicts);
    let ra = sync(&a, &door43).await;
    synced(&ra);
    println!("#761 Ana's second sync: new_conflicts={:?}", ra.new_conflicts);
    synced(&sync(&b, &door43).await);

    for (who, h) in [("Ana", &a), ("Juan", &b)] {
        println!("#761 {who} 1:3 = {}", frame_text(h, 1, 3));
        println!("#761 {who} 1:4 = {}", frame_text(h, 1, 4));
        println!("#761 {who} 1:5 = {}", frame_text(h, 1, 5));
        let conflicts: Vec<String> = h
            .snapshot()
            .fold()
            .conflicts()
            .map(|c| format!("{:?} {:?} candidates={}", c.kind, c.key, c.candidates.len()))
            .collect();
        println!("#761 {who} open conflicts: {conflicts:?}");
        assert!(frame_text(h, 1, 3).contains("as Ana has it"), "{who} 1:3");
        assert!(frame_text(h, 1, 4).contains("as Juan has it"), "{who} 1:4");
        assert_eq!(conflicts.len(), 1, "{who}: the same frame is one conflict for review");
        assert!(conflicts[0].contains("Frame"), "{who}: {conflicts:?}");
    }
    assert_eq!(std::fs::read(folder_b.join(front)).unwrap(), intro, "front matter synced");
    let story_a = std::fs::read_to_string(folder_a.join("ingredients/content/01.md")).unwrap();
    assert!(story_a.contains("as Juan has it") && story_a.contains("as Ana has it"));

    // A third computer opens it from Door43.
    let carla = computer(dir.path(), "carla");
    let folder_c = tc_sync::join(&carla.store, &door43.client, Some(&door43.session), &repo, "Stories")
        .await
        .unwrap()
        .folder;
    let c = open(&carla, &folder_c);
    a.close().unwrap();
    b.close().unwrap();
    c.close().unwrap();
    let (ca, cb, cc) = (content(&folder_a), content(&folder_b), content(&folder_c));
    let differ: Vec<&String> = ca
        .keys()
        .filter(|k| ca.get(*k) != cb.get(*k) || ca.get(*k) != cc.get(*k))
        .collect();
    println!(
        "#761 content files: {} on Ana, {} on Juan, {} on Carla; differing: {differ:?}",
        ca.len(),
        cb.len(),
        cc.len()
    );
    assert!(differ.is_empty() && ca.len() == cb.len() && ca.len() == cc.len());
}
