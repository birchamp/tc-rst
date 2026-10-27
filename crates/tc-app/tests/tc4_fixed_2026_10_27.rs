//! tC4 issues fixed in tC4 before 2026-10-27, replayed on the rebuild along the paths a
//! translator uses: Add a book from USFM into a project, then Translate's verse and section
//! editors, and the Align rail.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test code: a failed unwrap is a failed test; the shared support modules are \
              larger than what this file uses"
)]

#[path = "app/common.rs"]
mod common;
#[path = "support/counting_store.rs"]
mod counting_store;
#[path = "support/store_world.rs"]
mod store_world;

use std::path::Path;
use std::sync::Arc;

use tc_api::align::Index;
use tc_api::create::{AddUsfmBooks, Bible, Details, UsfmFile, UsfmOutcome};
use tc_api::editor::{Chapters, ChaptersView, SaveSection, SaveVerse, VerseRow};
use tc_api::ids::{ProjectRef, Scope, UploadId};
use tc_api::model::js_trim;
use tc_api::model::section::{SectionVerse, expand_keys, initial_text};
use tc_api::shown::Direction;
use tc_api::write::Stored;
use tc_app::StoreChoice;
use tc_core::BookId;

use common::{Parts, TestApp, open_progress, post, rid};
use counting_store::{CountingStore, SharedStore};
use store_world::{StoreWorld, assert_valid_burrito, files, store_world};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(path),
    )
    .unwrap()
}

fn ult(name: &str) -> Vec<u8> {
    fixture(&format!("tc4/test-fixtures/en_ult/{name}"))
}

/// A new Bible project in an app over a store world, ready for Add a book.
struct Project {
    dir: tempfile::TempDir,
    t: TestApp,
    world: StoreWorld,
    project: ProjectRef,
}

impl Project {
    async fn new() -> Project {
        let world = store_world();
        let store = Arc::new(CountingStore::new(SharedStore(world.store())));
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("app-data/uploads")).unwrap();
        let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
            deps.store = StoreChoice::Given(store.clone());
        })
        .await;
        let project = post(
            &t.app,
            &Bible {
                request: rid(1),
                details: Details {
                    name: "Spanish Bible".to_owned(),
                    language_name: "Spanish".to_owned(),
                    code: "es-419".to_owned(),
                    door43_code: None,
                    direction: Direction::Ltr,
                    font: String::new(),
                },
                versification: "eng".to_owned(),
            },
        )
        .await
        .unwrap()
        .project;
        Project {
            dir,
            t,
            world,
            project,
        }
    }

    /// Add a book → USFM, with the file as the upload route stores it.
    async fn add(&self, n: u8, name: &str, bytes: &[u8]) -> UsfmOutcome {
        let id = UploadId::parse(&format!("{n:032x}")).unwrap();
        std::fs::write(
            self.dir.path().join("app-data/uploads").join(id.as_str()),
            bytes,
        )
        .unwrap();
        let added = post(
            &self.t.app,
            &AddUsfmBooks {
                request: rid(10 + n),
                project: self.project.clone(),
                files: vec![UsfmFile {
                    upload: id,
                    name: name.to_owned(),
                }],
            },
        )
        .await
        .unwrap();
        added.rows[0].outcome.clone()
    }

    fn book_file(&self, book: &str) -> String {
        let all = files(&self.world.fs, &self.world.folder(&self.project));
        String::from_utf8(all[&format!("ingredients/{book}.usfm")].clone()).unwrap()
    }

    async fn open(&self) -> Scope {
        self.t
            .app
            .sessions()
            .open_screen(&self.project, open_progress())
            .await
            .unwrap()
            .at
    }

    async fn chapters(&self, at: &Scope, book: BookId) -> ChaptersView {
        post(
            &self.t.app,
            &Chapters {
                at: at.clone(),
                book,
            },
        )
        .await
        .unwrap()
    }

    async fn close(&self, at: &Scope) {
        self.t.app.sessions().leave_screen(at).unwrap();
        self.t.app.quit().await;
        assert_valid_burrito(&self.world.fs, &self.world.folder(&self.project));
    }
}

fn row<'a>(view: &'a ChaptersView, chapter: u16, key: &str) -> &'a VerseRow {
    view.chapters
        .iter()
        .find(|c| c.chapter == chapter)
        .and_then(|c| c.verses.iter().find(|v| v.key.as_str() == key))
        .unwrap_or_else(|| panic!("{chapter}:{key} is not in Translate"))
}

