//! Syncs against `tc-door43-fake`: two computers, each with a store on a real folder, and one
//! Door43 repository.

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
    FakePlatform, ManualClock, NewProject, OpenOptions, ProjectHandle, Store, StoreConfig,
};
use tc_sync::machine::{Ended, Received};
use tc_sync::{Pause, SyncReport};

/// Waits nothing between rounds and counts them.
#[derive(Default)]
pub struct NoPause;

impl Pause for NoPause {
    async fn pause(&self, _attempt: u8) {}
}

pub struct Computer {
    pub store: Store,
    pub projects: PathBuf,
}

impl Drop for Computer {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let problems = self.store.hooks().burrito_problems();
        assert!(
            problems.is_empty(),
            "the store wrote a state that is not a valid burrito: {problems:#?}"
        );
    }
}

pub fn computer(dir: &Path, name: &str) -> Computer {
    computer_on(
        dir,
        name,
        FakePlatform {
            host: name.to_owned(),
            machine: format!("machine-of-{name}"),
            ..FakePlatform::default()
        },
    )
}

pub fn computer_on(dir: &Path, name: &str, platform: FakePlatform) -> Computer {
    let projects = dir.join(format!("projects-{name}"));
    std::fs::create_dir_all(&projects).unwrap();
    let store = Store::start(StoreConfig {
        app_data: dir.join(format!("app-{name}")),
        projects: projects.clone(),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(platform),
        clock: Arc::new(ManualClock::at(1_792_195_200_000)),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    store.hooks().check_burrito(|folder: &Path| {
        tc_burrito::validate_dir(folder)
            .into_iter()
            .map(|p| format!("{p:?}"))
            .collect()
    });
    Computer { store, projects }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn json(text: &str) -> tc_journal::Json {
    tc_journal::json::parse(text.as_bytes()).unwrap()
}

/// A Titus project on `computer`, seeded from the ULT.
pub fn titus(computer: &Computer) -> PathBuf {
    let meta = |field: MetaField, value: &str| Record::ProjectMeta {
        field,
        value: Some(json(value)),
    };
    let extra = |path: &str| MetaField::Extra(MetaPath::parse(path).unwrap());
    computer
        .store
        .create(&NewProject {
            name: "Titus".into(),
            kind: ProjectKind::Bible,
            records: vec![
                meta(MetaField::Name, r#"{"en":"Titus"}"#),
                meta(MetaField::Abbreviation, r#"{"en":"tit"}"#),
                meta(
                    MetaField::Languages,
                    r#"[{"name":{"en":"English"},"scriptDirection":"ltr","tag":"en"}]"#,
                ),
                meta(extra("meta.dateCreated"), r#""2026-10-17T00:00:00.000Z""#),
                meta(extra("meta.defaultLocale"), r#""en""#),
                Record::Versification {
                    value: VersificationValue {
                        name: Some("eng".into()),
                        bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
                    },
                },
            ],
            books: vec![(
                BookId::Titus,
                Scope::Whole,
                SeedSource {
                    usfm: std::fs::read_to_string(
                        repo_root().join("fixtures/tc4/test-fixtures/en_ult/TIT.usfm"),
                    )
                    .unwrap(),
                },
            )],
        })
        .unwrap()
}

pub fn open(computer: &Computer, folder: &Path) -> ProjectHandle {
    let handle = computer
        .store
        .open(folder, &OpenOptions::default())
        .unwrap();
    handle.wait_idle();
    handle
}

fn verse(c: u16, v: u16) -> RecordKey {
    RecordKey::Verse(
        BookId::Titus,
        ChapterNo::new(c).unwrap(),
        VerseNo::new(v).unwrap(),
    )
}

pub fn save(handle: &ProjectHandle, c: u16, v: u16, words: &str) {
    let shown = handle.mint(&verse(c, v), None).unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: words.to_owned(),
        })
        .wait()
        .unwrap();
}

pub fn words(handle: &ProjectHandle, c: u16, v: u16) -> Option<String> {
    match handle.snapshot().fold().projected(&verse(c, v)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        }) => Some(words),
        _ => None,
    }
}

pub struct Door43 {
    pub fake: FakeDoor43,
    pub client: Door43Client,
    pub session: Session,
}

/// The fake, signed in as its seeded user, with an empty repository `name` of theirs.
pub async fn door43(name: &str) -> Door43 {
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

/// `origin` and `tc.branch` of the project's repository: what share, join or connect set.
pub fn connect(handle: &ProjectHandle, door43: &Door43, name: &str) {
    connect_to(handle, door43, seeded::USER, name);
}

pub fn connect_to(handle: &ProjectHandle, door43: &Door43, owner: &str, name: &str) {
    let repo: tc_door43::RepoRef = format!("{owner}/{name}").parse().unwrap();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(tc_sync::connect(
            handle,
            door43.client.server(),
            &repo,
            "main",
        ))
    })
    .unwrap();
}

pub async fn sync(handle: &ProjectHandle, door43: &Door43) -> SyncReport {
    tc_sync::sync(handle, &door43.client, Some(&door43.session), &NoPause).await
}

pub fn synced(report: &SyncReport) {
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

/// A copy of a project folder on another computer, as a USB stick carries it.
pub fn copy_to(from: &Path, to: &Computer) -> PathBuf {
    let into = to.projects.join(from.file_name().unwrap());
    copy_dir(from, &into);
    into
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

fn branch_on_door43(door43: &Door43, name: &str) -> Option<git2::Oid> {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init_bare(dir.path()).unwrap();
    let url = format!(
        "{}/{}/{name}.git",
        door43.fake.url().trim_end_matches('/'),
        seeded::USER
    );
    let mut remote = repo.remote_anonymous(&url).unwrap();
    remote.connect(git2::Direction::Fetch).unwrap();
    remote
        .list()
        .unwrap()
        .iter()
        .find(|h| h.name() == "refs/heads/main")
        .map(|h| h.oid())
}

/// The first sync of a project with an empty repository pushes its history, and Door43's
/// branch is read back at it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_first_sync_pushes_the_project_and_confirms_it() {
    let door43 = door43("titus-first").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let handle = open(&ana, &folder);
    save(&handle, 1, 1, "Paul, a servant of God");
    connect(&handle, &door43, "titus-first");
    let report = sync(&handle, &door43).await;
    synced(&report);
    let git = git2::Repository::open(handle.facts().git_dir.unwrap()).unwrap();
    let head = git.head().unwrap().target().unwrap();
    assert_eq!(branch_on_door43(&door43, "titus-first"), Some(head));
    assert_eq!(report.confirmed, Some(head.to_string()));
    let file = handle.sync_file().await.unwrap().expect(".tc/sync.json");
    assert_eq!(file.branch, "main");
    assert!(file.remote_url.ends_with("/titus-first.git"), "{file:?}");
    assert_eq!(
        file.confirmed.map(|c| c.commit),
        Some(head.to_string()),
        "the confirmed commit is recorded"
    );
    handle.close().unwrap();
}

/// Two computers save different verses and sync in turn: each ends with both verses, and
/// their book files are byte for byte the same.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_computers_converge_through_door43() {
    let door43 = door43("titus-team").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    connect(&a, &door43, "titus-team");
    synced(&sync(&a, &door43).await);
    a.close().unwrap();

    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    let a = open(&ana, &folder_a);
    save(&b, 1, 2, "in hope, as Juan has it");
    save(&a, 1, 3, "in his time, as Ana has it");
    synced(&sync(&b, &door43).await);
    let report = sync(&a, &door43).await;
    synced(&report);
    assert!(
        matches!(
            report.ended,
            Ended::Synced {
                received: Received::Integrated,
                ..
            }
        ),
        "{report:?}"
    );
    assert!(report.received.packs > 0, "{report:?}");
    synced(&sync(&b, &door43).await);
    for handle in [&a, &b] {
        assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, as Ana has it"));
        assert_eq!(
            words(handle, 1, 2).as_deref(),
            Some("in hope, as Juan has it")
        );
        assert_eq!(
            words(handle, 1, 3).as_deref(),
            Some("in his time, as Ana has it")
        );
    }
    a.close().unwrap();
    b.close().unwrap();
    let usfm = "ingredients/TIT.usfm";
    assert_eq!(
        std::fs::read(folder_a.join(usfm)).unwrap(),
        std::fs::read(folder_b.join(usfm)).unwrap()
    );
}

fn head(handle: &ProjectHandle) -> Option<git2::Oid> {
    let git = git2::Repository::open(handle.facts().git_dir.unwrap()).unwrap();
    git.head().ok().and_then(|h| h.target())
}

/// Q8: an account that may read the repository but not push to it is refused before the
/// fetch, and nothing changes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_write_access_is_refused_before_the_fetch_with_nothing_changed() {
    let door43 = door43("unused").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let handle = open(&ana, &folder);
    connect_to(&handle, &door43, seeded::READ_ONLY_ORG, "es-419_tn");
    let before = head(&handle);
    let report = sync(&handle, &door43).await;
    assert_eq!(
        report.ended,
        Ended::Failed(tc_api::refusal::SyncFailure::BeforeIntake(
            tc_api::refusal::SyncRefusal::NoWriteAccess {
                server_message: String::new()
            }
        )),
        "{report:?}"
    );
    assert_eq!(head(&handle), before);
    assert!(
        !door43
            .fake
            .requests()
            .iter()
            .any(|r| r.path.contains("git-upload-pack")),
        "no fetch"
    );
    handle.close().unwrap();
}

/// §6.4 Y1: a fetch cut off part way changes nothing, and the next sync takes the work.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fetch_cut_off_mid_pack_changes_nothing() {
    let door43 = door43("titus-cut").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-cut");
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    save(&b, 1, 2, "in hope, as Juan has it");
    synced(&sync(&b, &door43).await);
    let a = open(&ana, &folder_a);
    door43.fake.inject(tc_door43_fake::FaultRule::once(
        "POST",
        &format!("/{}/titus-cut.git/git-upload-pack", seeded::USER),
        tc_door43_fake::Fault::BreakOff { after: 64 },
    ));
    let report = sync(&a, &door43).await;
    assert!(
        matches!(
            report.ended,
            Ended::Failed(tc_api::refusal::SyncFailure::BeforeIntake(_))
        ),
        "{report:?}"
    );
    assert_eq!(words(&a, 1, 2).as_deref(), Some("___"));
    synced(&sync(&a, &door43).await);
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    a.close().unwrap();
    b.close().unwrap();
}

