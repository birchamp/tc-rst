//! The draft offer and the read-only modes (APP-DESIGN §6.4, §6.6, unit C1b), through
//! `App::call` on tc-store over its fault-injecting file system. A crash ends an app the way
//! `test/crash` does; the next launch is a new app over the files a killed process leaves
//! (`FaultFs::crash`), and its open offers what was not saved.

use std::sync::Arc;

use tc_api::buffer::{
    BufferEntry, BufferKey, BufferLine, FieldKey, FieldKind, FieldPlace, SentSave,
};
use tc_api::event::SessionMode;
use tc_api::ids::{ProjectRef, RequestId, ServerInstance, WallMs};
use tc_api::session::{
    CopyProject, DiscardDraft, DraftKind, DraftOffer, Drafts, Keep, KeepDraft, KeptDraft,
    OfferedDrafts, OtherComputerClosed,
};
use tc_api::test::{Crash, CrashPoint, SaveVerse};
use tc_api::write::{ReadOnlyReason, Stored};
use tc_api::{ApiError, Endpoint, NotSaved, ShownToken};
use tc_app::backend::{CheckpointReason, ProjectStore, StoreCommand};
use tc_app::config::ProjectsFolder;
use tc_app::session::ProjectSession;
use tc_app::wire::token::shown;
use tc_app::{Answer, StoreChoice};
use tc_journal::RecordKey;
use tc_journal::command::DraftField;
use tc_store::FakePlatform;
use tc_store::fault::FaultFs;
use tc_ui::app::buffer::{self as page, MemStore};

use super::common::{Parts, TestApp, post, rid, tit};
use crate::store_world::{APP, backend_over, folder, store_world, words_of};

/// A request id no other request of the test binary uses.
fn fresh() -> RequestId {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut bytes = [0xd7u8; 16];
    bytes[8..].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    RequestId::from_bytes(bytes)
}

/// One computer's app over a project folder's files; `relaunch` is the next launch after the
/// app was ended.
struct Rig {
    fs: FaultFs,
    hooks: Arc<tc_store::Hooks>,
    t: TestApp,
    dir: tempfile::TempDir,
    bible: ProjectRef,
}

impl Rig {
    async fn new() -> Rig {
        let world = store_world();
        let dir = tempfile::tempdir().unwrap();
        let t = start(&dir, world.store()).await;
        Rig {
            fs: world.fs.clone(),
            hooks: world.hooks.clone(),
            t,
            dir,
            bible: world.bible.clone(),
        }
    }

    /// The next launch on `platform`'s computer, over the files the ended app left.
    async fn relaunch_on(&mut self, platform: FakePlatform) {
        let fs = self.fs.crash(false);
        let hooks = Arc::new(tc_store::Hooks::default());
        let backend = Arc::new(backend_over(&fs, APP, platform, hooks.clone()));
        self.t = start(&self.dir, backend).await;
        self.fs = fs;
        self.hooks = hooks;
    }

    async fn relaunch(&mut self) {
        self.relaunch_on(FakePlatform::default()).await;
    }

    async fn open(&self, project: &ProjectRef) -> Arc<ProjectSession> {
        let open = self
            .t
            .app
            .sessions()
            .open_screen(project, crate::store_world::progress())
            .await
            .unwrap();
        let session = self.t.app.sessions().reader(&open.at).unwrap();
        session.handle().wait_idle();
        session
    }

    /// A token of `key` as the session shows it now.
    fn token(&self, session: &ProjectSession, key: &RecordKey) -> ShownToken {
        shown(&session.read().mint(key, None).unwrap()).unwrap()
    }

    async fn offer(&self, session: &ProjectSession, buffered: &[String]) -> OfferedDrafts {
        self.offer_with(session, buffered, false).await
    }

    /// The offer of a page in the shell, whose lines include the mirror's.
    async fn offer_with(
        &self,
        session: &ProjectSession,
        buffered: &[String],
        mirror: bool,
    ) -> OfferedDrafts {
        post(
            &self.t.app,
            &Drafts {
                at: session.scope().clone(),
                request: fresh(),
                buffered: buffered.to_vec(),
                mirror,
            },
        )
        .await
        .unwrap()
    }

