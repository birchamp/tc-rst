//! The count row of B4b (§8.2): a section save costs what the section touches, never the size
//! of the project or its history. It runs on the store, where a save is real, over a Psalms
//! chapter that is the same in both corpora, with the section's tokens read before it.

use std::sync::Mutex;

use tc_api::editor::SaveSection;
use tc_api::editor::{Chapters, ChaptersView};
use tc_api::model::js_trim;
use tc_api::model::section::{SectionVerse, expand_keys, initial_text};
use tc_api::write::Stored;

use super::corpus::{PSALMS, corpora};
use super::harness::{Backing, costs_on_both, growth};
use crate::common::{post, rid};

/// The first three verses of `chapter` as a card opens them, with the second one changed.
fn section(view: &ChaptersView, chapter: u16) -> (Vec<tc_api::ShownToken>, String) {
    let rows: Vec<_> = view
        .chapters
        .iter()
        .find(|c| c.chapter == chapter)
        .unwrap()
        .verses
        .iter()
        .take(3)
        .collect();
    assert_eq!(rows.len(), 3, "the chapter has three verses to a section");
    let tokens = rows
        .iter()
        .flat_map(|r| std::iter::once(r.words.token.clone()).chain(r.covered.iter().cloned()))
        .collect();
    let keys: Vec<String> = rows.iter().map(|r| r.key.as_str().to_owned()).collect();
    let verses: Vec<SectionVerse> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| SectionVerse {
            key: r.key.as_str().to_owned(),
            drafted: true,
            body: if i == 1 {
                format!("{} and more", js_trim(&r.words.value))
            } else {
                js_trim(&r.words.value).to_owned()
            },
            format: r.format,
        })
        .collect();
    (tokens, initial_text(&verses, &expand_keys(&keys)))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn section_save_touches_one_section() {
    let chapter = corpora().chapter;
    let views = Mutex::new(Vec::new());
    let (small, large) = costs_on_both(
        Backing::Store,
        async |s| {
            let at = s.open().await;
            let view = post(&s.app, &Chapters { at, book: PSALMS }).await.unwrap();
            views.lock().unwrap().push(view);
        },
        async |s| {
            let view = views.lock().unwrap().pop().unwrap();
            let (displayed, text) = section(&view, chapter);
            let ack = post(
                &s.app,
                &SaveSection {
                    at: s.scope(),
                    request: rid(1),
                    book: PSALMS,
                    displayed,
                    text,
                },
            )
            .await
            .unwrap();
            assert_eq!(ack.stored, Stored::Written);
        },
    )
    .await;
    // The store's own work of a save grows with the project in tc-store, for a verse's save as
    // for a section's (its fold reads and the bytes it reads back are its budget, P in the
    // plan); the app's counts are this row's.
    let mut grew = growth(&small, &large);
    grew.retain(|g| g.starts_with("app "));
    assert!(
        grew.is_empty(),
        "a section save grows with the project:\n{}",
        grew.join("\n")
    );
    assert!(small.store.commands >= 1, "{small:?}");
    assert!(
        small.app.records_read <= 16,
        "a three-verse section reads its own records: {small:?}"
    );
}