fn verse_count(view: &ChaptersView) -> usize {
    view.chapters.iter().map(|c| c.verses.len()).sum()
}

/// The byte range of verse `chapter:verse` in a USFM text: from its `\v` to the next `\v` or
/// `\c`, or the end.
fn verse_range(usfm: &str, chapter: u16, verse: u16) -> std::ops::Range<usize> {
    let c = usfm
        .find(&format!("\\c {chapter}\n"))
        .or_else(|| usfm.find(&format!("\\c {chapter} ")))
        .unwrap_or_else(|| panic!("no chapter {chapter}"));
    let tag = format!("\\v {verse} ");
    let start = c + usfm[c..]
        .find(&tag)
        .unwrap_or_else(|| panic!("no verse {chapter}:{verse}"));
    let body = start + tag.len();
    let end = [usfm[body..].find("\\v "), usfm[body..].find("\\c ")]
        .into_iter()
        .flatten()
        .min()
        .map_or(usfm.len(), |e| body + e);
    start..end
}

/// The words of every `\w` of each verse of an aligned fixture, as (chapter, verse, words).
fn fixture_verses(usfm: &str) -> Vec<(u16, u16, Vec<String>)> {
    let mut out = Vec::new();
    let mut chapter = 0;
    let mut at = 0;
    while let Some(next) = usfm[at..].find('\\').map(|i| at + i) {
        at = next + 1;
        let rest = &usfm[at..];
        if let Some(n) = rest.strip_prefix("c ") {
            chapter = n.split_whitespace().next().unwrap().parse().unwrap();
        } else if let Some(n) = rest.strip_prefix("v ") {
            let verse: u16 = n.split_whitespace().next().unwrap().parse().unwrap();
            let range = verse_range(usfm, chapter, verse);
            let words = usfm[range]
                .split("\\w ")
                .skip(1)
                .map(|w| w.split('|').next().unwrap().to_owned())
                .collect();
            out.push((chapter, verse, words));
        }
    }
    out
}

/// Every word, in order, is in `text`.
fn holds_in_order(text: &str, words: &[String]) -> Result<(), String> {
    assert!(!words.is_empty(), "the fixture verse has words");
    let mut from = 0;
    for word in words {
        match text[from..].find(word.as_str()) {
            Some(i) => from += i + word.len(),
            None => return Err(format!("{word:?} missing after byte {from} of {text:?}")),
        }
    }
    Ok(())
}

/// The words a verse holds in a saved USFM file, markers left out.
fn file_words(usfm: &str, chapter: u16, verse: u16) -> String {
    usfm[verse_range(usfm, chapter, verse)]
        .split_whitespace()
        .skip(2)
        .filter(|t| !t.starts_with('\\'))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Bytes outside verse `chapter:verse` are the same in both files.
fn outside_unchanged(before: &str, after: &str, chapter: u16, verse: u16) {
    let (b, a) = (
        verse_range(before, chapter, verse),
        verse_range(after, chapter, verse),
    );
    assert_eq!(
        before[..b.start],
        after[..a.start],
        "bytes before {chapter}:{verse}"
    );
    assert_eq!(
        before[b.end..],
        after[a.end..],
        "bytes after {chapter}:{verse}"
    );
}

/// tC4 #782 "Align drops words inside nested alignments (Gen 1:1 "the heavens" missing from the
/// word bank)". Steps: add Genesis from en_ult `GEN.usfm` (Add a book → USFM; here its chapter 1,
/// `GEN-1.usfm`), open Genesis 1 in Translate and on the Align rail. Every verse's Translate
/// text holds every aligned word of the file, and the Align rail's text is Translate's text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_782_align_and_translate_show_the_heavens_in_genesis_1_1() {
    let p = Project::new().await;
    let source = String::from_utf8(ult("GEN-1.usfm")).unwrap();
    assert_eq!(
        p.add(1, "GEN.usfm", source.as_bytes()).await,
        UsfmOutcome::Added {
            book: BookId::Genesis
        }
    );
    let at = p.open().await;
    let view = p.chapters(&at, BookId::Genesis).await;
    assert_eq!(
        row(&view, 1, "1").text,
        "In the beginning God created the heavens and the earth."
    );
    let expected = fixture_verses(&source);
    assert_eq!(expected.len(), 31);
    for (chapter, verse, words) in &expected {
        let text = &row(&view, *chapter, &verse.to_string()).text;
        holds_in_order(text, words).unwrap_or_else(|e| panic!("{chapter}:{verse}: {e}"));
    }
    let rail = post(
        &p.t.app,
        &Index {
            at: at.clone(),
            book: BookId::Genesis,
        },
    )
    .await
    .unwrap();
    assert_eq!(rail.items.len(), 31);
    for item in &rail.items {
        let translate = &row(&view, item.place.chapter, &item.place.verse.to_string()).text;
        let shown: String = translate.chars().take(item.text.chars().count()).collect();
        assert_eq!(
            item.text,
            shown,
            "Align's {} is Translate's",
            item.place.label()
        );
        assert!(
            !item.text.contains("  "),
            "{}: no gap where words were dropped: {:?}",
            item.place.label(),
            item.text
        );
    }
    assert!(rail.items[0].text.contains("the heavens"));
    p.close(&at).await;
}

