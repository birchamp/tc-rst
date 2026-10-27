//! Exchange by file (`docs/STORAGE.md` §10): two computers that never meet Door43.

use crate::door43::{computer, computer_on, copy_to, open, save, titus, words};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tc_sync::exchange::{ExchangeError, export, receive};

fn usb(dir: &Path) -> PathBuf {
    let stick = dir.join("stick");
    std::fs::create_dir_all(&stick).unwrap();
    stick
}

/// Two computers exchange files both ways: each ends with the other's verse.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usb_both_directions() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let a = open(&ana, &folder_a);
    let b = open(&juan, &folder_b);
    save(&a, 1, 1, "Paul, as Ana has it");
    save(&b, 1, 2, "in hope, as Juan has it");
    let from_ana = export(&a, &stick, "Ana").await.unwrap();
    let report = receive(&b, &from_ana, "Ana (file of today)").await.unwrap();
    assert!(report.intake.packs > 0, "{report:?}");
    assert!(report.received_by.values().sum::<u32>() > 0, "{report:?}");
    assert_eq!(words(&b, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    let from_juan = export(&b, &stick, "Juan").await.unwrap();
    receive(&a, &from_juan, "Juan (file of today)")
        .await
        .unwrap();
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    assert_eq!(words(&a, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    a.close().unwrap();
    b.close().unwrap();
    let usfm = "ingredients/TIT.usfm";
    assert_eq!(
        std::fs::read(folder_a.join(usfm)).unwrap(),
        std::fs::read(folder_b.join(usfm)).unwrap()
    );
}

/// Receiving a file again adds nothing, and a file relayed through a third computer carries
/// everyone's work.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exchange_idempotent_and_relayed() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let maria = computer(dir.path(), "maria");
    let folder_c = copy_to(&folder_a, &maria);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    let from_ana = export(&a, &stick, "Ana").await.unwrap();
    let b = open(&juan, &folder_b);
    receive(&b, &from_ana, "Ana").await.unwrap();
    let again = receive(&b, &from_ana, "Ana").await.unwrap();
    assert_eq!(again.intake.packs, 0, "{again:?}");
    assert!(again.received_by.is_empty(), "{again:?}");
    assert!(again.new_conflicts.is_empty(), "{again:?}");
    save(&b, 1, 2, "in hope, as Juan has it");
    let relayed = export(&b, &stick, "Juan").await.unwrap();
    let c = open(&maria, &folder_c);
    receive(&c, &relayed, "Juan").await.unwrap();
    assert_eq!(words(&c, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    assert_eq!(words(&c, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    for handle in [a, b, c] {
        handle.close().unwrap();
    }
}

/// `rulings/2026-10-22-H22.md` part 3: a book changed by hand inside an exchange file is not
/// taken in and nothing is kept; the project keeps its text, the file is not changed, and the
/// report names the file once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hand_edited_file_in_an_exchange_file_is_not_taken_in_and_named_once() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    let exported = export(&a, &stick, "Ana").await.unwrap();
    // Someone edits the book inside the file with another program.
    let edited = stick.join("edited.zip");
    rewrite_entry(&exported, &edited, "ingredients/TIT.usfm", |text| {
        text.replace("Paul, as Ana has it", "Paul, as someone typed it")
    });
    let before = std::fs::read(&edited).unwrap();
    let b = open(&juan, &folder_b);
    let report = receive(&b, &edited, "Ana").await.unwrap();
    assert_eq!(report.hand_edited, ["ingredients/TIT.usfm"], "{report:?}");
    assert_eq!(words(&b, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    assert_eq!(
        std::fs::read(&edited).unwrap(),
        before,
        "the file is not changed"
    );
    let kept = walk(&folder_b)
        .into_iter()
        .filter(|p| p.contains(".tc/external") || p.contains(".tc/kept"))
        .count();
    assert_eq!(kept, 0, "nothing is kept");
    a.close().unwrap();
    b.close().unwrap();
}

/// R2: a file of another project is refused, and nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_of_another_project_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    let exported = export(&a, &stick, "Ana").await.unwrap();
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let other = titus(&juan);
    let b = open(&juan, &other);
    let result = receive(&b, &exported, "Ana").await;
    assert!(
        matches!(result, Err(ExchangeError::OtherProject)),
        "{result:?}"
    );
    b.close().unwrap();
}

/// E2 and E5: the file is a valid burrito with metadata.json first, and holds no `.git`,
/// `.tc`, `.gitignore` or `.gitattributes`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usb_export_is_a_burrito_with_metadata_first() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    let exported = export(&a, &stick, "Ana P\u{e9}rez").await.unwrap();
    a.close().unwrap();
    let name = exported.file_name().unwrap().to_str().unwrap().to_owned();
    assert!(name.ends_with("-AnaPérez.zip"), "{name}");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&exported).unwrap()).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_owned())
        .filter(|n| !n.ends_with('/'))
        .collect();
    assert_eq!(names[0], "metadata.json");
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with(".git") || n.starts_with(".tc/") || n == ".gitattributes"),
        "{names:?}"
    );
    assert!(String::from_utf8_lossy(zip.comment()).starts_with("tc-exchange 1 "));
}

