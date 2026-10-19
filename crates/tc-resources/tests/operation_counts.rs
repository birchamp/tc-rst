//! Loading a book's resources opens that book's files (docs/PERFORMANCE.md rule 2): the
//! files of Psalms in each installed resource and the articles its word links name, never a
//! file of another book or of a resource the project does not pin, however many are
//! installed. On Windows every file opened is a Defender scan, so the count is a budget of
//! its own.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use tc_core::BookId;
use tc_resources::{
    HelpsChoice, ResourcePin, SetSlot, install_dcs_zip, load_book, suite, with_opened_paths,
};

use common::real::{BOOK_REPOS, bundle, store_with};

fn names(paths: &[PathBuf]) -> BTreeSet<PathBuf> {
    paths.iter().cloned().collect()
}

#[test]
fn loading_a_book_opens_only_its_files() {
    let (_dir, store) = store_with(BOOK_REPOS);
    let pins = suite::project_pins();
    let (loaded, opened) =
        with_opened_paths(|| load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Psalms));
    // No file is opened twice.
    let distinct = names(&opened);
    let mut seen = BTreeSet::new();
    let twice: Vec<String> = opened
        .iter()
        .filter(|p| !seen.insert(*p))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        twice.is_empty(),
        "files opened twice: {twice:?} of {}",
        opened.len()
    );
    // Each file is Psalms' own, or an article a link of Psalms names.
    let words = loaded
        .helps
        .word_links()
        .iter()
        .map(|l| format!("{:?}", l.article))
        .collect::<BTreeSet<_>>();
    let academy = loaded
        .helps
        .notes()
        .iter()
        .filter_map(|n| n.support_reference.as_ref().map(|a| format!("{a:?}")))
        .collect::<BTreeSet<_>>();
    let (mut tw, mut ta, mut book_files) = (0usize, 0usize, 0usize);
    for path in &opened {
        let text = path.to_string_lossy();
        let in_repo = |name: &str| path.components().any(|c| c.as_os_str() == name);
        if in_repo("en_tw") {
            tw += 1;
        } else if in_repo("en_ta") {
            ta += 1;
        } else {
            assert!(
                text.contains("PSA"),
                "a file that is not Psalms' or an article: {text}"
            );
            book_files += 1;
        }
    }
    assert_eq!(tw, words.len(), "one file for each tW article Psalms links");
    // Each academy article is three files: title, sub-title and body.
    assert!(
        ta <= 3 * academy.len() && ta >= academy.len(),
        "{ta} academy files for {} articles the notes name",
        academy.len()
    );
    // The tN and TWL tables, the three Bible panes and the original-language text.
    assert!(
        (5..=8).contains(&book_files),
        "{book_files} book files: {opened:?}"
    );
    eprintln!(
        "Psalms opened {} content files: {book_files} of the book, {tw} tW articles, {ta} \
         Translation Academy files",
        opened.len()
    );

    // Ten more copies of translationNotes installed under other owners: the same files.
    let notes = suite::english_helps()
        .pin(SetSlot::TranslationNotes)
        .unwrap()
        .clone();
    let zip = std::fs::read(bundle().join("en_tn_v91.sb.zip")).unwrap();
    for n in 0..10 {
        let mut copy: ResourcePin = notes.clone();
        copy.repo = format!("git.door43.org/other{n}/en_tn").parse().unwrap();
        install_dcs_zip(&store, &copy, &zip, 1 << 30).unwrap();
    }
    let (_, again) =
        with_opened_paths(|| load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Psalms));
    assert_eq!(
        names(&again),
        distinct,
        "the same files with more resources installed"
    );
}
