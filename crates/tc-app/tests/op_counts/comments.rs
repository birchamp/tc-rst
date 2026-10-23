//! C3 comment writes use their target and displayed comment, never a book or its history.

use std::sync::Mutex;

use tc_api::editor::{AddComment, CommentTarget, Draft, DraftField};
use tc_api::ids::WallMs;
use tc_api::write::Stored;
use tc_journal::RecordKey;

use super::corpus::{PSALMS, corpora};
use super::harness::{Backing, costs_on_both, growth};
use crate::common::{post, rid};

#[derive(Clone, Copy)]
enum Action {
    Add,
    Revise,
    Draft,
}

async fn measured(action: Action) {
    let inputs = Mutex::new(Vec::new());
    let (small, large) = costs_on_both(
        Backing::Store,
        async |s| {
            s.open().await;
            let session = s.app.read(&s.scope()).unwrap();
            let key = session
                .read()
                .chapter_keys(PSALMS, corpora().chapter)
                .verses[0]
                .clone();
            let RecordKey::Verse(book, chapter, verse) = key else {
                panic!("a verse target")
            };
            let target = CommentTarget::Verse {
                book,
                chapter: chapter.get(),
                verse: verse.get(),
            };
            let shown = if matches!(action, Action::Revise) {
                let ack = post(
                    &s.app,
                    &AddComment {
                        at: s.scope(),
                        request: rid(30),
                        target,
                        shown: None,
                        text: "the displayed note".to_owned(),
                    },
                )
                .await
                .unwrap();
                Some(ack.tokens[0].token.clone())
            } else {
                None
            };
            inputs.lock().unwrap().push((target, shown));
        },
        async |s| {
            let (target, shown) = inputs.lock().unwrap().pop().unwrap();
            if matches!(action, Action::Draft) {
                post(
                    &s.app,
                    &Draft {
                        at: s.scope(),
                        request: rid(31),
                        instance: s.app.instance(),
                        field: DraftField::UserComment { target },
                        shown: Vec::new(),
                        text: "a note still being typed".to_owned(),
                        changed: WallMs(100),
                    },
                )
                .await
                .unwrap();
            } else {
                let ack = post(
                    &s.app,
                    &AddComment {
                        at: s.scope(),
                        request: rid(31),
                        target,
                        shown,
                        text: "a confirmed note".to_owned(),
                    },
                )
                .await
                .unwrap();
                assert_eq!(ack.stored, Stored::Written);
                assert_eq!(ack.saved[0].text, "a confirmed note");
            }
        },
    )
    .await;
    let grew = growth(&small, &large);
    assert!(
        grew.is_empty(),
        "a comment action grows with the project: {grew:?}"
    );
    if matches!(action, Action::Draft) {
        assert!(small.store.drafts_written > 0, "{small:?}");
        assert_eq!(small.fold.keys_updated, 0);
    } else {
        assert!(small.store.commands > 0, "{small:?}");
        assert!(small.fold.keys_updated > 0, "{small:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adding_a_comment_touches_one_target() {
    measured(Action::Add).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revising_a_comment_touches_one_comment() {
    measured(Action::Revise).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_comment_draft_touches_one_target() {
    measured(Action::Draft).await;
}
