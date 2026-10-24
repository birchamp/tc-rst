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
    let receive = format!("/{}/titus-race.git/git-receive-pack", seeded::USER);
    door43.fake.inject(tc_door43_fake::FaultRule::once(
        "POST",
        &receive,
        tc_door43_fake::Fault::Hold,
    ));
    let ana_done = std::sync::atomic::AtomicBool::new(false);
    let teammate = async {
        // Ana's push is held: Juan's goes first.
        loop {
            if door43
                .fake
                .requests()
                .iter()
                .any(|r| r.method == "POST" && r.path == receive)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let report = sync(&b, &door43).await;
        // The fake records a request before it holds it: a release that comes between is
        // lost, so it is sent until Ana's sync ends.
        while !ana_done.load(std::sync::atomic::Ordering::Acquire) {
            door43.fake.release_held();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        report
    };
    let ana_sync = async {
        let report = sync(&a, &door43).await;
        ana_done.store(true, std::sync::atomic::Ordering::Release);
        report
    };
    let (ana_report, juan_report) = tokio::join!(ana_sync, teammate);
    synced(&juan_report);
    synced(&ana_report);
    assert!(ana_report.raced >= 1, "{ana_report:?}");
    assert_eq!(words(&a, 1, 2).as_deref(), Some("in hope, as Juan has it"));
    synced(&sync(&b, &door43).await);
    assert_eq!(words(&b, 1, 1).as_deref(), Some("Paul, as Ana has it"));
    a.close().unwrap();
    b.close().unwrap();
}

/// gap-tc4-straggler-sync-safety state 3b: Door43's branch was deleted by its owner. The sync
/// pushes nothing, so no branch made from this computer replaces the team's, and the person
/// is told to ask the owner to restore it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sync_against_a_repository_with_no_main_branch_refuses_and_names_the_state() {
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
    assert_eq!(
        report.ended,
        Ended::Failed(tc_api::refusal::SyncFailure::Blocked(
            tc_api::refusal::SyncBlock::BranchMissing
        )),
        "{report:?}"
    );
    assert_eq!(
        branch_on_door43(&door43, "titus-gone"),
        None,
        "nothing pushed"
    );
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
    assert_eq!(report.other_tools.len(), 1, "{report:?}");
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
