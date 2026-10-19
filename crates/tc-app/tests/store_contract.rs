//! The storage seam's contract (APP-DESIGN §4.2): the cases every backend passes, each named
//! after the rule it protects. `store_contract!` runs them on one backend; this file runs them
//! on the memory backend, and C0 adds `store_contract!(store, crate::store_fixture)` with the cases
//! only the store can run.
// Tests read whole folds to check results; the lint on whole-fold walks guards the product's
// paths (crates/tc-app/clippy.toml).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

#[path = "app/common.rs"]
mod common;
#[path = "support/counting_store.rs"]
mod counting_store;

use std::collections::BTreeMap;
use std::sync::Arc;

use tc_api::ids::ProjectRef;
use tc_app::backend::ProjectStore;
use tc_app::backend::memory::{MemoryBackend, MemoryProject};
use tc_app::backend::scenario::ScenarioState;
use tc_journal::fold::ProjectKind;
use tc_journal::{BookId, ProjectId, RecordKey};

/// What a backend gives the contract cases.
pub struct ContractFixture {
    pub store: Arc<dyn ProjectStore>,
    /// The sample Bible project (Titus and Jonah).
    pub bible: ProjectRef,
    /// Its USFM files' bytes, by book.
    pub sources: BTreeMap<BookId, Vec<u8>>,
    /// A record with two heads: two teammates changed Titus 1:1 from the same version.
    pub conflicted: (ProjectRef, RecordKey),
    /// A project with another id, so another token key.
    pub other: ProjectRef,
}

/// The memory backend over the `sample` scenario, plus the `shared` scenario's Bible project
/// with two teammates' heads on Titus 1:1, and an empty project of another id.
fn memory_fixture() -> ContractFixture {
    let root = common::repo_root();
    let sample = ScenarioState::from_repository(&root, "sample").unwrap();
    let (backend, _) = MemoryBackend::from_scenario(&sample).unwrap();
    let shared = ScenarioState::from_repository(&root, "shared").unwrap();
    let (shared_backend, _) = MemoryBackend::from_scenario(&shared).unwrap();
    let teammates_folder = common::folder("sample_burrito_teammates");
    backend
        .add(
            teammates_folder.clone(),
            shared_backend
                .project(&common::folder("sample_burrito"))
                .unwrap(),
        )
        .unwrap();
    let changes = backend
        .teammates_edit_verse(
            &teammates_folder,
            &common::tit(1, 1),
            &["Paul, a servant of God", "Paul, a slave of God"],
        )
        .unwrap();
    backend.teammate_edits(&teammates_folder, changes).unwrap();
    let other = common::folder("another_project");
    backend
        .add(
            other.clone(),
            MemoryProject::empty(ProjectId::from_bytes([0x0d; 16]), ProjectKind::Bible),
        )
        .unwrap();
    let source = |book: &str| {
        sample.files()[&format!("repos/_local_/_local_/sample_burrito/ingredients/{book}.usfm")]
            .clone()
    };
    ContractFixture {
        store: Arc::new(backend),
        bible: common::folder("sample_burrito"),
        sources: BTreeMap::from([
            (BookId::Titus, source("TIT")),
            (BookId::Jonah, source("JON")),
        ]),
        conflicted: (teammates_folder, common::tit(1, 1)),
        other,
    }
}

