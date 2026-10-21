//! The count row of B7 (§8.2): opening a story reads that story and nothing of the other
//! forty-nine, whatever they hold. The memory backend holds the project; the second project has
//! ten times the text in every other story, and comments and decisions on their frames.

use tc_api::editor::Book;
use tc_api::ids::StoryNo;
use tc_api::story;
use tc_app::backend::memory::MemoryProject;
use tc_app::backend::seed::{Tc4Folder, fixture_project_id, tc4_state_seed};
use tc_journal::fold::FoldInput;
use tc_journal::record::{CommentTarget, CommentValue};
use tc_journal::{
    BatchId, BatchRef, Cause, Change, ChangeBody, CommentId, DeviceId, DeviceSeq, FORMAT_MINOR,
    FrameNo, Record, WallTime,
};
use tc_store::fault::FaultFs;

use super::harness::{Backing, cost, growth, opened, scale};
use crate::common::repo_root;

fn journal_story(n: u8) -> tc_journal::StoryNo {
    tc_journal::StoryNo::new(n).unwrap()
}

/// The OBS sample's project as the memory backend holds it.
fn sample_stories() -> MemoryProject {
    let folder = Tc4Folder::read(&repo_root().join("fixtures/tc4/sample-burrito-obs")).unwrap();
    let seed = tc4_state_seed(&folder, fixture_project_id("sample_burrito_obs")).unwrap();
    MemoryProject::from_seed(seed, None)
}

/// The same project with every frame of stories 2 to 50 drafted at ten times the text, and a
/// comment on each: what a translator's other forty-nine stories can hold.
fn heavy_stories() -> MemoryProject {
    let mut project = sample_stories();
    let device = DeviceId::local([7; 15]);
    let mut seq = 0u64;
    let mut changes = Vec::new();
    for story in 2..=50u8 {
        for frame in 1..=16u16 {
            seq += 1;
            let record = if frame % 2 == 0 {
                Record::Frame {
                    story: journal_story(story),
                    frame: FrameNo(frame),
                    value: "Un texto largo de un fotograma. "
                        .repeat(40)
                        .trim()
                        .to_owned(),
                }
            } else {
                let mut id = [0u8; 16];
                id[..8].copy_from_slice(&seq.to_be_bytes());
                Record::Comment {
                    id: CommentId::from_bytes(id),
                    value: CommentValue {
                        target: CommentTarget::Frame {
                            story: journal_story(story),
                            frame: FrameNo(frame),
                        },
                        text: "Una nota larga sobre este fotograma. ".repeat(20),
                    },
                }
            };
            let mut batch = [0u8; 16];
            batch[..8].copy_from_slice(&seq.to_be_bytes());
            changes.push(
                Change::new(ChangeBody {
                    v: FORMAT_MINOR,
                    record,
                    parents: Vec::new(),
                    device,
                    seq: DeviceSeq(seq),
                    at: WallTime(1_790_000_000_000 + i64::try_from(seq).unwrap()),
                    batch: BatchRef::new(BatchId::from_bytes(batch), 1).unwrap(),
                    cause: Cause::Edit,
                })
                .unwrap(),
            );
        }
    }
    project.fold.add(changes.into_iter().map(FoldInput::from));
    project
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn story_open_touches_one_story() {
    let disk = FaultFs::new();
    let mut costs = Vec::new();
    let (light, heavy) = (sample_stories(), heavy_stories());
    let size = |p: &MemoryProject, n: u8| {
        p.fold
            .render(&tc_journal::fold::DerivedFile::Story(journal_story(n)))
            .unwrap()
            .len()
    };
    assert_eq!(
        size(&light, 1),
        size(&heavy, 1),
        "story 1 is the same in both"
    );
    assert!(
        size(&heavy, 7) > 4 * size(&light, 7),
        "the others are far larger"
    );
    for project in [light, heavy] {
        let s = scale(Backing::Memory, &project, &disk).await;
        opened(&s).await;
        let at = s.scope();
        costs.push(
            cost(&s, async |s| {
                let view = story_open(s, &at, 1).await;
                assert_eq!(view.frames.len(), 16);
                assert_eq!(view.comments, [], "story 1 holds no comment in either");
            })
            .await,
        );
    }
    let (small, large) = (costs[0], costs[1]);
    let grew = growth(&small, &large);
    assert!(
        grew.is_empty(),
        "opening a story grows with the others:\n{}",
        grew.join("\n")
    );
    // The story is its one derived file; no book is projected, no conflict list walked, and no
    // comment key is probed for a story without one.
    assert_eq!(small.app.files_rendered, 1, "{small:?}");
    assert_eq!(small.app.book_usfm, 0, "{small:?}");
    assert_eq!(small.app.books_rendered, 0, "{small:?}");
    assert_eq!(small.app.conflict_walks, 0, "{small:?}");
    assert_eq!(small.app.keys_probed, 0, "{small:?}");
    // The story's resources are read once for the open story project, not once per story.
    assert_eq!(small.app.book_resource_loads, 1, "{small:?}");
}

async fn story_open(s: &super::harness::Scale, at: &tc_api::ids::Scope, n: u8) -> story::StoryView {
    match s
        .app
        .request(&story::Open {
            at: at.clone(),
            story: StoryNo::new(n).unwrap(),
        })
        .await
        .unwrap()
    {
        tc_api::Reply::Ok(view) => view,
        tc_api::Reply::Err(error) => panic!("story/open: {error:?}"),
    }
}

/// A story's helps and its picture are read from the resources, which load once: a second
/// story, and the story's helps, read nothing again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_projects_resources_load_once_for_every_story() {
    let disk = FaultFs::new();
    let s = scale(Backing::Memory, &sample_stories(), &disk).await;
    opened(&s).await;
    let at = s.scope();
    let first = s.app.counters().book_resource_loads;
    story_open(&s, &at, 1).await;
    assert_eq!(s.app.counters().book_resource_loads - first, 1, "one load");
    let again = s.app.counters().book_resource_loads;
    story_open(&s, &at, 2).await;
    story_open(&s, &at, 1).await;
    assert_eq!(s.app.counters().book_resource_loads, again, "then none");
}