    async fn keep(
        &self,
        session: &ProjectSession,
        id: u8,
        offer: &DraftOffer,
    ) -> Result<KeptDraft, ApiError> {
        post(
            &self.t.app,
            &KeepDraft {
                at: session.scope().clone(),
                request: rid(id),
                kind: offer.kind,
                shown: offer.shown.clone(),
                text: offer.text.clone(),
                discard: offer.settles.stored.clone(),
            },
        )
        .await
    }

    /// A draft of `key` as an editor sends it, through the store (C2 sends it with
    /// `editor/draft`), written to `drafts.json` before this returns.
    async fn stored_draft(&self, session: &ProjectSession, key: &RecordKey, text: &str) {
        let token = session.read().mint(key, None).unwrap();
        session
            .handle()
            .submit(StoreCommand::draft(
                DraftField::Words,
                vec![token],
                text.to_owned(),
            ))
            .await
            .unwrap();
        session.handle().wait_idle();
    }

    /// A save through the request path, whose answer the caller reads: `Answer::Ended` when a
    /// crash point ended the app.
    async fn save(&self, session: &ProjectSession, id: u8, key: &RecordKey, words: &str) -> Answer {
        let req = SaveVerse {
            at: session.scope().clone(),
            request: rid(id),
            shown: self.token(session, key),
            words: words.to_owned(),
        };
        self.t
            .app
            .call(SaveVerse::PATH, &serde_json::to_vec(&req).unwrap())
            .await
    }

    async fn arm(&self, at: CrashPoint) {
        post(
            &self.t.app,
            &Crash {
                request: RequestId::from_bytes([0xc1; 16]),
                at,
                path: Some(SaveVerse::PATH.to_owned()),
            },
        )
        .await
        .unwrap();
    }
}

async fn start(dir: &tempfile::TempDir, store: Arc<dyn ProjectStore>) -> TestApp {
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(store);
    })
    .await
}

fn key_of(session: &ProjectSession, place: &str) -> BufferKey {
    BufferKey {
        project: session.scope().project.clone(),
        id: session.id().clone(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse(place).unwrap(),
        },
    }
}

/// An entry as the page's buffer keeps it.
fn entry(
    instance: ServerInstance,
    base: ShownToken,
    text: &str,
    sent: Option<&str>,
    at: i64,
) -> BufferEntry {
    BufferEntry {
        api: 1,
        instance,
        base: vec![base],
        text: text.to_owned(),
        sent: sent.map(|text| SentSave {
            request: RequestId::from_bytes([0x5e; 16]),
            text: text.to_owned(),
            scope: None,
        }),
        at: WallMs(at),
        draft: None,
    }
}

/// The mirror's line of a change, as the shell keeps it and the UI sends it.
fn line(key: &BufferKey, entry: &BufferEntry) -> String {
    serde_json::to_string(&BufferLine::Changed {
        key: key.clone(),
        entry: entry.clone(),
    })
    .unwrap()
}

fn words(session: &ProjectSession, key: &RecordKey) -> Option<String> {
    words_of(&session.handle().snapshot(), key)
}

fn heads(session: &ProjectSession, key: &RecordKey) -> usize {
    session.handle().snapshot().fold().heads(key).len()
}

fn offer_at<'a>(offers: &'a OfferedDrafts, place: &str) -> &'a DraftOffer {
    offers
        .offers
        .iter()
        .find(|offer| offer.place.as_str() == place)
        .unwrap_or_else(|| panic!("no offer for {place}: {offers:?}"))
}