/// §6.4 Y6: a push whose reply is cut off was applied on Door43 but not confirmed; the next
/// sync finds Door43's branch at this computer's commit and confirms it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_whose_reply_is_cut_off_is_sent_not_confirmed_and_the_next_sync_confirms() {
    let door43 = door43("titus-y6").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-y6");
    save(&a, 1, 1, "Paul, a servant of God");
    door43.fake.inject(tc_door43_fake::FaultRule::once(
        "POST",
        &format!("/{}/titus-y6.git/git-receive-pack", seeded::USER),
        tc_door43_fake::Fault::BreakOff { after: 0 },
    ));
    let report = sync(&a, &door43).await;
    assert!(
        matches!(report.ended, Ended::Failed(_)),
        "the push's answer never came: {report:?}"
    );
    assert_eq!(report.confirmed, None);
    assert_eq!(branch_on_door43(&door43, "titus-y6"), head(&a), "applied");
    let again = sync(&a, &door43).await;
    synced(&again);
    assert_eq!(again.confirmed, head(&a).map(|h| h.to_string()));
    a.close().unwrap();
}

/// §9.5: a teammate pushes while this computer's push waits; Door43 refuses the second push
/// as not a fast-forward, the sync fetches again, merges and pushes, and nothing is lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_that_loses_a_race_fetches_again_and_keeps_both() {
    let door43 = door43("titus-race").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-race");
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    save(&b, 1, 2, "in hope, as Juan has it");
    // Ana's work is committed already and Door43 has something new for her (an edit of a
    // file the app does not own), so the one commit her sync makes is its merge, after the
    // fetch. It waits there while Juan pushes, so Ana's push loses the race. The store's hold
    // stops a known step; the fake's Hold of the push hung under load.
    a.checkpoint(tc_store::CheckpointReason::Other("before the sync".into()))
        .await
        .unwrap();
    door43_commit(&door43, "titus-race", "README.md", Some(b"# Titus\n"));
    let hooks = ana.store.hooks();
    let point = tc_store::Point::BeforeHeadUpdate;
    hooks.hold(point);
    let teammate = async {
        held_at(hooks, point, 1).await;
        let report = sync(&b, &door43).await;
        hooks.release(point);
        report
    };
    let (ana_report, juan_report) = tokio::join!(sync(&a, &door43), teammate);
    synced(&juan_report);
    synced(&ana_report);
    assert!(ana_report.raced >= 1, "{ana_report:?}");
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    synced(&sync(&b, &door43).await);
    assert_eq!(words(&b, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    a.close().unwrap();
    b.close().unwrap();
}

/// gap-tc4-straggler-sync-safety state 3b, owner ruling 2026-10-24 ("recreate it"): Door43's
/// branch was deleted by its owner. The sync pushes this copy as the branch again, as tC4
/// does, and the report says so, so the person can tell the team to sync.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sync_against_a_repository_with_no_main_branch_recreates_it_and_says_so() {
    let door43 = door43("titus-gone").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-gone");
    synced(&sync(&a, &door43).await);
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, "titus-gone")).unwrap();
    bare.find_reference("refs/heads/main")
        .unwrap()
        .delete()
        .unwrap();
    save(&a, 1, 1, "Paul, a servant of God");
    let report = sync(&a, &door43).await;
    synced(&report);
    assert!(report.recreated, "{report:?}");
    let local = git2::Repository::open(a.facts().git_dir.unwrap())
        .unwrap()
        .head()
        .unwrap()
        .target()
        .unwrap();
    assert_eq!(branch_on_door43(&door43, "titus-gone"), Some(local));
    assert!(!sync(&a, &door43).await.recreated, "said once");
    a.close().unwrap();
}