/// The contract cases, on the backend `$make` builds.
macro_rules! store_contract {
    ($name:ident, $make:path) => {
        mod $name {
            use std::sync::Arc;
            use std::time::Duration;

            use tc_api::app::SessionRole;
            use tc_api::{ApiError, NotSaved};
            use tc_app::backend::{ProjectHandle, ProjectStore};
            use tc_app::config::ProjectsFolder;
            use tc_app::counters::{AppCounters, AppCounts, CountingSnapshot};
            use tc_app::wire::token::{edit_token, review, review_token, shown};
            use tc_journal::fold::Counters;
            use tc_journal::{ShownToken, TokenKey};

            use crate::ContractFixture;
            use crate::common::{Parts, TestApp, open_progress};
            use crate::counting_store::{CountingStore, SharedStore};

            fn fixture() -> ContractFixture {
                $make()
            }

            fn open(
                store: &Arc<dyn ProjectStore>,
                folder: &tc_api::ids::ProjectRef,
            ) -> Arc<dyn ProjectHandle> {
                store.open(folder, open_progress()).unwrap()
            }

            fn counted(handle: &dyn ProjectHandle) -> CountingSnapshot {
                CountingSnapshot::new(handle.snapshot(), Arc::new(AppCounters::default()))
            }

            /// The projection matches the source bytes: the book's head and each chapter, as
            /// the editor reads a chapter.
            #[tokio::test]
            async fn chapter_view_equals_seed_usfm() {
                let f = fixture();
                let handle = open(&f.store, &f.bible);
                let read = counted(handle.as_ref());
                for (book, source) in &f.sources {
                    let text = std::str::from_utf8(source).unwrap();
                    // Every `\c` of these files starts a line.
                    let starts: Vec<usize> = text
                        .match_indices("\\c ")
                        .map(|(at, _)| at)
                        .filter(|at| *at == 0 || text.as_bytes()[at - 1] == b'\n')
                        .collect();
                    assert_eq!(starts.len(), text.matches("\\c ").count(), "{book:?}");
                    assert!(!starts.is_empty(), "{book:?}");
                    let mut whole = text[..starts[0]].to_owned();
                    for (i, start) in starts.iter().enumerate() {
                        let end = starts.get(i + 1).copied().unwrap_or(text.len());
                        let chunk = &text[*start..end];
                        let number: u16 = chunk[3..]
                            .split(|c: char| !c.is_ascii_digit())
                            .next()
                            .unwrap()
                            .parse()
                            .unwrap();
                        assert_eq!(
                            read.chapter_usfm(*book, number).as_deref(),
                            Some(chunk),
                            "{book:?} {number}"
                        );
                        whole.push_str(chunk);
                    }
                    assert_eq!(whole, text);
                    assert_eq!(read.book_usfm(*book).as_deref(), Some(text), "{book:?}");
                }
            }

            /// A token the backend minted reads back as the same token (STORAGE:745-754).
            #[tokio::test]
            async fn minted_token_parses() {
                let f = fixture();
                let handle = open(&f.store, &f.bible);
                let key = crate::common::tit(1, 1);
                let minted = handle.snapshot().mint(&key, None).unwrap();
                let json = serde_json::to_string(&shown(&minted).unwrap()).unwrap();
                let back: tc_api::ShownToken = serde_json::from_str(&json).unwrap();
                let parsed = edit_token(handle.as_ref(), &back).unwrap();
                assert_eq!(parsed.key(), &key);
                assert_eq!(parsed.heads(), minted.heads());
                assert!(!parsed.heads().is_empty());
            }

            /// A screen cannot forge a token: a changed byte, another installation's key or
            /// another project's key is refused (STORAGE:745-754).
            #[tokio::test]
            async fn forged_token_is_refused() {
                let f = fixture();
                let handle = open(&f.store, &f.bible);
                let key = crate::common::tit(1, 1);
                let good = shown(&handle.snapshot().mint(&key, None).unwrap()).unwrap();
                let bad = |token: &tc_api::ShownToken| {
                    assert_eq!(
                        edit_token(handle.as_ref(), token).unwrap_err(),
                        ApiError::NotSaved(NotSaved::BadToken)
                    );
                };

                // A character inside the MAC that carries six bits of it: the last one may
                // carry fewer, which a decoder could ignore.
                let mut body: Vec<u8> = good.body().as_bytes().to_vec();
                let inside = body.len() - 3;
                body[inside] = if body[inside] == b'A' { b'B' } else { b'A' };
                bad(&tc_api::ShownToken::from_body(std::str::from_utf8(&body).unwrap()).unwrap());

                let facts = handle.facts();
                let elsewhere = TokenKey::derive(&[7; 32], facts.id);
                let heads = handle.snapshot().fold().heads(&key);
                let minted = ShownToken::mint(&elsewhere, key.clone(), heads).unwrap();
                bad(&tc_api::ShownToken::from_body(&minted.to_text()).unwrap());

                let other = open(&f.store, &f.other);
                let theirs = shown(&other.snapshot().mint(&key, None).unwrap()).unwrap();
                bad(&theirs);
            }

            /// A review token lists every head of a record in conflict; an edit token only the
            /// displayed one (STORAGE:1610-1616).
            #[tokio::test]
            async fn review_token_lists_every_head() {
                let f = fixture();
                let (folder, key) = &f.conflicted;
                let handle = open(&f.store, folder);
                let snapshot = handle.snapshot();
                let heads = snapshot.fold().heads(key);
                assert_eq!(heads.len(), 2);
                assert_eq!(snapshot.mint_review(key).unwrap().heads(), heads.as_slice());
                let edit = snapshot.mint(key, None).unwrap();
                assert_eq!(edit.heads().len(), 1);
                assert!(heads.contains(&edit.heads()[0]));
            }

            /// AD3: a review token's text is never an edit token, by its prefix, and by the
            /// kind under its MAC when it is relabelled.
            #[tokio::test]
            async fn review_token_text_is_refused_as_an_edit_token() {
                let f = fixture();
                let (folder, key) = &f.conflicted;
                let handle = open(&f.store, folder);
                let rt = review(&handle.snapshot().mint_review(key).unwrap()).unwrap();
                let text = serde_json::to_string(&rt).unwrap();
                assert!(text.starts_with("\"rt1."));
                assert!(serde_json::from_str::<tc_api::ShownToken>(&text).is_err());
                let relabelled = format!("\"st1.{}\"", rt.body());
                let read: tc_api::ShownToken = serde_json::from_str(&relabelled).unwrap();
                assert_eq!(
                    edit_token(handle.as_ref(), &read).unwrap_err(),
                    ApiError::NotSaved(NotSaved::BadToken)
                );
            }

            /// The mirror of the case above: an edit token's text is never a review token.
            #[tokio::test]
            async fn edit_token_text_is_refused_as_a_review_token() {
                let f = fixture();
                let (folder, key) = &f.conflicted;
                let handle = open(&f.store, folder);
                let edit = shown(&handle.snapshot().mint(key, None).unwrap()).unwrap();
                let text = serde_json::to_string(&edit).unwrap();
                assert!(serde_json::from_str::<tc_api::ReviewToken>(&text).is_err());
                let relabelled = format!("\"rt1.{}\"", edit.body());
                let read: tc_api::ReviewToken = serde_json::from_str(&relabelled).unwrap();
                assert_eq!(
                    review_token(handle.as_ref(), &read).unwrap_err(),
                    ApiError::NotSaved(NotSaved::BadToken)
                );
            }

            /// STORAGE:574-577: a snapshot is an `Arc` clone; taking one does no fold work.
            #[tokio::test]
            async fn snapshot_clone_costs_nothing() {
                let f = fixture();
                let handle = open(&f.store, &f.bible);
                let counters = Arc::new(AppCounters::default());
                let first = handle.snapshot();
                let before = (first.fold().counters(), counters.read());
                let mut kept = Vec::new();
                for _ in 0..1_000 {
                    kept.push(first.clone());
                    kept.push(handle.snapshot());
                    let _ = CountingSnapshot::new(handle.snapshot(), counters.clone());
                }
                assert_eq!(
                    first.fold().counters().since(&before.0),
                    Counters::default()
                );
                assert_eq!(counters.read().since(&before.1), AppCounts::default());
                assert_eq!(handle.snapshot().seq(), first.seq());
            }

            async fn app_over(
                f: &ContractFixture,
            ) -> (tempfile::TempDir, TestApp, Arc<CountingStore<SharedStore>>) {
                let store = Arc::new(CountingStore::new(SharedStore(f.store.clone())));
                let dir = tempfile::tempdir().unwrap();
                let projects = dir.path().join("projects");
                std::fs::create_dir(&projects).unwrap();
                let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
                    config.projects = ProjectsFolder::Given(projects.clone());
                    deps.store = store.clone();
                })
                .await;
                (dir, t, store)
            }

            /// AD38: a second open of a folder while the first runs waits for it and shares
            /// its session, whichever comes first.
            async fn opens_of_one_folder_share_one_session(screen_first: bool) {
                let f = fixture();
                let (_dir, t, store) = app_over(&f).await;
                let sessions = t.app.sessions().clone();
                store.hold_open(&f.bible);
                let screen = {
                    let sessions = sessions.clone();
                    let folder = f.bible.clone();
                    async move {
                        sessions
                            .open_screen(&folder, open_progress())
                            .await
                            .unwrap()
                    }
                };
                let side = {
                    let sessions = sessions.clone();
                    let folder = f.bible.clone();
                    async move { sessions.open_side(&folder, open_progress()).await.unwrap() }
                };
                let (screen, side) = if screen_first {
                    let screen = tokio::spawn(screen);
                    crate::common::until("the first open reaches the store", || {
                        store.opens(&f.bible) == 1
                    })
                    .await;
                    let side = tokio::spawn(side);
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    store.release_open(&f.bible);
                    (screen.await.unwrap(), side.await.unwrap())
                } else {
                    let side = tokio::spawn(side);
                    crate::common::until("the first open reaches the store", || {
                        store.opens(&f.bible) == 1
                    })
                    .await;
                    let screen = tokio::spawn(screen);
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    store.release_open(&f.bible);
                    (screen.await.unwrap(), side.await.unwrap())
                };
                assert_eq!(&screen.at, side.scope(), "one session, one epoch");
                assert_eq!(store.opens(&f.bible), 1, "the store opened the folder once");
                let open = sessions.open_sessions();
                assert_eq!(open.len(), 1);
                assert_eq!(open[0].role, SessionRole::Screen);

                sessions.leave_screen(&screen.at).unwrap();
                sessions.settled(&f.bible).await;
                let open = sessions.open_sessions();
                assert_eq!(open.len(), 1, "the side user keeps it open");
                assert_eq!(open[0].role, SessionRole::Side);
                assert_eq!(open[0].at, screen.at);
                assert_eq!(store.closes(&f.bible), 0);

                drop(side);
                sessions.settled(&f.bible).await;
                assert_eq!(store.closes(&f.bible), 1);
                assert!(sessions.open_sessions().is_empty());
            }

            #[tokio::test]
            async fn opens_of_one_folder_share_one_session_screen_first() {
                opens_of_one_folder_share_one_session(true).await;
            }

            #[tokio::test]
            async fn opens_of_one_folder_share_one_session_side_first() {
                opens_of_one_folder_share_one_session(false).await;
            }
        }
    };
}

store_contract!(memory, crate::memory_fixture);