/// A-3, §6.6: after the app was ended, the next open offers the text of all three places:
/// the page's buffer, the shell's mirror and `drafts.json`. An entry whose text the project
/// already holds is not offered but named as saved; another folder's entry is not this
/// project's; Keep stores each, and the store's draft goes with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relaunch_offers_buffered_and_mirrored_entries() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let run = rig.t.app.instance();
    let (v4, v5, v6, v7) = (tit(1, 4), tit(1, 5), tit(1, 6), tit(1, 7));
    let stored_v7 = words(&session, &v7).unwrap();
    let page_entry = entry(
        run,
        rig.token(&session, &v4),
        "typed in the page",
        None,
        1_000,
    );
    let mirrored = entry(
        run,
        rig.token(&session, &v5),
        "kept by the mirror",
        None,
        2_000,
    );
    let already = entry(run, rig.token(&session, &v7), &stored_v7, None, 3_000);
    let elsewhere = BufferKey {
        project: folder("another_project"),
        ..key_of(&session, "TIT 1:4")
    };
    rig.stored_draft(&session, &v6, "written to drafts.json")
        .await;
    let buffered = vec![
        line(&key_of(&session, "TIT 1:4"), &page_entry),
        line(&key_of(&session, "TIT 1:5"), &mirrored),
        line(&key_of(&session, "TIT 1:7"), &already),
        line(
            &elsewhere,
            &entry(run, rig.token(&session, &v4), "not here", None, 4_000),
        ),
        "{\"type\":\"changed\",\"value\":{\"key\"".to_owned(),
    ];
    drop(session);

    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    let offers = rig.offer(&session, &buffered).await;
    assert_eq!(offers.offers.len(), 3, "{offers:#?}");
    for (place, text) in [
        ("TIT 1:4", "typed in the page"),
        ("TIT 1:5", "kept by the mirror"),
        ("TIT 1:6", "written to drafts.json"),
    ] {
        let offer = offer_at(&offers, place);
        assert_eq!(offer.text, text);
        assert_eq!(offer.kind, DraftKind::Words);
        assert_eq!(offer.keep, Keep::Save);
    }
    // Each text is in one offered entry of drafts.json now, and the page drops its slots.
    for place in ["TIT 1:4", "TIT 1:5", "TIT 1:6"] {
        let offer = offer_at(&offers, place);
        assert_eq!(offer.settles.stored.len(), 1, "{place}");
        assert!(offer.settles.buffered.is_empty(), "{place}");
    }
    let settled: Vec<&BufferKey> = offers.settle.iter().map(|slot| &slot.key).collect();
    assert_eq!(
        settled.len(),
        3,
        "the saved slot, and the two taken in: {settled:?}"
    );
    assert!(settled.contains(&&key_of(&session, "TIT 1:7")));

    for (n, offer) in offers.offers.iter().enumerate() {
        let kept = rig.keep(&session, 40 + n as u8, offer).await.unwrap();
        assert_eq!(kept.stored, tc_api::write::Stored::Written);
        assert!(kept.new_conflicts.is_empty(), "{kept:?}");
    }
    assert_eq!(words(&session, &v4).as_deref(), Some("typed in the page"));
    assert_eq!(words(&session, &v5).as_deref(), Some("kept by the mirror"));
    assert_eq!(
        words(&session, &v6).as_deref(),
        Some("written to drafts.json")
    );
    // Kept: nothing is offered again, whatever the page still sends.
    assert!(rig.offer(&session, &buffered).await.offers.is_empty());
}

/// §6.6 with `test/crash`: the app is ended while the person's save of typed text is on its
/// way, before the store; a teammate's change reaches the verse before the next open. The
/// text is offered, and Keep makes a conflict that keeps both versions, never a revert.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_while_typing_then_keep_makes_a_conflict_when_the_verse_changed() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let key = tit(1, 8);
    let typed = entry(
        rig.t.app.instance(),
        rig.token(&session, &key),
        "lover of what is good",
        Some("lover of what is good"),
        5_000,
    );
    rig.arm(CrashPoint::BeforeStore).await;
    assert_eq!(
        rig.save(&session, 1, &key, "lover of what is good").await,
        Answer::Ended
    );
    let buffered = vec![line(&key_of(&session, "TIT 1:8"), &typed)];
    drop(session);

    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    let theirs = rig.save(&session, 2, &key, "a friend of the good").await;
    assert!(matches!(theirs, Answer::Reply { .. }));
    let offers = rig.offer(&session, &buffered).await;
    let offer = offer_at(&offers, "TIT 1:8");
    assert_eq!(offer.text, "lover of what is good");
    assert_eq!(offer.keep, Keep::Save);
    let kept = rig.keep(&session, 3, offer).await.unwrap();
    assert_eq!(kept.new_conflicts.len(), 1, "{kept:?}");
    assert_eq!(heads(&session, &key), 2, "both versions stand");
}

