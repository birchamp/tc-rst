//! tC4 issues fixed upstream by 2026-10-27, tried on the rebuild's sync against
//! `tc-door43-fake` (report of 2026-10-27, area sync-store).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test code: a failed unwrap is a failed test, and tests set up files directly"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tc_door43::{
    Credentials, Door43Client, InternetSwitch, NewRepoOwner, Password, Session, TokenScope,
    Visibility,
};
use tc_door43_fake::{FakeDoor43, seeded};
use tc_journal::command::{Command, SeedSource};
use tc_journal::fold::ProjectKind;
use tc_journal::record::{Scope, VerseValue, VersificationValue};
use tc_journal::{BookId, ChapterNo, MetaField, MetaPath, Record, RecordKey, VerseNo};
use tc_store::{
    CheckpointReason, FakePlatform, ManualClock, NewProject, OpenOptions, ProjectHandle, Store,
    StoreConfig,
};
use tc_sync::Pause;
use tc_sync::machine::Ended;

struct NoPause;

impl Pause for NoPause {
    async fn pause(&self, _attempt: u8) {}
}

fn store(dir: &Path, name: &str) -> (Store, PathBuf) {
    let projects = dir.join(format!("projects-{name}"));
    std::fs::create_dir_all(&projects).unwrap();
    let store = Store::start(StoreConfig {
        app_data: dir.join(format!("app-{name}")),
        projects: projects.clone(),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(FakePlatform {
            host: name.to_owned(),
            machine: format!("machine-of-{name}"),
            ..FakePlatform::default()
        }),
        clock: Arc::new(ManualClock::at(1_792_195_200_000)),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    (store, projects)
}

fn json(text: &str) -> tc_journal::Json {
    tc_journal::json::parse(text.as_bytes()).unwrap()
}

/// A Titus project seeded from the ULT, with a versification frame or without one.
fn titus(store: &Store, frame: bool) -> PathBuf {
    let meta = |field: MetaField, value: &str| Record::ProjectMeta {
        field,
        value: Some(json(value)),
    };
    let extra = |path: &str| MetaField::Extra(MetaPath::parse(path).unwrap());
    let mut records = vec![
        meta(MetaField::Name, r#"{"en":"Titus"}"#),
        meta(MetaField::Abbreviation, r#"{"en":"tit"}"#),
        meta(
            MetaField::Languages,
            r#"[{"name":{"en":"English"},"scriptDirection":"ltr","tag":"en"}]"#,
        ),
        meta(extra("meta.dateCreated"), r#""2026-10-17T00:00:00.000Z""#),
        meta(extra("meta.defaultLocale"), r#""en""#),
    ];
    if frame {
        records.push(Record::Versification {
            value: VersificationValue {
                name: Some("eng".into()),
                bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
            },
        });
    }
    let usfm = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/tc4/test-fixtures/en_ult/TIT.usfm"),
    )
    .unwrap();
    store
        .create(&NewProject {
            name: "Titus".into(),
            kind: ProjectKind::Bible,
            records,
            books: vec![(BookId::Titus, Scope::Whole, SeedSource { usfm })],
        })
        .unwrap()
}

fn open(store: &Store, folder: &Path) -> ProjectHandle {
    let handle = store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    handle
}

fn verse(v: u16) -> RecordKey {
    RecordKey::Verse(
        BookId::Titus,
        ChapterNo::new(1).unwrap(),
        VerseNo::new(v).unwrap(),
    )
}

fn save(handle: &ProjectHandle, v: u16, words: &str) {
    let shown = handle.mint(&verse(v), None).unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: words.to_owned(),
        })
        .wait()
        .unwrap();
}

fn words(handle: &ProjectHandle, v: u16) -> Option<String> {
    match handle.snapshot().fold().projected(&verse(v)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        }) => Some(words),
        _ => None,
    }
}

struct Door43 {
    fake: FakeDoor43,
    client: Door43Client,
    session: Session,
}

async fn door43(name: &str) -> Door43 {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let switch = InternetSwitch::new();
    switch.turn_on();
    let client = Door43Client::new(fake.url().parse().unwrap(), switch).unwrap();
    let credentials =
        Credentials::new(seeded::USER, Password::new(seeded::PASSWORD.to_owned())).unwrap();
    let session = client
        .mint_token(
            &credentials,
            &"translationCore".parse().unwrap(),
            &TokenScope::FOR_SHARING,
        )
        .await
        .unwrap();
    client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &name.parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap();
    Door43 {
        fake,
        client,
        session,
    }
}

fn repo(name: &str) -> tc_door43::RepoRef {
    format!("{}/{name}", seeded::USER).parse().unwrap()
}

async fn push(handle: &ProjectHandle, door43: &Door43, name: &str) {
    tc_sync::connect(handle, door43.client.server(), &repo(name), "main")
        .await
        .unwrap();
    synced(handle, door43).await;
}

async fn synced(handle: &ProjectHandle, door43: &Door43) {
    let report = tc_sync::sync(handle, &door43.client, Some(&door43.session), &NoPause).await;
    assert!(
        matches!(
            report.ended,
            Ended::Synced {
                confirmed: true,
                ..
            }
        ),
        "{report:?}"
    );
}

/// A commit on Door43's `main` made by another tool (a web edit): `path` written, or removed.
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
    bare.commit(
        Some("refs/heads/main"),
        &sig,
        &sig,
        "web edit",
        &tree,
        &[&head],
    )
    .unwrap();
}

