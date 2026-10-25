//! INV-IDS-ROUND-TRIP and INV-ZERO-IS-NOT-A-VERSE for tc-core's identifiers.

use proptest::prelude::*;
use std::fmt::{Debug, Display};
use std::str::FromStr;
use tc_core::{BookId, ChapterNo, FrameNo, GitOid, Md5, Reference, StoryNo, VerseNo, VerseSpan};

/// A value that parses prints a text that parses back to it.
fn round_trips<T: FromStr + Display + PartialEq + Debug>(text: &str) -> Result<(), TestCaseError> {
    if let Ok(value) = text.parse::<T>() {
        let printed = value.to_string();
        prop_assert_eq!(
            printed.parse::<T>().ok(),
            Some(value),
            "{:?} printed {:?}",
            text,
            printed
        );
    }
    Ok(())
}

/// An identifier the app writes: a text that parses prints back exactly as written.
fn prints_back<T: FromStr + Display>(text: &str) -> Result<(), TestCaseError> {
    if let Ok(value) = text.parse::<T>() {
        prop_assert_eq!(value.to_string(), text);
    }
    Ok(())
}

/// A span written with the same verse at both ends (`40-40`), which reads as that verse and
/// is written without the dash.
fn same_ends(text: &str) -> bool {
    let verses = text.rsplit(':').next().unwrap_or(text);
    verses.split_once('-').is_some_and(|(a, b)| a == b)
}

proptest! {
    #[test]
    fn ids_round_trip(
        book in "[A-Za-z1-3]{3}",
        // Numbers as the app writes them, or with a sign or a space a parser must refuse. A
        // leading zero is left out: a USFM file's verse label may have one, read as the
        // number.
        span in "[+ ]?[1-9][0-9]{0,2}(-[+ ]?[1-9][0-9]{0,2})?",
        reference in "(TIT|JON|3JN|tit) [+ ]?[1-9][0-9]{0,2}:[+ ]?[1-9][0-9]{0,2}(-[+ ]?[1-9][0-9]{0,2})?",
        hex in "[0-9a-f]{32}|[0-9a-f]{40}|[0-9a-fA-F]{32}|[0-9a-fA-F]{40}",
    ) {
        round_trips::<BookId>(&book)?;
        round_trips::<VerseSpan>(&span)?;
        round_trips::<Reference>(&reference)?;
        round_trips::<GitOid>(&hex)?;
        round_trips::<Md5>(&hex)?;
        // Book codes are read in either case (tC4's files have both) and written upper.
        prints_back::<BookId>(&book.to_ascii_uppercase())?;
        if !same_ends(&span) {
            prints_back::<VerseSpan>(&span)?;
        }
        if !same_ends(&reference) {
            prints_back::<Reference>(&reference.replace("tit", "TIT"))?;
        }
        prints_back::<GitOid>(&hex)?;
        prints_back::<Md5>(&hex)?;
    }
}

#[test]
fn zero_is_not_a_chapter_verse_or_story() {
    assert_eq!(ChapterNo::new(0), None);
    assert_eq!(VerseNo::new(0), None);
    assert_eq!(StoryNo::new(0), None);
    assert_eq!(StoryNo::new(51), None);
    assert!(serde_json::from_str::<StoryNo>("0").is_err());
    assert!("TIT 0:1".parse::<Reference>().is_err());
    assert!("TIT 1:0".parse::<Reference>().is_err());
    assert!("0".parse::<VerseSpan>().is_err());
    // Frame 0 is a story's title, by name.
    assert_eq!(FrameNo::new(0), FrameNo::TITLE);
    assert!(FrameNo::TITLE.is_title());
    assert_eq!(StoryNo::all().count(), 50);
}