/// A-4b's rule for the offer (§6.6): the app is ended after the save was stored and before
/// its reply; the person had typed on. The newer text continues from the stored one, so Keep
/// stores it over the stored value's token, with no conflict against the person's own save.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_save_that_landed_before_the_end_is_continued_without_a_conflict() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let key = tit(1, 9);
    let typed = entry(
        rig.t.app.instance(),
        rig.token(&session, &key),
        "holding firm the trustworthy word, and more",
        Some("holding firm the trustworthy word"),
        6_000,
    );
    rig.arm(CrashPoint::BeforeReply).await;
    assert_eq!(
        rig.save(&session, 1, &key, "holding firm the trustworthy word")
            .await,
        Answer::Ended
    );
    let buffered = vec![line(&key_of(&session, "TIT 1:9"), &typed)];
    drop(session);

    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    assert_eq!(
        words(&session, &key).as_deref(),
        Some("holding firm the trustworthy word")
    );
    let offers = rig.offer(&session, &buffered).await;
    let kept = rig
        .keep(&session, 2, offer_at(&offers, "TIT 1:9"))
        .await
        .unwrap();
    assert!(kept.new_conflicts.is_empty(), "{kept:?}");
    assert_eq!(heads(&session, &key), 1);
    assert_eq!(
        words(&session, &key).as_deref(),
        Some("holding firm the trustworthy word, and more")
    );
}

/// R20, §6.6: after the installation secret is lost (an app-data reset, a reinstall), the
/// drafts and the buffered entries of the last session are offered as text, never dropped,
/// and "Keep it as a separate version" writes the text beside the current value as a conflict.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn draft_with_unverifiable_token_is_offered_as_text() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let (stored_key, typed_key) = (tit(1, 10), tit(1, 11));
    // Saved here first, so the current values are the person's own versions, which a seed's
    // first text is not (a seed stub gives way to any other head, STORAGE §7).
    for (n, key) in [&stored_key, &typed_key].into_iter().enumerate() {
        assert!(matches!(
            rig.save(&session, 10 + n as u8, key, "saved before the reset")
                .await,
            Answer::Reply { .. }
        ));
    }
    rig.stored_draft(&session, &stored_key, "rebellious, typed")
        .await;
    let typed = entry(
        rig.t.app.instance(),
        rig.token(&session, &typed_key),
        "teaching what they should not",
        None,
        7_000,
    );
    let buffered = vec![line(&key_of(&session, "TIT 1:11"), &typed)];
    drop(session);

    rig.fs
        .delete(&std::path::Path::new(APP).join("installation.json"))
        .unwrap();
    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    let offers = rig.offer(&session, &buffered).await;
    assert_eq!(offers.offers.len(), 2, "{offers:#?}");
    for (n, (place, key)) in [("TIT 1:10", &stored_key), ("TIT 1:11", &typed_key)]
        .into_iter()
        .enumerate()
    {
        let offer = offer_at(&offers, place);
        assert_eq!(offer.keep, Keep::AsVersion, "{place}");
        let before = words(&session, key);
        let kept = rig.keep(&session, 50 + n as u8, offer).await.unwrap();
        assert_eq!(kept.new_conflicts.len(), 1, "{place}: {kept:?}");
        assert_eq!(heads(&session, key), 2, "{place}: both versions stand");
        assert_ne!(words(&session, key), None, "{place}: {before:?}");
    }
}

/// Discard drops a stored draft for good: the next open does not offer it again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discard_drops_the_draft_for_good() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let key = tit(1, 12);
    let before = words(&session, &key);
    rig.stored_draft(&session, &key, "not wanted after all")
        .await;
    drop(session);

    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    let offers = rig.offer(&session, &[]).await;
    let offer = offer_at(&offers, "TIT 1:12");
    post(
        &rig.t.app,
        &DiscardDraft {
            at: session.scope().clone(),
            request: rid(60),
            stored: offer.settles.stored.clone(),
        },
    )
    .await
    .unwrap();
    assert!(rig.offer(&session, &[]).await.offers.is_empty());
    session.handle().wait_idle();
    drop(session);

    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    assert!(rig.offer(&session, &[]).await.offers.is_empty());
    assert_eq!(words(&session, &key), before);
}