fn on_door43(door43: &Door43, name: &str, path: &str) -> bool {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    let tree = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    tree.get_path(Path::new(path)).is_ok()
}

/// After the open from Door43: a save, then a mode-switch checkpoint, which must commit.
fn save_and_checkpoint_commit(handle: &ProjectHandle, words_now: &str) {
    save(handle, 1, words_now);
    let report = handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    assert!(
        report.commit.is_some() && report.skipped.is_none(),
        "the checkpoint after the open commits: {report:?}"
    );
    save(handle, 2, "and a second save");
    let report = handle
        .checkpoint(CheckpointReason::LeavingProject)
        .wait()
        .unwrap();
    assert!(
        report.commit.is_some(),
        "every later checkpoint commits too: {report:?}"
    );
    assert_eq!(words(handle, 1).as_deref(), Some(words_now));
}

/// tC4 #823 "Open from Door43: does a project with no vrs.json or no frame refuse every
/// checkpoint?", path 1: a project with a book and no versification frame is on Door43, and a
/// teammate opens it.
///
/// Steps: Ana makes a Titus project with no versification frame, shares it to Door43; Juan
/// opens it from Door43, saves a verse, switches mode (a checkpoint), saves again and leaves
/// (a checkpoint). Every checkpoint must commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_823_a_project_with_no_versification_frame_opened_from_door43_still_checkpoints() {
    let door43 = door43("titus-no-frame").await;
    let dir = tempfile::tempdir().unwrap();
    let (ana, _) = store(dir.path(), "ana");
    let folder_a = titus(&ana, false);
    let a = open(&ana, &folder_a);
    assert!(
        a.snapshot()
            .fold()
            .projected(&RecordKey::Versification)
            .is_none(),
        "the project has no frame"
    );
    push(&a, &door43, "titus-no-frame").await;
    a.close().unwrap();

    let (juan, _) = store(dir.path(), "juan");
    let folder_b = tc_sync::join(
        &juan,
        &door43.client,
        Some(&door43.session),
        &repo("titus-no-frame"),
        "Titus",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    save_and_checkpoint_commit(&b, "Paul, as Juan has it");
    synced(&b, &door43).await;
    b.close().unwrap();
}

/// tC4 #823 path 2: a journal that has a frame, in a repository whose `vrs.json` is missing.
///
/// Steps: Ana shares a Titus project (with its frame and `ingredients/vrs.json`); someone
/// deletes `ingredients/vrs.json` on Door43 with another tool; Juan opens it from Door43,
/// saves, switches mode, saves and leaves. Every checkpoint must commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_823_a_repository_without_vrs_json_opened_from_door43_still_checkpoints() {
    let door43 = door43("titus-no-vrs").await;
    let dir = tempfile::tempdir().unwrap();
    let (ana, _) = store(dir.path(), "ana");
    let folder_a = titus(&ana, true);
    let a = open(&ana, &folder_a);
    push(&a, &door43, "titus-no-vrs").await;
    a.close().unwrap();
    assert!(on_door43(&door43, "titus-no-vrs", "ingredients/vrs.json"));
    door43_commit(&door43, "titus-no-vrs", "ingredients/vrs.json", None);
    assert!(!on_door43(&door43, "titus-no-vrs", "ingredients/vrs.json"));

    let (juan, _) = store(dir.path(), "juan");
    let folder_b = tc_sync::join(
        &juan,
        &door43.client,
        Some(&door43.session),
        &repo("titus-no-vrs"),
        "Titus",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    save_and_checkpoint_commit(&b, "Paul, as Juan has it");
    synced(&b, &door43).await;
    b.close().unwrap();
}