/// §9.6 Join: a teammate gets the project from Door43. It opens with Ana's text and the same
/// book file bytes, and their next syncs carry each other's work.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_joined_project_holds_the_teams_work_and_syncs_both_ways() {
    let door43 = door43("titus-join").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    connect(&a, &door43, "titus-join");
    synced(&sync(&a, &door43).await);

    let juan = computer(dir.path(), "juan");
    let repo: tc_door43::RepoRef = format!("{}/titus-join", seeded::USER).parse().unwrap();
    let folder_b = tc_sync::join(
        &juan.store,
        &door43.client,
        Some(&door43.session),
        &repo,
        "Titus",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    assert_eq!(words(&b, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    assert_ne!(b.facts().device, a.facts().device, "a device of its own");
    let restored: Vec<_> = b
        .notices()
        .into_iter()
        .filter(|n| {
            matches!(
                n.notice,
                tc_journal::fold::Notice::PackRestored { .. }
                    | tc_journal::fold::Notice::FixedFileRestored { .. }
            )
        })
        .collect();
    assert!(
        restored.is_empty(),
        "every file came from the tree: {restored:?}"
    );
    let usfm = "ingredients/TIT.usfm";
    assert_eq!(
        std::fs::read(folder_a.join(usfm)).unwrap(),
        std::fs::read(folder_b.join(usfm)).unwrap()
    );
    let file = b.sync_file().await.unwrap().expect(".tc/sync.json");
    assert_eq!(file.branch, "main");
    save(&b, 1, 2, "in hope, as Juan has it");
    synced(&sync(&b, &door43).await);
    synced(&sync(&a, &door43).await);
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    a.close().unwrap();
    b.close().unwrap();
}

/// §9.6 Share: a new repository, the project connected to it, and its history pushed and
/// confirmed; a second share with the same name is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn share_makes_the_repository_and_pushes_the_project() {
    let door43 = door43("unused-share").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    save(&a, 1, 1, "Paul, a servant of God");
    let name: tc_door43::NewRepoName = "en_tit_ana".parse().unwrap();
    let report = tc_sync::share(
        &a,
        &door43.client,
        &door43.session,
        &NewRepoOwner::SignedInUser,
        &name,
        Visibility::Public,
        &NoPause,
    )
    .await
    .unwrap();
    synced(&report);
    assert_eq!(branch_on_door43(&door43, "en_tit_ana"), head(&a));
    let again = tc_sync::share(
        &a,
        &door43.client,
        &door43.session,
        &NewRepoOwner::SignedInUser,
        &name,
        Visibility::Public,
        &NoPause,
    )
    .await;
    assert!(
        matches!(again, Err(tc_sync::ShareError::NameExists)),
        "{again:?}"
    );
    a.close().unwrap();
}

/// Commits on Door43's branch with no translationCore trailer, as another tool (or Door43's
/// own editor) makes, and the change it makes to `path`.
fn commit_by_another_tool(
    door43: &Door43,
    name: &str,
    path: &str,
    edit: impl Fn(&[u8]) -> Vec<u8>,
) {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    let head = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let tree = head.tree().unwrap();
    let old = bare
        .find_blob(tree.get_path(Path::new(path)).unwrap().id())
        .unwrap()
        .content()
        .to_vec();
    let blob = bare.blob(&edit(&old)).unwrap();
    let mut update = git2::build::TreeUpdateBuilder::new();
    update.upsert(path, blob, git2::FileMode::Blob);
    let tree = bare
        .find_tree(update.create_updated(&bare, &tree).unwrap())
        .unwrap();
    let sig = git2::Signature::now("jdoe", "jdoe@example.org").unwrap();
    bare.commit(
        Some("refs/heads/main"),
        &sig,
        &sig,
        "Edit TIT.usfm",
        &tree,
        &[&head],
    )
    .unwrap();
}

/// `rulings/2026-10-22-H18.md` B and delta L152: a book changed on Door43 with another tool
/// is reported once, by its author and commit; the project keeps its own text, and the
/// sync's merge puts translationCore's file back on Door43.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_door43_edit_by_another_tool_is_reported_once() {
    let door43 = door43("titus-web").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    save(&a, 1, 1, "Paul, a servant of God");
    connect(&a, &door43, "titus-web");
    synced(&sync(&a, &door43).await);
    let ours = std::fs::read(folder.join("ingredients/TIT.usfm")).unwrap();
    commit_by_another_tool(&door43, "titus-web", "ingredients/TIT.usfm", |bytes| {
        String::from_utf8_lossy(bytes)
            .replace("Paul, a servant of God", "Paul, a slave of God")
            .into_bytes()
    });
    let report = sync(&a, &door43).await;
    synced(&report);
    assert_eq!(
        report.other_tools.len(),
        1,
        "INV-ONLY-TRANSLATIONCORE-MERGES: {report:?}"
    );
    assert_eq!(report.other_tools[0].author, "jdoe");
    assert_eq!(report.other_tools[0].books, [BookId::Titus]);
    assert_eq!(words(&a, 1, 1).as_deref(), Some("Paul, a servant of God"));
    assert_eq!(
        std::fs::read(folder.join("ingredients/TIT.usfm")).unwrap(),
        ours
    );
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, "titus-web")).unwrap();
    let tip = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    let on_door43 = bare
        .find_blob(
            tip.get_path(Path::new("ingredients/TIT.usfm"))
                .unwrap()
                .id(),
        )
        .unwrap()
        .content()
        .to_vec();
    assert_eq!(
        on_door43, ours,
        "the merge puts translationCore's file back on Door43"
    );
    let again = sync(&a, &door43).await;
    synced(&again);
    assert!(again.other_tools.is_empty(), "said once: {again:?}");
    a.close().unwrap();
}