/// §6.4, A-10, R7: another installation saves into the folder while it is open here, so the
/// project turns read-only (`SecondWriter`). Nothing can be kept there, so nothing is offered;
/// the copy takes `drafts.json`, the page moves its entries to the copy, and the copy's first
/// open offers both, and keeps them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_writer_makes_the_project_read_only_and_the_copy_carries_its_drafts() {
    let rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let (stored_key, typed_key) = (tit(1, 13), tit(1, 14));
    rig.stored_draft(&session, &stored_key, "this testimony is true")
        .await;
    let mut storage = MemStore::new();
    let typed = entry(
        rig.t.app.instance(),
        rig.token(&session, &typed_key),
        "not paying attention to myths",
        None,
        8_000,
    );
    page::write_entry(&mut storage, &key_of(&session, "TIT 1:14"), &typed);

    // The other computer is told this one is closed, and saves.
    let theirs = backend_over(
        &rig.fs.other_machine(),
        "/app-desktop",
        FakePlatform {
            machine: "machine-2".to_owned(),
            host: "host-2".to_owned(),
            ..FakePlatform::default()
        },
        Arc::default(),
    );
    theirs.trust_lock(&rig.bible);
    let handle = theirs
        .open(&rig.bible, crate::store_world::progress())
        .unwrap();
    crate::store_world::save(handle.as_ref(), &tit(1, 15), "from the other computer")
        .await
        .unwrap();
    handle.wait_idle();
    session
        .handle()
        .checkpoint(CheckpointReason::Other("test".to_owned()))
        .await;
    assert_eq!(
        session.handle().facts().mode,
        SessionMode::ReadOnly(ReadOnlyReason::SecondWriter)
    );
    let refused = post(
        &rig.t.app,
        &SaveVerse {
            at: session.scope().clone(),
            request: rid(70),
            shown: rig.token(&session, &typed_key),
            words: "not paying attention to myths".to_owned(),
        },
    )
    .await;
    assert_eq!(
        refused.unwrap_err(),
        ApiError::NotSaved(NotSaved::ReadOnly(ReadOnlyReason::SecondWriter))
    );
    let buffered: Vec<String> = page::entries(&storage)
        .iter()
        .map(|(key, entry)| line(key, entry))
        .collect();
    let offers = rig.offer(&session, &buffered).await;
    assert!(
        offers.offers.is_empty() && offers.settle.is_empty(),
        "{offers:?}"
    );

    let copy = post(
        &rig.t.app,
        &CopyProject {
            at: session.scope().clone(),
            request: rid(71),
        },
    )
    .await
    .unwrap();
    assert_eq!(copy.project.as_str(), "sample_burrito - Copy");
    assert_eq!(&copy.id, session.id());
    let copied = tc_ui::app::offer::copy(
        &mut storage,
        &[],
        (&session.scope().project, session.id()),
        (&copy.project, &copy.id),
    );
    assert!(copied.stayed.is_empty());
    assert_eq!(
        copied.lines.len(),
        2,
        "the copy's line and the folder's removal"
    );
    rig.t.app.sessions().leave_screen(session.scope()).unwrap();
    drop(session);

    let copied = rig.open(&copy.project).await;
    assert_eq!(copied.handle().facts().mode, SessionMode::Writable);
    let buffered: Vec<String> = page::entries(&storage)
        .iter()
        .map(|(key, entry)| line(key, entry))
        .collect();
    let offers = rig.offer(&copied, &buffered).await;
    assert_eq!(offers.offers.len(), 2, "{offers:#?}");
    for (n, (place, key, text)) in [
        ("TIT 1:13", &stored_key, "this testimony is true"),
        ("TIT 1:14", &typed_key, "not paying attention to myths"),
    ]
    .into_iter()
    .enumerate()
    {
        let kept = rig
            .keep(&copied, 72 + n as u8, offer_at(&offers, place))
            .await
            .unwrap();
        assert_eq!(kept.stored, Stored::Written);
        assert_eq!(words(&copied, key).as_deref(), Some(text));
    }
}