fn rewrite_entry(from: &Path, to: &Path, name: &str, edit: impl Fn(&str) -> String) {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(from).unwrap()).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(to).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let entry_name = entry.name().to_owned();
        if entry.is_dir() {
            out.add_directory(entry_name.as_str(), options).unwrap();
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if entry_name == name {
            bytes = edit(&String::from_utf8(bytes).unwrap()).into_bytes();
        }
        out.start_file(entry_name.as_str(), options).unwrap();
        out.write_all(&bytes).unwrap();
    }
    out.finish().unwrap();
}

fn walk(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out
}

/// R4: one damaged pack in the file has its readable changes salvaged, and every other pack
/// is still taken.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loss_usb_one_bad_pack_rest_taken() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    a.checkpoint(tc_store::CheckpointReason::Other("first".into()))
        .await
        .unwrap();
    save(&a, 1, 2, "in hope, as Ana has it");
    let exported = export(&a, &stick, "Ana").await.unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&exported).unwrap()).unwrap();
    let mut packs: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_owned())
        .filter(|n| n.starts_with("ingredients/tc/changes/") && n.ends_with(".ndjson"))
        .collect();
    packs.sort();
    assert!(packs.len() >= 2, "{packs:?}");
    // Damage the pack with Ana's 1:1 (Juan lacks it), so 1:2 must come from the intact one.
    let mut text_of = |name: &str| {
        let mut bytes = String::new();
        zip.by_name(name)
            .unwrap()
            .read_to_string(&mut bytes)
            .unwrap();
        bytes
    };
    let damaged = packs
        .iter()
        .find(|p| {
            let text = text_of(p);
            text.contains("Paul, as Ana has it") && !text.contains("in hope, as Ana has it")
        })
        .unwrap()
        .clone();
    let cut = stick.join("cut.zip");
    rewrite_entry(&exported, &cut, &damaged, |text| {
        text[..text.len() - 20].to_owned()
    });
    let b = open(&juan, &folder_b);
    let report = receive(&b, &cut, "Ana").await.unwrap();
    assert_eq!(report.intake.salvaged, 1, "{report:?}");
    assert!(report.intake.packs >= 1, "{report:?}");
    assert_eq!(words(&b, 1, 2).as_deref(), Some("in hope, as Ana has it"));
    assert!(report.hand_edited.is_empty(), "{report:?}");
    a.close().unwrap();
    b.close().unwrap();
}

