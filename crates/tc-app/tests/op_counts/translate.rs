//! Production Translate saves and drafts cost their verse, across Psalms and ten times it.

use std::sync::Mutex;

use tc_api::editor::{Draft, DraftField, SaveLayout, SaveVerse};
use tc_api::ids::WallMs;
use tc_api::write::Stored;
use tc_app::backend::memory::Scripted;
use tc_app::wire::token::shown;
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};

use super::corpus::{PSALMS, corpora};
use super::harness::{OnStore, Scale, costs_what_it_touches};
use crate::common::{post, rid};

#[derive(Clone, Copy)]
enum Action {
    Words,
    Layout,
    Draft,
}

async fn measured(action: Action) {
    let inputs = Mutex::new(Vec::new());
    let (_, (stored, _)) = costs_what_it_touches(
        "a production verse save or draft",
        OnStore::Everything,
        async |s: &Scale| {
            s.open().await;
            let session = s.app.read(&s.scope()).unwrap();
            let read = session.read();
            let key = read.chapter_keys(PSALMS, corpora().chapter).verses[0].clone();
            let RecordKey::Verse(_, _, _) = key else {
                panic!("a verse key")
            };
            let Some(Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            }) = read.projected(&key)
            else {
                panic!("a drafted verse")
            };
            inputs
                .lock()
                .unwrap()
                .push((shown(&read.mint(&key, None).unwrap()).unwrap(), words));
            if !matches!(action, Action::Draft)
                && let Some(memory) = s.memory()
            {
                memory.script(&s.folder, Scripted::Written).unwrap();
            }
        },
        async |s: &Scale| {
            let (shown, words) = inputs.lock().unwrap().pop().unwrap();
            match action {
                Action::Words => {
                    let ack = post(
                        &s.app,
                        &SaveVerse {
                            at: s.scope(),
                            request: rid(1),
                            shown,
                            words: format!("{words} and more"),
                        },
                    )
                    .await
                    .unwrap();
                    assert_eq!(ack.stored, Stored::Written);
                }
                Action::Layout => {
                    let ack = post(
                        &s.app,
                        &SaveLayout {
                            at: s.scope(),
                            request: rid(1),
                            shown,
                            layout: "\n\\q2\n".to_owned(),
                        },
                    )
                    .await
                    .unwrap();
                    assert_eq!(ack.stored, Stored::Written);
                    assert_eq!(ack.saved[0].text, "\n\\q2\n");
                }
                Action::Draft => {
                    post(
                        &s.app,
                        &Draft {
                            at: s.scope(),
                            request: rid(1),
                            instance: s.app.instance(),
                            field: DraftField::Words,
                            shown: vec![shown],
                            text: format!("{words} and more"),
                            changed: WallMs(100),
                        },
                    )
                    .await
                    .unwrap();
                }
            }
        },
    )
    .await;
    if matches!(action, Action::Draft) {
        assert!(stored.store.drafts_written > 0, "{stored:?}");
        assert_eq!(
            stored.fold.keys_updated, 0,
            "a draft changes no journal value"
        );
    } else {
        assert!(stored.store.commands > 0, "{stored:?}");
        assert!(stored.fold.keys_updated > 0, "{stored:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn save_verse_touches_one_verse() {
    measured(Action::Words).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn save_layout_touches_one_verse() {
    measured(Action::Layout).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_verse_draft_touches_one_verse() {
    measured(Action::Draft).await;
}