/// INV-OWN-WORK-NEVER-REVERTED: both computers change one verse; the sync keeps both
/// versions as a conflict, lists it as new, and says who sent the changes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_versions_of_one_verse_are_a_listed_conflict_after_sync() {
    let door43 = door43("titus-both").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-both");
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    save(&b, 1, 1, "Paul, as Juan has it");
    synced(&sync(&b, &door43).await);
    let report = sync(&a, &door43).await;
    synced(&report);
    assert_eq!(report.new_conflicts.len(), 1, "{report:?}");
    assert!(report.received_by.values().sum::<u32>() > 0, "{report:?}");
    let snapshot = a.snapshot();
    let conflict = snapshot
        .fold()
        .conflicts()
        .next()
        .expect("a conflict")
        .clone();
    let texts: Vec<String> = conflict
        .candidates
        .iter()
        .filter_map(|c| match &c.value {
            Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            } => Some(words.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"Paul, as Ana has it".to_owned()),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"Paul, as Juan has it".to_owned()),
        "{texts:?}"
    );
    a.close().unwrap();
    b.close().unwrap();
}

/// Every file under `ingredients/` and `metadata.json`, by path.
fn project_files(folder: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![folder.join("ingredients")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    out.insert(
        "metadata.json".into(),
        std::fs::read(folder.join("metadata.json")).unwrap(),
    );
    out
}

/// J26: three computers, each with its own saves, converge through Door43 with the same
/// project files byte for byte (INV-SYNC-CONVERGES).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn three_devices_converge_with_byte_identical_derived_files() {
    let door43 = door43("titus-three").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-three");
    synced(&sync(&a, &door43).await);
    let repo: tc_door43::RepoRef = format!("{}/titus-three", seeded::USER).parse().unwrap();
    let mut others = Vec::new();
    for name in ["juan", "maria"] {
        let computer = computer(dir.path(), name);
        let folder = tc_sync::join(
            &computer.store,
            &door43.client,
            Some(&door43.session),
            &repo,
            "Titus",
        )
        .await
        .unwrap();
        others.push((computer, folder));
    }
    let b = open(&others[0].0, &others[0].1);
    let c = open(&others[1].0, &others[1].1);
    save(&a, 1, 1, "Paul, as Ana has it");
    save(&b, 1, 2, "in hope, as Juan has it");
    save(&c, 1, 3, "in his time, as Maria has it");
    for handle in [&a, &b, &c, &a, &b] {
        synced(&sync(handle, &door43).await);
    }
    for handle in [&a, &b, &c] {
        assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, as Ana has it"));
        assert_eq!(
            words(handle, 1, 2).as_deref(),
            Some("in hope, as Juan has it")
        );
        assert_eq!(
            words(handle, 1, 3).as_deref(),
            Some("in his time, as Maria has it")
        );
    }
    a.close().unwrap();
    b.close().unwrap();
    c.close().unwrap();
    let first = project_files(&folder_a);
    for (_, folder) in &others {
        let files = project_files(folder);
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            first.keys().collect::<Vec<_>>()
        );
        for (path, bytes) in &first {
            assert_eq!(&files[path], bytes, "{path} differs");
        }
    }
}

/// A commit on Door43's branch with no translationCore trailer that sets `path` to `bytes`, or
/// removes it.
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

fn on_door43(door43: &Door43, name: &str, path: &str) -> Option<Vec<u8>> {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    let tree = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    let entry = tree.get_path(Path::new(path)).ok()?;
    Some(bare.find_blob(entry.id()).unwrap().content().to_vec())
}

/// §9.4: a file the app does not own (a README, a style guide) is Door43's: an edit made on
/// Door43 stays after a sync, and a file deleted on Door43 stays deleted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_the_app_does_not_own_follows_door43() {
    let door43 = door43("titus-readme").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-readme");
    synced(&sync(&a, &door43).await);
    door43_commit(&door43, "titus-readme", "README.md", Some(b"# Titus\n"));
    save(&a, 1, 1, "Paul");
    synced(&sync(&a, &door43).await);
    assert_eq!(
        on_door43(&door43, "titus-readme", "README.md").as_deref(),
        Some(&b"# Titus\n"[..])
    );
    door43_commit(
        &door43,
        "titus-readme",
        "README.md",
        Some(b"# Titus, by the team\n"),
    );
    save(&a, 1, 2, "in hope");
    synced(&sync(&a, &door43).await);
    assert_eq!(
        on_door43(&door43, "titus-readme", "README.md").as_deref(),
        Some(&b"# Titus, by the team\n"[..]),
        "the edit made on Door43 stays"
    );
    door43_commit(&door43, "titus-readme", "README.md", None);
    save(&a, 1, 3, "in his time");
    synced(&sync(&a, &door43).await);
    assert_eq!(
        on_door43(&door43, "titus-readme", "README.md"),
        None,
        "the file deleted on Door43 stays deleted"
    );
    a.close().unwrap();
}

/// PLAN Q8: the merge fails after the teammate's work was taken in (here, before the branch
/// moves); the sync reports a failure after intake, the work stays, and the next sync ends it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merge_that_fails_after_intake_says_the_work_was_kept() {
    let door43 = door43("titus-q8").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-q8");
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    save(&b, 1, 2, "in hope, as Juan has it");
    synced(&sync(&b, &door43).await);
    let a = open(&ana, &folder_a);
    ana.store
        .hooks()
        .fail(tc_store::Point::BeforeHeadUpdate, tc_store::Failure::Stop);
    let report = sync(&a, &door43).await;
    assert!(
        matches!(
            report.ended,
            Ended::Failed(tc_api::refusal::SyncFailure::AfterIntake(_))
        ),
        "{report:?}"
    );
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    ana.store.hooks().heal(tc_store::Point::BeforeHeadUpdate);
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    b.close().unwrap();
}

fn username_of(handle: &ProjectHandle) -> Option<String> {
    match handle
        .snapshot()
        .fold()
        .projected(&RecordKey::DeviceProfile(handle.facts().device))
    {
        Some(Record::DeviceProfile { value, .. }) => {
            value.door43_username.map(|u| u.as_str().to_owned())
        }
        _ => None,
    }
}

/// A session of another account; `record_username` reads only its name.
fn session_as(user: &str) -> Session {
    Session {
        username: user.parse().unwrap(),
        token: tc_door43::Token::new("a-token-of-another-account".to_owned()).unwrap(),
    }
}

/// Q7 and #680: the username is written once, after a sync and the name notice, and another
/// account signing in later does not change it; without the notice nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn username_written_once_after_first_successful_fetch() {
    let door43 = door43("titus-name").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-name");
    assert!(matches!(
        tc_sync::record_username(&a, &door43.session).await,
        Err(tc_sync::UsernameError::NotSynced)
    ));
    synced(&sync(&a, &door43).await);
    // share_no_username_without_consent: the sync alone writes no name.
    assert_eq!(username_of(&a), None);
    assert!(tc_sync::record_username(&a, &door43.session).await.unwrap());
    assert_eq!(username_of(&a).as_deref(), Some(seeded::USER));
    let other = session_as("juan-p");
    assert!(!tc_sync::record_username(&a, &other).await.unwrap());
    assert_eq!(username_of(&a).as_deref(), Some(seeded::USER), "first wins");
    a.close().unwrap();
}