/// A file received into a project that cannot be written (here, a folder a cloud client syncs)
/// is refused, and the report never names work that was not taken.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_received_into_a_read_only_project_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    let exported = export(&a, &stick, "Ana").await.unwrap();
    a.close().unwrap();
    let juan = computer_on(
        dir.path(),
        "juan",
        tc_store::FakePlatform {
            host: "juan".into(),
            machine: "machine-of-juan".into(),
            facts: tc_store::FolderFacts {
                unsafe_folder: Some(tc_store::UnsafeFolder::CloudSynced {
                    client: "OneDrive".into(),
                }),
                network: false,
            },
            ..tc_store::FakePlatform::default()
        },
    );
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    let result = receive(&b, &exported, "Ana").await;
    assert!(
        matches!(result, Err(ExchangeError::NotCommitted(_))),
        "{result:?}"
    );
    b.close().unwrap();
}

/// R2: a newcomer creates the project from a teammate's file; it opens with the team's work
/// and a device of its own, and files go both ways after.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_newcomer_creates_the_project_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    let exported = export(&a, &stick, "Ana").await.unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = tc_sync::exchange::receive_new(&juan.store, &exported, "Titus").unwrap();
    let b = open(&juan, &folder_b);
    assert_eq!(words(&b, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    assert_ne!(b.facts().device, a.facts().device);
    save(&b, 1, 2, "in hope, as Juan has it");
    let from_juan = export(&b, &stick, "Juan").await.unwrap();
    receive(&a, &from_juan, "Juan").await.unwrap();
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    a.close().unwrap();
    b.close().unwrap();
    let usfm = "ingredients/TIT.usfm";
    assert_eq!(
        std::fs::read(folder_a.join(usfm)).unwrap(),
        std::fs::read(folder_b.join(usfm)).unwrap()
    );
}

/// L83 (§10.2 R4): a carried file whose bytes changed on the stick (a bit flip) fails its CRC.
/// It is named as damaged and never taken in: nothing in Juan's project records it as an
/// edit, and nothing of it reaches his folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loss_usb_damaged_carried_entry_not_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let a = open(&ana, &folder_a);
    let song = "ingredients/audio/TIT_01_01.mp3";
    let audio: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect();
    std::fs::create_dir_all(folder_a.join("ingredients/audio")).unwrap();
    std::fs::write(folder_a.join(song), &audio).unwrap();
    tokio::task::block_in_place(|| a.focus().wait().unwrap());
    a.wait_idle();
    save(&a, 1, 1, "Paul, as Ana has it");
    let exported = export(&a, &stick, "Ana").await.unwrap();
    // The same file with the audio stored as it is, so one byte of it can be flipped.
    let stored = stick.join("stored.zip");
    {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&exported).unwrap()).unwrap();
        let mut out = zip::ZipWriter::new(std::fs::File::create(&stored).unwrap());
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).unwrap();
            let name = entry.name().to_owned();
            if entry.is_dir() {
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            out.start_file(name.as_str(), options).unwrap();
            out.write_all(&bytes).unwrap();
        }
        out.finish().unwrap();
    }
    let mut bytes = std::fs::read(&stored).unwrap();
    let at = bytes
        .windows(64)
        .position(|w| w == &audio[1000..1064])
        .unwrap();
    bytes[at] ^= 0x01;
    let flipped = stick.join("flipped.zip");
    std::fs::write(&flipped, &bytes).unwrap();
    let b = open(&juan, &folder_b);
    let report = receive(&b, &flipped, "Ana").await.unwrap();
    assert_eq!(report.damaged, vec![song.to_owned()], "{report:?}");
    assert_eq!(
        words(&b, 1, 1).as_deref(),
        Some("Paul, as Ana has it"),
        "the rest is taken"
    );
    b.wait_idle();
    assert!(
        !folder_b.join(song).exists() || std::fs::read(folder_b.join(song)).unwrap() == audio,
        "the damaged bytes never reach the folder"
    );
    let mut changed = audio.clone();
    changed[1000] ^= 0x01;
    let damaged_oid = git2::Oid::hash_object(git2::ObjectType::Blob, &changed)
        .unwrap()
        .to_string();
    let snapshot = b.snapshot();
    for placement in snapshot.fold().carried_files() {
        assert_ne!(placement.oid.to_string(), damaged_oid, "{placement:?}");
        let made_by = snapshot.fold().change(&placement.head).unwrap().device;
        assert_ne!(made_by, b.facts().device, "recorded as Juan's edit");
    }
    a.close().unwrap();
    b.close().unwrap();
}