/// tC4 #784 "Verses whose \v follows a poetry or paragraph marker on the same line are missing
/// from Translate and Align". Steps: add en_ult Psalms (Add a book → USFM), where nearly every
/// verse is written `\q1 \v N …`; Translate and the Align rail list all 2,461 verses, and Psalm
/// 3:1 (`\q1 \v 1`) has its text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_784_psalms_verses_after_q1_are_in_translate_and_align() {
    let p = Project::new().await;
    let source = fixture("scale/psa/en_ult/19-PSA.usfm");
    assert_eq!(
        p.add(1, "19-PSA.usfm", &source).await,
        UsfmOutcome::Added {
            book: BookId::Psalms
        }
    );
    let at = p.open().await;
    let view = p.chapters(&at, BookId::Psalms).await;
    assert_eq!(verse_count(&view), 2461);
    let undrafted: Vec<String> = view
        .chapters
        .iter()
        .flat_map(|c| {
            c.verses
                .iter()
                .filter(|v| !v.drafted)
                .map(move |v| format!("{}:{}", c.chapter, v.key.as_str()))
        })
        .collect();
    assert!(undrafted.is_empty(), "{undrafted:?}");
    assert!(
        row(&view, 3, "1").text.starts_with("Yahweh, how"),
        "{}",
        row(&view, 3, "1").text
    );
    let rail = post(
        &p.t.app,
        &Index {
            at: at.clone(),
            book: BookId::Psalms,
        },
    )
    .await
    .unwrap();
    assert_eq!(rail.items.len(), 2461);
    p.close(&at).await;
}