/// #161: a folder copied to another computer copies the name; the first signed-in sync there
/// replaces it with the account that signs in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copied_username_is_replaced_by_the_first_signed_in_fetch() {
    let door43 = door43("titus-copied").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-copied");
    synced(&sync(&a, &door43).await);
    tc_sync::record_username(&a, &door43.session).await.unwrap();
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    assert_eq!(username_of(&b).as_deref(), Some(seeded::USER), "copied");
    synced(&sync(&b, &door43).await);
    let other = session_as("juan-p");
    assert!(tc_sync::record_username(&b, &other).await.unwrap());
    assert_eq!(username_of(&b).as_deref(), Some("juan-p"));
    b.close().unwrap();
}

/// INV-PUSH-IS-NEVER-FORCED: a push refspec is built from branch names only, so one that
/// would force cannot be made.
#[test]
fn push_is_never_forced() {
    use tc_door43::FastForwardRefspec;
    assert_eq!(
        FastForwardRefspec::branch("main").unwrap().as_str(),
        "refs/heads/main:refs/heads/main"
    );
    for bad in [
        "+main",
        "-main",
        "main:other",
        "ma in",
        "",
        "..",
        "main.lock",
    ] {
        assert!(FastForwardRefspec::branch(bad).is_err(), "{bad:?}");
    }
}

/// §9.2 N1 and #813/#821: a repository still in tC4's team format is blocked before anything
/// is taken or sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tc4_team_repository_is_blocked_and_nothing_changes() {
    let door43 = door43("titus-tc4").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-tc4");
    synced(&sync(&a, &door43).await);
    // A tC4 computer turns the repository back into its team format.
    door43_commit(&door43, "titus-tc4", "ingredients/tc/format.json", None);
    door43_commit(
        &door43,
        "titus-tc4",
        "ingredients/checking/journal/0001.json",
        Some(b"{}"),
    );
    let tip = on_door43_tip(&door43, "titus-tc4");
    save(&a, 1, 1, "Paul");
    let report = sync(&a, &door43).await;
    assert_eq!(
        report.ended,
        Ended::Failed(tc_api::refusal::SyncFailure::Blocked(
            tc_api::refusal::SyncBlock::Tc4TeamRepository
        )),
        "{report:?}"
    );
    assert_eq!(on_door43_tip(&door43, "titus-tc4"), tip, "nothing sent");
    let git = git2::Repository::open(a.facts().git_dir.unwrap()).unwrap();
    let last = git.find_commit(head(&a).unwrap()).unwrap();
    assert_eq!(last.parent_count(), 1, "no merge commit");
    a.close().unwrap();
}

fn on_door43_tip(door43: &Door43, name: &str) -> git2::Oid {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    bare.refname_to_id("refs/heads/main").unwrap()
}

/// TC4-REQUEST-MOVED-NOTICE: `ingredients/tc/moved.json`, written once by the team move, is
/// never rewritten or deleted by a sync.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sync_never_rewrites_or_deletes_moved_json() {
    let door43 = door43("titus-moved").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-moved");
    synced(&sync(&a, &door43).await);
    let moved = br#"{"format":"tc-moved","version":1,"to":"https://git.door43.org/team/titus"}"#;
    door43_commit(
        &door43,
        "titus-moved",
        "ingredients/tc/moved.json",
        Some(moved),
    );
    for v in 1..=2 {
        save(&a, 1, v, "a verse");
        synced(&sync(&a, &door43).await);
        assert_eq!(
            on_door43(&door43, "titus-moved", "ingredients/tc/moved.json").as_deref(),
            Some(&moved[..]),
            "sync {v}"
        );
    }
    a.close().unwrap();
}

/// An empty repository of the seeded user made as Door43 makes one that was migrated or made
/// long ago: its default branch is `master`.
async fn door43_with_master(name: &str, private: bool) -> Door43 {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fake.add_repo(tc_door43_fake::RepoSeed {
        owner: seeded::USER.to_owned(),
        name: name.to_owned(),
        subject: String::new(),
        language: String::new(),
        topics: Vec::new(),
        private,
        commits: Vec::new(),
    })
    .await
    .unwrap();
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
    Door43 {
        fake,
        client,
        session,
    }
}

fn ref_on_door43(door43: &Door43, name: &str, refname: &str) -> Option<git2::Oid> {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    bare.refname_to_id(refname).ok()
}