/// INV-BOTH-RECORDINGS-STAY (delta §2.14, `rulings/2026-10-22-H19.md`): Ana and Juan each
/// replace the same recording from the same version; when Ana receives Juan's file, both
/// recordings stay on her disk, hers at the path and Juan's beside it, and both are listed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recording_changed_on_both_sides_keeps_both_files_after_a_receive() {
    let dir = tempfile::tempdir().unwrap();
    let stick = usb(dir.path());
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let song = "ingredients/audio/TIT_01_01.mp3";
    let first: Vec<u8> = (0..4_000u32).map(|i| (i * 13 % 251) as u8).collect();
    let a = open(&ana, &folder_a);
    std::fs::create_dir_all(folder_a.join("ingredients/audio")).unwrap();
    std::fs::write(folder_a.join(song), &first).unwrap();
    tokio::task::block_in_place(|| a.focus().wait().unwrap());
    a.wait_idle();
    a.close().unwrap();
    // Juan starts from the same version of the recording.
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let mine: Vec<u8> = (0..4_000u32).map(|i| (i * 17 % 251) as u8).collect();
    let theirs: Vec<u8> = (0..5_000u32).map(|i| (i * 19 % 251) as u8).collect();
    let a = open(&ana, &folder_a);
    std::fs::write(folder_a.join(song), &mine).unwrap();
    tokio::task::block_in_place(|| a.focus().wait().unwrap());
    a.wait_idle();
    let b = open(&juan, &folder_b);
    std::fs::write(folder_b.join(song), &theirs).unwrap();
    tokio::task::block_in_place(|| b.focus().wait().unwrap());
    b.wait_idle();
    let from_juan = export(&b, &stick, "Juan").await.unwrap();
    receive(&a, &from_juan, "Juan").await.unwrap();
    a.wait_idle();
    let audio: Vec<(String, Vec<u8>)> = std::fs::read_dir(folder_a.join("ingredients/audio"))
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        audio.len(),
        2,
        "INV-BOTH-RECORDINGS-STAY: both recordings stay on disk: {:?}",
        audio.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    assert_eq!(
        std::fs::read(folder_a.join(song)).unwrap(),
        mine,
        "INV-BOTH-RECORDINGS-STAY: Ana's recording keeps its path"
    );
    let copy = audio
        .iter()
        .find(|(name, _)| name != "TIT_01_01.mp3")
        .expect("INV-BOTH-RECORDINGS-STAY: Juan's recording beside Ana's");
    // The store names the other version `<stem>-<oid8>.<ext>` beside the file.
    assert!(
        copy.0.starts_with("TIT_01_01-")
            && copy.0.ends_with(".mp3")
            && copy.0.len() == "TIT_01_01-99dc4d43.mp3".len(),
        "INV-BOTH-RECORDINGS-STAY: the copy is named for its version: {}",
        copy.0
    );
    assert_eq!(
        copy.1, theirs,
        "INV-BOTH-RECORDINGS-STAY: the copy holds Juan's bytes"
    );
    let metadata = std::fs::read_to_string(folder_a.join("metadata.json")).unwrap();
    assert!(
        metadata.contains("TIT_01_01.mp3") && metadata.contains(&copy.0),
        "INV-BOTH-RECORDINGS-STAY: both are listed"
    );
    a.close().unwrap();
    b.close().unwrap();
}