/// tC4 #786 "Editing a poetry verse that spans several lines changes only its first line; the
/// rest of the old text stays". Steps: add en_ult Psalm 3 (Add a book → USFM; verse 1 is
/// `\q1 \v 1` plus a `\q2` line, verse 2 likewise plus `\qs Selah\qs*`), save new text for 3:1 in
/// the Verse editor and for 3:2 in the Section editor. Translate and the file then hold only the
/// new text for each verse, and every byte outside the verse is unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_786_saving_a_poem_verse_replaces_every_line_of_it() {
    let p = Project::new().await;
    assert_eq!(
        p.add(1, "PSA.usfm", &ult("PSA-3.usfm")).await,
        UsfmOutcome::Added {
            book: BookId::Psalms
        }
    );
    let before = p.book_file("PSA");
    let at = p.open().await;
    let view = p.chapters(&at, BookId::Psalms).await;
    let one = row(&view, 3, "1");
    assert!(
        one.text.contains("Yahweh") && one.text.contains("against me"),
        "the box holds every poem line: {:?}",
        one.text
    );
    let new_one = "Pondré enemistad entre ti y la mujer.";
    let ack = post(
        &p.t.app,
        &SaveVerse {
            at: at.clone(),
            request: rid(40),
            shown: one.words.token.clone(),
            words: new_one.to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);

    let view = p.chapters(&at, BookId::Psalms).await;
    assert_eq!(row(&view, 3, "1").text, new_one);
    let new_two = "Muchos dicen de mí: No hay salvación para él en Dios.";
    let keys = ["1", "2"];
    let verses: Vec<SectionVerse> = keys
        .iter()
        .map(|key| {
            let r = row(&view, 3, key);
            SectionVerse {
                key: (*key).to_owned(),
                drafted: r.drafted,
                body: if *key == "2" {
                    new_two.to_owned()
                } else {
                    js_trim(&r.words.value).to_owned()
                },
                format: r.format,
            }
        })
        .collect();
    let text = initial_text(&verses, &expand_keys(&keys.map(str::to_owned)));
    let ack = post(
        &p.t.app,
        &SaveSection {
            at: at.clone(),
            request: rid(41),
            book: BookId::Psalms,
            displayed: keys
                .iter()
                .map(|k| row(&view, 3, k).words.token.clone())
                .collect(),
            text,
        },
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
    let view = p.chapters(&at, BookId::Psalms).await;
    assert_eq!(row(&view, 3, "1").text, new_one);
    assert_eq!(row(&view, 3, "2").text, new_two);
    p.close(&at).await;

    let after = p.book_file("PSA");
    assert_eq!(file_words(&after, 3, 1), new_one, "{after}");
    assert_eq!(file_words(&after, 3, 2), new_two, "{after}");
    let b1 = verse_range(&before, 3, 1);
    let b2 = verse_range(&before, 3, 2);
    let a1 = verse_range(&after, 3, 1);
    let a2 = verse_range(&after, 3, 2);
    assert_eq!(before[..b1.start], after[..a1.start], "bytes before 3:1");
    assert_eq!(before[b2.end..], after[a2.end..], "bytes after 3:2");
}

const FILES_812: [(&str, BookId, &str); 4] = [
    ("1SA-2-812.usfm", BookId::FirstSamuel, "1SA"),
    ("MAT-27-812.usfm", BookId::Matthew, "MAT"),
    ("REV-7-812.usfm", BookId::Revelation, "REV"),
    ("PSA-9-812.usfm", BookId::Psalms, "PSA"),
];

/// tC4 #812 "Editing a verse with a paragraph, stanza or \ts line in its middle changes only
/// the part before it; the rest of the old text stays". Steps: add each `*-812` en_ult extract
/// (1SA 2:2 with `\ts\*`, MAT 27:9 with `\pmo`, REV 7:12 with `\pi1`, PSA 9:15 with `\q3`) with
/// Add a book → USFM; the Verse editor shows every word of every verse; a save of new text
/// replaces the whole verse, in Translate and in the file, and leaves every other byte as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_812_verse_with_a_mid_verse_marker_shows_and_saves_whole() {
    for (name, book, code) in FILES_812 {
        let p = Project::new().await;
        let source = String::from_utf8(ult(name)).unwrap();
        assert_eq!(
            p.add(1, name, source.as_bytes()).await,
            UsfmOutcome::Added { book }
        );
        let before = p.book_file(code);
        let verses = fixture_verses(&source);
        // Without a mid-verse marker left in the added book this test would prove nothing.
        assert!(
            verses.iter().any(|(c, v, _)| {
                let body = &before[verse_range(&before, *c, *v)];
                ["\\ts\\*", "\\pmo", "\\pi", "\\q3", "\\m\n", "\\m "]
                    .iter()
                    .any(|m| {
                        body.find(m).is_some_and(|i| {
                            body[i..]
                                .split_whitespace()
                                .skip(1)
                                .any(|t| !t.starts_with('\\'))
                        })
                    })
            }),
            "{name}: {before}"
        );
        let at = p.open().await;
        let view = p.chapters(&at, book).await;
        for (chapter, verse, words) in &verses {
            let r = row(&view, *chapter, &verse.to_string());
            holds_in_order(&r.text, words)
                .unwrap_or_else(|e| panic!("{name} {chapter}:{verse} Translate: {e}"));
            holds_in_order(&r.words.value, words)
                .unwrap_or_else(|e| panic!("{name} {chapter}:{verse} editor box: {e}"));
        }
        let mut n = 50;
        for (chapter, verse, _) in &verses {
            n += 1;
            let r = row(&view, *chapter, &verse.to_string());
            let new = format!("Texto nuevo del versículo {verse}.");
            let ack = post(
                &p.t.app,
                &SaveVerse {
                    at: at.clone(),
                    request: rid(n),
                    shown: r.words.token.clone(),
                    words: new.clone(),
                },
            )
            .await
            .unwrap();
            assert_eq!(ack.stored, Stored::Written);
        }
        let view = p.chapters(&at, book).await;
        for (chapter, verse, _) in &verses {
            assert_eq!(
                row(&view, *chapter, &verse.to_string()).text,
                format!("Texto nuevo del versículo {verse}."),
                "{name} {chapter}:{verse}"
            );
        }
        p.close(&at).await;
        let after = p.book_file(code);
        for (chapter, verse, _) in &verses {
            assert_eq!(
                file_words(&after, *chapter, *verse),
                format!("Texto nuevo del versículo {verse}."),
                "{name} {chapter}:{verse} in the file:\n{after}"
            );
        }
        let (first, last) = (verses.first().unwrap(), verses.last().unwrap());
        let (b, a) = (
            verse_range(&before, first.0, first.1).start,
            verse_range(&after, first.0, first.1).start,
        );
        assert_eq!(before[..b], after[..a], "{name}: bytes before the verses");
        let (b, a) = (
            verse_range(&before, last.0, last.1).end,
            verse_range(&after, last.0, last.1).end,
        );
        assert_eq!(before[b..], after[a..], "{name}: bytes after the verses");
    }
}

