//! Which original-language verse a project verse is aligned against: tC4's
//! `test/alignmentFrame.test.ts`, ported. A project in another scheme names a different
//! source verse with the same numbers, so the lookup maps project to `eng` and refuses
//! rather than align against the wrong Greek or Hebrew.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tc_align::frame::{NoSourceVerse, RESOURCE_FRAME, source_reference};
use tc_core::{BookId, ChapterNo, Reference, VerseNo, VerseSpan};
use tc_versification::{MapOutcome, RowReference, StandardScheme, Unplaceable, map_reference};

fn reference(book: BookId, chapter: u16, verse: u16) -> Reference {
    Reference::new(
        book,
        ChapterNo::new(chapter).unwrap(),
        VerseSpan::single(VerseNo::new(verse).unwrap()),
    )
}

fn source(
    project: StandardScheme,
    book: BookId,
    chapter: u16,
    verse: u16,
) -> Result<(u16, u16), NoSourceVerse> {
    source_reference(Some(project.scheme()), reference(book, chapter, verse))
        .map(|r| (r.chapter, r.verses.start()))
}

#[test]
fn a_non_eng_project_maps_the_source_lookup() {
    use StandardScheme::*;
    // rsc JON 1 ends at verse 16, so eng JON 1:17 is rsc JON 2:1.
    assert_eq!(source(Rsc, BookId::Jonah, 2, 1), Ok((1, 17)));
    assert_eq!(source(Rsc, BookId::Jonah, 2, 11), Ok((2, 10)));
    // rsc counts a psalm's superscription as verse 1; eng does not.
    assert_eq!(source(Rsc, BookId::Psalms, 3, 2), Ok((3, 1)));
    assert_eq!(source(Rso, BookId::Psalms, 12, 6), Ok((13, 5)));
    assert_eq!(source(Vul, BookId::Malachi, 4, 1), Ok((4, 1)));
}

#[test]
fn an_eng_project_looks_up_its_own_numbers() {
    assert_eq!(source(StandardScheme::Eng, BookId::Jonah, 2, 1), Ok((2, 1)));
}

#[test]
fn the_two_directions_round_trip_for_the_verses_alignment_uses() {
    use StandardScheme::*;
    for (project, book, chapter, verse) in [
        (Rsc, BookId::Jonah, 1, 17),
        (Rsc, BookId::Psalms, 3, 1),
        (Lxx, BookId::Genesis, 31, 55),
        (Rso, BookId::Psalms, 13, 5),
    ] {
        let MapOutcome::Mapped(to_project) = map_reference(
            RESOURCE_FRAME.scheme(),
            project.scheme(),
            &RowReference::from(reference(book, chapter, verse)),
        ) else {
            panic!(
                "{book} {chapter}:{verse} does not map into {}",
                project.name()
            );
        };
        let back =
            source_reference(Some(project.scheme()), to_project.to_reference().unwrap()).unwrap();
        assert_eq!(
            (back.book.book_id(), back.chapter, back.verses.start()),
            (Some(book), chapter, verse)
        );
    }
}

#[test]
fn an_unknown_project_frame_has_no_source_reference() {
    assert_eq!(
        source_reference(None, reference(BookId::Jonah, 2, 1)),
        Err(NoSourceVerse::UnknownFrame)
    );
}

#[test]
fn a_psalm_superscription_and_content_eng_lacks_refuse() {
    use StandardScheme::*;
    for project in [Rsc, Vul] {
        assert_eq!(
            source(project, BookId::Psalms, 3, 1),
            Err(NoSourceVerse::Unplaceable(Unplaceable::VerseZero))
        );
    }
    assert_eq!(
        source(Rso, BookId::Joshua, 24, 34),
        Err(NoSourceVerse::Unplaceable(Unplaceable::PastChapterEnd))
    );
    assert_eq!(
        source(Lxx, BookId::Ezra, 14, 18),
        Err(NoSourceVerse::Unplaceable(Unplaceable::NoChapter))
    );
}