/// Waits until the store's threads have stopped at `point` `n` times while it was held. A sync
/// that never gets there fails the test instead of hanging it.
async fn held_at(hooks: &tc_store::Hooks, point: tc_store::Point, n: u64) {
    let start = std::time::Instant::now();
    while hooks.reached(point) < n {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(60),
            "the sync never stopped at {point:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

/// Runs a store command that waits for its reply from inside a test's runtime.
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    tokio::task::block_in_place(f)
}

/// L30 (sync-design §9.2): a repository whose default branch is `master` syncs on `master`.
/// The branch is recorded at connect and at join, and neither side makes a `main`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_master_branch() {
    let door43 = door43_with_master("titus-master", false).await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    let repo: tc_door43::RepoRef = format!("{}/titus-master", seeded::USER).parse().unwrap();
    blocking(|| {
        tokio::runtime::Handle::current().block_on(tc_sync::connect(
            &a,
            door43.client.server(),
            &repo,
            "master",
        ))
    })
    .unwrap();
    synced(&sync(&a, &door43).await);
    assert_eq!(
        ref_on_door43(&door43, "titus-master", "refs/heads/master"),
        head(&a)
    );
    assert_eq!(
        ref_on_door43(&door43, "titus-master", "refs/heads/main"),
        None
    );
    let juan = computer(dir.path(), "juan");
    let folder_b = tc_sync::join(
        &juan.store,
        &door43.client,
        Some(&door43.session),
        &repo,
        "Titus",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    assert_eq!(b.sync_file().await.unwrap().unwrap().branch, "master");
    save(&b, 1, 2, "in hope, as Juan has it");
    synced(&sync(&b, &door43).await);
    synced(&sync(&a, &door43).await);
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    assert_eq!(
        ref_on_door43(&door43, "titus-master", "refs/heads/main"),
        None
    );
    a.close().unwrap();
    b.close().unwrap();
}

/// L31 (sync-design §9.1): a private repository syncs and joins, because the fetch and the
/// join send the account's token.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_private_repository() {
    let door43 = door43_with_master("titus-private", true).await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    let repo: tc_door43::RepoRef = format!("{}/titus-private", seeded::USER).parse().unwrap();
    blocking(|| {
        tokio::runtime::Handle::current().block_on(tc_sync::connect(
            &a,
            door43.client.server(),
            &repo,
            "master",
        ))
    })
    .unwrap();
    synced(&sync(&a, &door43).await);
    let juan = computer(dir.path(), "juan");
    let anonymous = tc_sync::join(&juan.store, &door43.client, None, &repo, "Titus").await;
    assert!(anonymous.is_err(), "a private repository is not public");
    let folder_b = tc_sync::join(
        &juan.store,
        &door43.client,
        Some(&door43.session),
        &repo,
        "Titus",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    assert_eq!(words(&b, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    save(&b, 1, 2, "in hope, as Juan has it");
    synced(&sync(&b, &door43).await);
    synced(&sync(&a, &door43).await);
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    let fetches: Vec<_> = door43
        .fake
        .requests()
        .into_iter()
        .filter(|r| r.path.ends_with("/titus-private.git/git-upload-pack"))
        .collect();
    assert!(!fetches.is_empty());
    assert!(
        fetches
            .iter()
            .all(|r| r.authorization != tc_door43_fake::AuthSeen::None),
        "{fetches:?}"
    );
    a.close().unwrap();
    b.close().unwrap();
}

/// Puts a pre-receive hook in Door43's copy of `name` that refuses every push with `text`, as
/// Gitea refuses a push to a protected branch.
fn protect(door43: &Door43, name: &str, text: &str) {
    let hook = door43
        .fake
        .git_dir(seeded::USER, name)
        .join("hooks/pre-receive");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ncat >/dev/null\necho '{text}' >&2\nexit 1\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// L20 (F11) and L34: Door43 takes the pack and then refuses the ref (a protected branch).
/// The push returned no error, yet the sync fails with Door43's own words, nothing reads as
/// on Door43, and the confirmed commit stays the last one Door43 really showed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_protected_branch_reported() {
    let door43 = door43("titus-protected").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-protected");
    synced(&sync(&a, &door43).await);
    let confirmed = a.sync_file().await.unwrap().unwrap().confirmed;
    let tip = on_door43_tip(&door43, "titus-protected");
    let words_of_door43 = "Not allowed to push to protected branch main";
    protect(&door43, "titus-protected", words_of_door43);
    save(&a, 1, 1, "Paul, a servant of God");
    let report = sync(&a, &door43).await;
    match &report.ended {
        Ended::Failed(
            tc_api::refusal::SyncFailure::BeforeIntake(tc_api::refusal::SyncRefusal::RefRejected {
                server_message,
            })
            | tc_api::refusal::SyncFailure::AfterIntake(tc_api::refusal::SyncRefusal::RefRejected {
                server_message,
            }),
        ) => assert!(server_message.contains(words_of_door43), "{server_message}"),
        other => panic!("a refused ref read as {other:?}"),
    }
    assert_eq!(report.confirmed, None, "{report:?}");
    assert_eq!(on_door43_tip(&door43, "titus-protected"), tip);
    assert_ne!(head(&a), Some(tip), "the new work is only here");
    assert_eq!(
        a.sync_file().await.unwrap().unwrap().confirmed,
        confirmed,
        "nothing newer reads as on Door43"
    );
    a.close().unwrap();
}

/// L23 (F14): the person saves while a sync runs (here, while its commit waits to move the
/// branch). The saves are taken at once, the sync ends, and the next sync sends them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loss_f14_edits_during_sync_kept() {
    let door43 = door43("titus-during").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-during");
    save(&a, 1, 1, "Paul, a servant of God");
    let hooks = ana.store.hooks();
    hooks.hold(tc_store::Point::BeforeHeadUpdate);
    let editor = async {
        held_at(hooks, tc_store::Point::BeforeHeadUpdate, 1).await;
        // The saves run beside this thread, so a save that waits for the sync fails the test
        // instead of hanging it.
        blocking(|| {
            std::thread::scope(|scope| {
                let (done, saved) = std::sync::mpsc::channel();
                let a = &a;
                scope.spawn(move || {
                    save(a, 1, 2, "saved while the sync ran");
                    save(a, 1, 1, "Paul, changed while the sync ran");
                    let _ = done.send(());
                });
                let in_time = saved.recv_timeout(std::time::Duration::from_secs(60));
                hooks.release(tc_store::Point::BeforeHeadUpdate);
                assert!(in_time.is_ok(), "a save waited for the sync");
            });
        });
    };
    let (report, ()) = tokio::join!(sync(&a, &door43), editor);
    synced(&report);
    assert_eq!(words(&a, 1, 2).as_deref(), Some("saved while the sync ran"));
    assert_eq!(
        words(&a, 1, 1).as_deref(),
        Some("Paul, changed while the sync ran")
    );
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let repo: tc_door43::RepoRef = format!("{}/titus-during", seeded::USER).parse().unwrap();
    let folder_b = tc_sync::join(
        &juan.store,
        &door43.client,
        Some(&door43.session),
        &repo,
        "Titus",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    assert_eq!(words(&b, 1, 2).as_deref(), Some("saved while the sync ran"));
    assert_eq!(
        words(&b, 1, 1).as_deref(),
        Some("Paul, changed while the sync ran")
    );
    b.close().unwrap();
}

/// L89 (§9.2 N1, N2): `project.json` deleted on Door43 is not another project: the sync
/// goes on and its merge commit puts the file back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_project_json_deleted_on_door43_restored() {
    let door43 = door43("titus-pj").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, "titus-pj");
    synced(&sync(&a, &door43).await);
    let ours = std::fs::read(folder.join("ingredients/tc/project.json")).unwrap();
    door43_commit(&door43, "titus-pj", "ingredients/tc/project.json", None);
    save(&a, 1, 1, "Paul");
    synced(&sync(&a, &door43).await);
    assert_eq!(
        on_door43(&door43, "titus-pj", "ingredients/tc/project.json"),
        Some(ours)
    );
    a.close().unwrap();
}

/// L29 (sync-design §9.1): a file the project carries (an audio recording) travels to
/// Door43 and to a teammate who joins, byte for byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loss_carried_file_travels() {
    let door43 = door43("titus-audio").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    let song = "ingredients/audio/TIT_01_01.mp3";
    let bytes: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect();
    std::fs::create_dir_all(folder_a.join("ingredients/audio")).unwrap();
    std::fs::write(folder_a.join(song), &bytes).unwrap();
    blocking(|| a.focus().wait().unwrap());
    a.wait_idle();
    connect(&a, &door43, "titus-audio");
    synced(&sync(&a, &door43).await);
    assert_eq!(on_door43(&door43, "titus-audio", song), Some(bytes.clone()));
    let juan = computer(dir.path(), "juan");
    let repo: tc_door43::RepoRef = format!("{}/titus-audio", seeded::USER).parse().unwrap();
    let folder_b = tc_sync::join(
        &juan.store,
        &door43.client,
        Some(&door43.session),
        &repo,
        "Titus",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    assert_eq!(std::fs::read(folder_b.join(song)).unwrap(), bytes);
    a.close().unwrap();
    b.close().unwrap();
}

/// An Open Bible Stories project on `computer`, made as a new one is: its details and each
/// story's template base.
fn stories(computer: &Computer) -> PathBuf {
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

fn frame_text(handle: &ProjectHandle, story: u8, frame: u16) -> Option<String> {
    match handle.snapshot().fold().projected(&frame_key(story, frame)) {
        Some(Record::Frame { value, .. }) => Some(format!("{value:?}")),
        _ => None,
    }
}

/// L32 (sync-design §9.1): an Open Bible Stories project shares, joins and syncs both ways
/// as a Bible project does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn team_sync_obs_project() {
    let door43 = door43("en_obs").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = stories(&ana);
    let a = open(&ana, &folder_a);
    save_frame(&a, 1, 1, "This is how God made everything, as Ana has it.");
    connect(&a, &door43, "en_obs");
    synced(&sync(&a, &door43).await);
    let juan = computer(dir.path(), "juan");
    let repo: tc_door43::RepoRef = format!("{}/en_obs", seeded::USER).parse().unwrap();
    let folder_b = tc_sync::join(
        &juan.store,
        &door43.client,
        Some(&door43.session),
        &repo,
        "Stories",
    )
    .await
    .unwrap();
    let b = open(&juan, &folder_b);
    assert_eq!(b.facts().kind, ProjectKind::Stories);
    assert_eq!(frame_text(&b, 1, 1), frame_text(&a, 1, 1));
    assert!(
        frame_text(&b, 1, 1).unwrap().contains("as Ana has it"),
        "{:?}",
        frame_text(&b, 1, 1)
    );
    save_frame(
        &b,
        1,
        2,
        "God created the world in six days, as Juan has it.",
    );
    synced(&sync(&b, &door43).await);
    synced(&sync(&a, &door43).await);
    assert!(
        frame_text(&a, 1, 2)
            .unwrap_or_default()
            .contains("as Juan has it")
    );
    a.close().unwrap();
    b.close().unwrap();
    let story = "ingredients/content/01.md";
    let ours = std::fs::read(folder_a.join(story)).unwrap();
    assert!(String::from_utf8_lossy(&ours).contains("as Juan has it"));
    assert_eq!(ours, std::fs::read(folder_b.join(story)).unwrap());
}

/// The packs on Door43's tip, by path.
fn packs_on_door43(door43: &Door43, name: &str) -> std::collections::BTreeMap<String, Vec<u8>> {
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, name)).unwrap();
    let tree = bare
        .find_reference("refs/heads/main")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    let mut out = std::collections::BTreeMap::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        let path = format!("{dir}{}", entry.name().unwrap_or_default());
        if path.starts_with("ingredients/tc/changes/")
            && entry.kind() == Some(git2::ObjectType::Blob)
        {
            out.insert(path, bare.find_blob(entry.id()).unwrap().content().to_vec());
        }
        git2::TreeWalkResult::Ok
    })
    .unwrap();
    out
}