/// tC4 #812, its no-change case: saving each `*-812` verse with the editor's text unchanged
/// writes nothing, and the book is byte-for-byte as it was added.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_812_no_change_save_leaves_the_book_byte_for_byte() {
    for (name, book, code) in FILES_812 {
        let p = Project::new().await;
        let source = ult(name);
        assert_eq!(p.add(1, name, &source).await, UsfmOutcome::Added { book });
        let before = p.book_file(code);
        let at = p.open().await;
        let view = p.chapters(&at, book).await;
        let mut n = 50;
        for chapter in &view.chapters {
            for r in &chapter.verses {
                n += 1;
                let ack = post(
                    &p.t.app,
                    &SaveVerse {
                        at: at.clone(),
                        request: rid(n),
                        shown: r.words.token.clone(),
                        words: r.words.value.clone(),
                    },
                )
                .await
                .unwrap();
                assert_eq!(
                    ack.stored,
                    Stored::Unchanged,
                    "{name} {}:{}",
                    chapter.chapter,
                    r.key.as_str()
                );
            }
        }
        p.close(&at).await;
        assert_eq!(p.book_file(code), before, "{name}");
    }
}

/// tC4 #767 "USFM import creates an empty book after the 4 MB size warning". Steps: open a
/// project, Add a book → USFM with a file over 4 MB (en_ult Psalms, 5.1 MB), click Add book.
/// The book is added whole (every verse with its text, no `___` placeholder) or not at all;
/// here it is added.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc4_767_a_book_over_4_mb_is_added_whole_never_empty() {
    let p = Project::new().await;
    let source = fixture("scale/psa/en_ult/19-PSA.usfm");
    assert!(source.len() > 4 * 1024 * 1024, "{}", source.len());
    assert_eq!(
        p.add(1, "19-PSA.usfm", &source).await,
        UsfmOutcome::Added {
            book: BookId::Psalms
        }
    );
    let book = p.book_file("PSA");
    assert!(!book.contains("___"), "no placeholder verse is left");
    let last = verse_range(&book, 150, 6);
    assert!(
        book[last].contains("breath"),
        "the last verse has its words"
    );
    let at = p.open().await;
    let view = p.chapters(&at, BookId::Psalms).await;
    assert_eq!(view.chapters.len(), 150);
    assert!(
        view.chapters
            .iter()
            .flat_map(|c| &c.verses)
            .all(|v| v.drafted),
        "every verse holds its text"
    );
    p.close(&at).await;
}