/// STORAGE §5.0: a project whose lock names another computer opens read-only and offers
/// nothing; once the person says that computer is closed, the next open is writable and
/// offers the drafts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lock_of_another_computer_is_read_only_until_the_person_says_it_is_closed() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let key = tit(1, 16);
    rig.stored_draft(&session, &key, "they profess to know God")
        .await;
    drop(session);

    // The folder, still locked by this computer's ended app, is opened on another.
    rig.relaunch_on(FakePlatform {
        machine: "machine-2".to_owned(),
        host: "host-2".to_owned(),
        ..FakePlatform::default()
    })
    .await;
    let session = rig.open(&rig.bible.clone()).await;
    assert_eq!(
        session.handle().facts().mode,
        SessionMode::ReadOnly(ReadOnlyReason::OpenOnAnotherComputer)
    );
    assert!(rig.offer(&session, &[]).await.offers.is_empty());
    post(
        &rig.t.app,
        &OtherComputerClosed {
            at: session.scope().clone(),
            request: rid(80),
        },
    )
    .await
    .unwrap();
    rig.t.app.sessions().leave_screen(session.scope()).unwrap();
    drop(session);

    let session = rig.open(&rig.bible.clone()).await;
    assert_eq!(session.handle().facts().mode, SessionMode::Writable);
    let offers = rig.offer(&session, &[]).await;
    assert_eq!(
        offer_at(&offers, "TIT 1:16").text,
        "they profess to know God"
    );
}

/// Finding 6: after "That computer is closed", the read-only session the person left is
/// never joined again, even while another user (a sync from Home) keeps it open: the open
/// waits for its close and takes the lock afresh, writable, and the one-time trust is used up.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reopen_after_that_computer_is_closed_never_joins_the_read_only_session() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    drop(session);
    rig.relaunch_on(FakePlatform {
        machine: "machine-2".to_owned(),
        host: "host-2".to_owned(),
        ..FakePlatform::default()
    })
    .await;
    let session = rig.open(&rig.bible.clone()).await;
    assert_eq!(
        session.handle().facts().mode,
        SessionMode::ReadOnly(ReadOnlyReason::OpenOnAnotherComputer)
    );
    let side = rig
        .t
        .app
        .sessions()
        .open_side(&rig.bible, crate::store_world::progress())
        .await
        .unwrap();
    post(
        &rig.t.app,
        &OtherComputerClosed {
            at: session.scope().clone(),
            request: rid(81),
        },
    )
    .await
    .unwrap();
    rig.t.app.sessions().leave_screen(session.scope()).unwrap();
    drop(session);
    let sessions = rig.t.app.sessions().clone();
    let bible = rig.bible.clone();
    let reopen = tokio::spawn(async move {
        sessions
            .open_screen(&bible, crate::store_world::progress())
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!reopen.is_finished(), "the read-only session is not joined");
    drop(side);
    let reopened = reopen.await.unwrap().unwrap();
    assert_eq!(reopened.mode, SessionMode::Writable);
    // The trust was used: the next computer's lock is met afresh.
    rig.t.app.sessions().leave_screen(&reopened.at).unwrap();
    rig.relaunch_on(FakePlatform {
        machine: "machine-3".to_owned(),
        host: "host-3".to_owned(),
        ..FakePlatform::default()
    })
    .await;
    let session = rig.open(&rig.bible.clone()).await;
    assert_eq!(
        session.handle().facts().mode,
        SessionMode::ReadOnly(ReadOnlyReason::OpenOnAnotherComputer)
    );
}

fn now_ms() -> i64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    i64::try_from(since.as_millis()).unwrap()
}