/// L22 (G2) and L45: a teammate's pack reaches Door43 damaged. Each file is checked alone, so
/// the sync goes on: the pack's readable changes are taken, the rest of the team's work
/// with them, and the damaged pack stays on Door43's tip for its owner to repair.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loss_bad_pack_from_teammate_sync_continues() {
    let door43 = door43("titus-bad").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-bad");
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    let before = packs_on_door43(&door43, "titus-bad");
    save(&b, 1, 2, "in hope, as Juan has it");
    save(&b, 1, 3, "in his time, as Juan has it");
    synced(&sync(&b, &door43).await);
    let juans: Vec<(String, Vec<u8>)> = packs_on_door43(&door43, "titus-bad")
        .into_iter()
        .filter(|(path, _)| !before.contains_key(path))
        .collect();
    assert_eq!(juans.len(), 1, "{juans:?}");
    let (path, bytes) = &juans[0];
    // Door43's copy loses its last bytes: the last change in it no longer reads.
    let damaged = bytes[..bytes.len() - 7].to_vec();
    door43_commit(&door43, "titus-bad", path, Some(&damaged));
    let a = open(&ana, &folder_a);
    save(&a, 1, 1, "Paul, as Ana has it");
    let report = sync(&a, &door43).await;
    synced(&report);
    assert!(report.received.salvaged >= 1, "{report:?}");
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    assert_eq!(
        on_door43(&door43, "titus-bad", path),
        Some(damaged),
        "the damaged pack stays on the tip"
    );
    // Its owner's sync repairs it from the copy only Juan's computer holds whole.
    synced(&sync(&b, &door43).await);
    synced(&sync(&a, &door43).await);
    assert_eq!(
        words(&a, 1, 3).as_deref(),
        Some("in his time, as Juan has it")
    );
    a.close().unwrap();
    b.close().unwrap();
}

/// L26 (sync-design §9.3): three computers sync at once. One that loses the race to another's
/// push fetches what is new and pushes again, within its attempts, and all three converge.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_contention_three_devices_converge() {
    let door43 = door43("titus-busy").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-busy");
    synced(&sync(&a, &door43).await);
    let repo: tc_door43::RepoRef = format!("{}/titus-busy", seeded::USER).parse().unwrap();
    let mut others = Vec::new();
    for name in ["juan", "maria"] {
        let computer = computer(dir.path(), name);
        let folder = tc_sync::join(
            &computer.store,
            &door43.client,
            Some(&door43.session),
            &repo,
            "Titus",
        )
        .await
        .unwrap();
        others.push((computer, folder));
    }
    let b = open(&others[0].0, &others[0].1);
    let c = open(&others[1].0, &others[1].1);
    save(&a, 1, 1, "Paul, as Ana has it");
    save(&b, 1, 2, "in hope, as Juan has it");
    save(&c, 1, 3, "in his time, as Maria has it");
    synced(&sync(&b, &door43).await);
    // Ana's work is committed already, so the only commit her sync makes is its merge of
    // Juan's work, after the fetch. It waits there while Maria pushes, so Ana's push loses
    // the race.
    a.checkpoint(tc_store::CheckpointReason::Other("before the sync".into()))
        .await
        .unwrap();
    let hooks = ana.store.hooks();
    let point = tc_store::Point::BeforeHeadUpdate;
    hooks.hold(point);
    let maria = async {
        held_at(hooks, point, 1).await;
        let report = sync(&c, &door43).await;
        hooks.release(point);
        report
    };
    let (ana_report, maria_report) = tokio::join!(sync(&a, &door43), maria);
    synced(&ana_report);
    synced(&maria_report);
    assert!(ana_report.raced >= 1, "{ana_report:?}");
    for handle in [&b, &c] {
        synced(&sync(handle, &door43).await);
    }
    for handle in [&a, &b, &c] {
        assert_eq!(words(handle, 1, 1).as_deref(), Some("Paul, as Ana has it"));
        assert_eq!(
            words(handle, 1, 2).as_deref(),
            Some("in hope, as Juan has it")
        );
        assert_eq!(
            words(handle, 1, 3).as_deref(),
            Some("in his time, as Maria has it")
        );
    }
    a.close().unwrap();
    b.close().unwrap();
    c.close().unwrap();
}