/// tC4 #820 "Flag ambiguous repeated-word replacements for alignment review", through the app.
/// Steps (sample burrito on the real store, with its UGNT installed): in Translate save Titus
/// 1:1 as "Dios Dios"; in Align place the first "Dios" on the Θεοῦ card and leave; in Translate
/// save the verse as "Dios Pablo". The store cannot tell which Dios stayed, so it sends the link
/// back to the bank (Q30 (a)). The translator must be told: by the save's reply, or by Align
/// when the verse opens again (its dropped-links list or the "draft changed" notice). The
/// save's `Ack` has no field for it, so the test reads what Align shows on reopen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "tC4 #820: same bug in the rebuild (link dropped silently in Translate), report 2026-10-27"]
async fn tc4_820_a_translate_save_that_sends_a_link_back_to_the_bank_tells_the_translator() {
    use tc_api::align::{
        AlignEdit, BankClick, Edit, Flush, Freshness, Open, OpenVerse, Opened, Word,
    };
    use tc_app::backend::scenario::{ScenarioState, install_resources_into};
    use tc_app::backend::scenario_store::ScenarioStore;
    use tc_app::config::ProjectsFolder;

    let dir = tempfile::tempdir().unwrap();
    let fs = tc_store::fault::FaultFs::new();
    let hooks = Arc::new(tc_store::Hooks::default());
    let state = ScenarioState::from_repository(&common::repo_root(), "sample").unwrap();
    let (backend, watch) = store_world::backend_over(
        &fs,
        store_world::APP,
        tc_store::FakePlatform::default(),
        hooks,
    );
    let (store, _) = ScenarioStore::write(backend, &state).unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let parts = Parts::new();
    parts.clock.set(store_world::START_MS);
    let mut t = TestApp::start_with(dir.path(), &parts, |config, deps| {
        config.projects = ProjectsFolder::Given(projects);
        deps.store = StoreChoice::Given(Arc::new(store));
    })
    .await;
    t.watch = Some(watch);
    let app = &t.app;
    let at = app
        .sessions()
        .open_screen(&common::folder("sample_burrito"), open_progress())
        .await
        .unwrap()
        .at;
    let save = |n: u8, words: &'static str| {
        let at = at.clone();
        async move {
            let view: ChaptersView = post(
                app,
                &Chapters {
                    at: at.clone(),
                    book: BookId::Titus,
                },
            )
            .await
            .unwrap();
            post(
                app,
                &SaveVerse {
                    at,
                    request: rid(n),
                    shown: row(&view, 1, "1").words.token.clone(),
                    words: words.to_owned(),
                },
            )
            .await
            .unwrap()
        }
    };
    let open_1_1 = |n: u8| {
        let at = at.clone();
        async move {
            match post(
                app,
                &Open {
                    at,
                    request: rid(n),
                    book: BookId::Titus,
                    verse: Some(OpenVerse {
                        chapter: 1,
                        verse: 1,
                    }),
                },
            )
            .await
            .unwrap()
            {
                Opened::Ready(view) => *view,
                other => panic!("Titus 1:1 does not open in Align: {other:?}"),
            }
        }
    };

    save(201, "Dios Dios").await;
    let view = open_1_1(202).await;
    let theou = view
        .cards
        .iter()
        .find(|c| c.top.iter().any(|w| w.word == "Θεοῦ" && w.occurrence == 1))
        .unwrap()
        .index;
    let first = Word {
        word: "Dios".into(),
        occurrence: 1,
        occurrences: 2,
    };
    let edit = |n: u8, view: &tc_api::align::AlignerView, edit: AlignEdit| {
        let request = Edit {
            at: at.clone(),
            request: rid(n),
            aligner: view.aligner,
            seq: view.seq,
            edit,
        };
        async move { post(app, &request).await.unwrap() }
    };
    // Only the first Dios stays linked, on Θεοῦ¹ (the reflow of the sample's record may have
    // kept both).
    let mut view = view;
    let mut n = 210;
    while let Some((card, word)) = view.cards.iter().find_map(|c| {
        c.placed
            .iter()
            .find(|w| !(c.index == theou && **w == first))
            .map(|w| (c.index, w.clone()))
    }) {
        view = edit(n, &view, AlignEdit::Unplace { card, word }).await;
        n += 1;
    }
    if view.placed == 0 {
        let armed = edit(
            203,
            &view,
            AlignEdit::ClickBank {
                word: first.clone(),
                click: BankClick::Plain,
            },
        )
        .await;
        view = edit(204, &armed, AlignEdit::PlaceSelection { card: theou }).await;
    }
    assert_eq!(view.placed, 1);
    post(
        app,
        &Flush {
            at: at.clone(),
            request: rid(205),
        },
    )
    .await
    .unwrap();
    let linked = open_1_1(206).await;
    assert_eq!(linked.placed, 1, "the link was saved");

    post(
        app,
        &tc_api::align::Close {
            at: at.clone(),
            request: rid(209),
            aligner: linked.aligner,
        },
    )
    .await
    .unwrap();
    // Leaving Align for Translate closes the aligner.
    let ack = save(207, "Dios Pablo").await;
    assert_eq!(ack.saved[0].text, "Dios Pablo");
    let reopened = open_1_1(208).await;
    assert_eq!(
        reopened.placed, 0,
        "the store sent the link back to the bank"
    );
    assert!(
        !reopened.dropped.is_empty() || reopened.freshness == Freshness::Stale,
        "nothing tells the translator the Dios link went back to the bank: dropped {:?}, \
         freshness {:?}",
        reopened.dropped,
        reopened.freshness
    );
}