/// `Book` on a story project is refused, never answered with an empty book.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_story_project_has_no_book() {
    let disk = FaultFs::new();
    let s = scale(Backing::Memory, &sample_stories(), &disk).await;
    opened(&s).await;
    let reply = s
        .app
        .request(&Book {
            at: s.scope(),
            book: tc_core::BookId::Titus,
        })
        .await
        .unwrap();
    assert!(matches!(reply, tc_api::Reply::Err(_)), "{reply:?}");
}

/// The existing light/heavy OBS pair on the real store as valid story Burritos.
fn story_disk(project: &MemoryProject) -> FaultFs {
    let fs = FaultFs::new();
    let backend = crate::store_world::backend_over(
        &fs,
        // A separate importing installation gives the measured writer the same initial
        // sequence width in both projects; record-count digits must not mask a scan.
        "/obs-import-app",
        tc_store::FakePlatform::default(),
        std::sync::Arc::default(),
    );
    let records = project
        .fold
        .record_keys()
        .into_iter()
        .filter_map(|key| project.fold.projected(&key))
        .collect();
    let path = backend
        .store()
        .create(&tc_store::NewProject {
            name: super::corpus::FOLDER.to_owned(),
            kind: tc_journal::fold::ProjectKind::Stories,
            records,
            books: Vec::new(),
        })
        .unwrap();
    crate::store_world::assert_valid_burrito(&fs, &path);
    fs
}

async fn a_story_save_costs_its_record(reference: bool) {
    let mut costs = Vec::new();
    let (light, heavy) = (sample_stories(), heavy_stories());
    for project in [&light, &heavy] {
        let disk = story_disk(project);
        let s = scale(Backing::Store, project, &disk).await;
        opened(&s).await;
        let key = if reference {
            tc_journal::RecordKey::StoryRef(journal_story(1))
        } else {
            tc_journal::RecordKey::Frame(journal_story(1), FrameNo(1))
        };
        let shown = tc_app::wire::token::shown(
            &s.app
                .read(&s.scope())
                .unwrap()
                .read()
                .mint(&key, None)
                .unwrap(),
        )
        .unwrap();
        costs.push(
            cost(&s, async |s| {
                let ack = if reference {
                    crate::common::post(
                        &s.app,
                        &story::SaveRef {
                            at: s.scope(),
                            request: crate::common::rid(90),
                            shown,
                            text: "Genesis 1:1-4".to_owned(),
                        },
                    )
                    .await
                    .unwrap()
                } else {
                    crate::common::post(
                        &s.app,
                        &story::SaveFrame {
                            at: s.scope(),
                            request: crate::common::rid(90),
                            shown,
                            text: "The text of this frame was edited.".to_owned(),
                        },
                    )
                    .await
                    .unwrap()
                };
                assert_eq!(ack.stored, tc_api::write::Stored::Written);
            })
            .await,
        );
    }
    let grew = growth(&costs[0], &costs[1]);
    assert!(
        grew.is_empty(),
        "saving one story record grows with other stories: {grew:?}"
    );
    assert!(costs[0].store.commands > 0, "{:?}", costs[0]);
    assert!(costs[0].fold.keys_updated > 0, "{:?}", costs[0]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saving_a_frame_touches_one_frame_on_the_store() {
    a_story_save_costs_its_record(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saving_a_reference_touches_one_record_on_the_store() {
    a_story_save_costs_its_record(true).await;
}