/// Every file under the project's `ingredients/`, by path relative to the folder.
fn ingredient_paths(folder: &Path) -> Vec<String> {
    project_files(folder)
        .into_keys()
        .filter(|p| p.starts_with("ingredients/") && !p.starts_with("ingredients/tc/"))
        .collect()
}

/// L79 (§3.3, §7.7): two teammates add a recording whose paths differ only in letter case.
/// Both are kept: on Door43 and on each computer one of them is at a `.case-` path, so on
/// Windows or macOS neither file is written over the other, and the look at the folder
/// records neither as an edit of the other.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loss_door43_case_alias_never_recorded() {
    let door43 = door43("titus-case").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-case");
    synced(&sync(&a, &door43).await);
    a.close().unwrap();
    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    let a = open(&ana, &folder_a);
    let add = |handle: &ProjectHandle, folder: &Path, rel: &str, bytes: &[u8]| {
        std::fs::create_dir_all(folder.join("ingredients/audio")).unwrap();
        std::fs::write(folder.join(rel), bytes).unwrap();
        blocking(|| handle.focus().wait().unwrap());
        handle.wait_idle();
    };
    add(&a, &folder_a, "ingredients/audio/Intro.mp3", b"Ana's intro");
    add(
        &b,
        &folder_b,
        "ingredients/audio/intro.mp3",
        b"Juan's intro",
    );
    synced(&sync(&a, &door43).await);
    synced(&sync(&b, &door43).await);
    synced(&sync(&a, &door43).await);
    let on_tip: std::collections::BTreeMap<String, Vec<u8>> = {
        let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, "titus-case")).unwrap();
        let tree = bare
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_tree()
            .unwrap();
        let mut out = std::collections::BTreeMap::new();
        tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            let path = format!("{dir}{}", entry.name().unwrap_or_default());
            if path.starts_with("ingredients/audio/") {
                out.insert(path, bare.find_blob(entry.id()).unwrap().content().to_vec());
            }
            git2::TreeWalkResult::Ok
        })
        .unwrap();
        out
    };
    let folded: std::collections::BTreeSet<String> =
        on_tip.keys().map(|p| p.to_lowercase()).collect();
    assert_eq!(
        folded.len(),
        on_tip.len(),
        "two paths alias on Door43: {on_tip:?}"
    );
    assert!(on_tip.values().any(|b| b == b"Ana's intro"), "{on_tip:?}");
    assert!(on_tip.values().any(|b| b == b"Juan's intro"), "{on_tip:?}");
    for (handle, folder) in [(&a, &folder_a), (&b, &folder_b)] {
        let before: Vec<_> = handle.snapshot().fold().carried_files();
        let paths = ingredient_paths(folder);
        let folded: std::collections::BTreeSet<String> =
            paths.iter().map(|p| p.to_lowercase()).collect();
        assert_eq!(folded.len(), paths.len(), "two paths alias: {paths:?}");
        let bytes: Vec<Vec<u8>> = paths
            .iter()
            .filter(|p| p.to_lowercase().contains("intro"))
            .map(|p| std::fs::read(folder.join(p)).unwrap())
            .collect();
        assert!(bytes.contains(&b"Ana's intro".to_vec()), "{paths:?}");
        assert!(bytes.contains(&b"Juan's intro".to_vec()), "{paths:?}");
        blocking(|| handle.focus().wait().unwrap());
        handle.wait_idle();
        assert_eq!(
            handle.snapshot().fold().carried_files(),
            before,
            "the look at the folder recorded nothing"
        );
    }
    a.close().unwrap();
    b.close().unwrap();
}

/// §9.7 `restored`: Door43's branch moved back behind a teammate's push (tC4's
/// sync-ok-restored), so the next sync sends the teammate's change again and says so; a
/// device's profile record is bookkeeping and never counts as received work (Q7).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sync_sends_again_what_door43_lost_and_never_counts_a_profile() {
    let door43 = door43("titus-restored").await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder_a = titus(&ana);
    let a = open(&ana, &folder_a);
    connect(&a, &door43, "titus-restored");
    synced(&sync(&a, &door43).await);
    tc_sync::record_username(&a, &door43.session).await.unwrap();
    synced(&sync(&a, &door43).await);
    let before_juan = on_door43_tip(&door43, "titus-restored");
    a.close().unwrap();

    let juan = computer(dir.path(), "juan");
    let folder_b = copy_to(&folder_a, &juan);
    let b = open(&juan, &folder_b);
    save(&b, 1, 2, "in hope, as Juan has it");
    let report = sync(&b, &door43).await;
    synced(&report);
    assert_eq!(
        report.received_by.values().sum::<u32>(),
        0,
        "Ana's profile record is not received work: {report:?}"
    );
    let a = open(&ana, &folder_a);
    let report = sync(&a, &door43).await;
    synced(&report);
    assert_eq!(report.received_by.values().sum::<u32>(), 1, "{report:?}");
    assert_eq!(report.restored, 0, "{report:?}");
    // Door43 loses Juan's push.
    let bare = git2::Repository::open(door43.fake.git_dir(seeded::USER, "titus-restored")).unwrap();
    bare.reference("refs/heads/main", before_juan, true, "moved back")
        .unwrap();
    save(&a, 1, 3, "in his time, as Ana has it");
    let report = sync(&a, &door43).await;
    synced(&report);
    assert_eq!(
        report.restored, 1,
        "Juan's change was sent again: {report:?}"
    );
    assert_eq!(report.received_by.values().sum::<u32>(), 0, "{report:?}");
    let again = sync(&a, &door43).await;
    assert_eq!(again.restored, 0, "{again:?}");
    a.close().unwrap();
    b.close().unwrap();
}