/// `drafts.json` is written at most once a second, so after a crash it holds an older copy of
/// the typing the buffer kept: one page edits a field per run, so a stored draft of a run whose
/// own buffer entry of the field is newer goes once that entry is taken in, whatever version
/// it was typed over. A draft another run typed is other typing, and is offered beside it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stored_draft_gives_way_only_to_newer_typing_of_its_own_run() {
    let mut rig = Rig::new().await;
    let (same, other) = (tit(2, 1), tit(2, 2));
    // TIT 2:2: an earlier run's page typed a draft and the app ended.
    let session = rig.open(&rig.bible.clone()).await;
    rig.stored_draft(&session, &other, "typed by another run")
        .await;
    drop(session);
    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    let run = rig.t.app.instance();
    // TIT 2:1: the draft and the buffer edit one version; the buffer is newer.
    rig.stored_draft(&session, &same, "But as for you, speak")
        .await;
    let newer = entry(
        run,
        rig.token(&session, &same),
        "But as for you, speak what fits",
        None,
        now_ms() + 60_000,
    );
    // TIT 2:2: this run saved and typed on, without answering the earlier run's draft.
    assert!(matches!(
        rig.save(&session, 1, &other, "Older men are to be sober")
            .await,
        Answer::Reply { .. }
    ));
    let over_saved = entry(
        run,
        rig.token(&session, &other),
        "Older men are to be sober, dignified",
        None,
        now_ms() + 60_000,
    );
    let buffered = vec![
        line(&key_of(&session, "TIT 2:1"), &newer),
        line(&key_of(&session, "TIT 2:2"), &over_saved),
    ];
    drop(session);

    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    let offers = rig.offer(&session, &buffered).await;
    let at_same: Vec<&DraftOffer> = offers
        .offers
        .iter()
        .filter(|o| o.place.as_str() == "TIT 2:1")
        .collect();
    assert_eq!(at_same.len(), 1, "{offers:#?}");
    assert_eq!(at_same[0].text, "But as for you, speak what fits");
    assert_eq!(at_same[0].settles.stored.len(), 1, "taken in");
    assert!(
        offers
            .settle
            .iter()
            .any(|slot| slot.key == key_of(&session, "TIT 2:1"))
    );
    let mut at_other: Vec<&str> = offers
        .offers
        .iter()
        .filter(|o| o.place.as_str() == "TIT 2:2")
        .map(|o| o.text.as_str())
        .collect();
    at_other.sort_unstable();
    assert_eq!(
        at_other,
        [
            "Older men are to be sober, dignified",
            "typed by another run"
        ]
    );

    post(
        &rig.t.app,
        &DiscardDraft {
            at: session.scope().clone(),
            request: rid(90),
            stored: at_same[0].settles.stored.clone(),
        },
    )
    .await
    .unwrap();
    let again = rig.offer(&session, &[]).await;
    assert!(
        again.offers.iter().all(|o| o.place.as_str() != "TIT 2:1"),
        "{again:#?}"
    );
    drop(session);
    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    let after = rig.offer(&session, &[]).await;
    let places: Vec<&str> = after.offers.iter().map(|o| o.place.as_str()).collect();
    assert_eq!(
        places,
        ["TIT 2:2", "TIT 2:2"],
        "the older copy went too: {after:#?}"
    );
}

/// Finding 5: the session's own save replaced a draft, and the process ended before the
/// writer rewrote `drafts.json`. Behind the shell's mirror, which holds no entry of that run
/// for the verse (it ended clean), the older copy is not offered over the person's save, and
/// does not come back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_older_draft_is_not_offered_over_the_sessions_own_save() {
    let mut rig = Rig::new().await;
    let session = rig.open(&rig.bible.clone()).await;
    let key = tit(3, 1);
    rig.stored_draft(&session, &key, "Remind them to be").await;
    rig.hooks.hold(tc_store::Point::BeforeDraftWrite);
    assert!(matches!(
        rig.save(&session, 1, &key, "Remind them to be subject")
            .await,
        Answer::Reply { .. }
    ));
    drop(session);
    let hooks = rig.hooks.clone();
    rig.relaunch().await;
    hooks.release(tc_store::Point::BeforeDraftWrite);
    let session = rig.open(&rig.bible.clone()).await;
    let offers = rig.offer_with(&session, &[], true).await;
    assert!(offers.offers.is_empty(), "{offers:#?}");
    assert_eq!(
        words(&session, &key).as_deref(),
        Some("Remind them to be subject")
    );
    session.handle().wait_idle();
    drop(session);
    rig.relaunch().await;
    let session = rig.open(&rig.bible.clone()).await;
    assert!(rig.offer_with(&session, &[], true).await.offers.is_empty());
}
