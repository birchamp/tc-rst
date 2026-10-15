//! Open Bible Stories story files.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::fixture;
use proptest::prelude::*;
use tc_helps::obs::{ObsFrame, ObsImage, ObsStory, parse_obs_story, write_obs_story};
use tc_helps::{Error, StoryProblem};

#[test]
fn a_drafted_story_reads_title_frames_and_reference() {
    let story =
        parse_obs_story(&fixture("tc4/sample-burrito-obs/ingredients/content/01.md")).unwrap();
    assert_eq!(story.number, 1);
    assert_eq!(story.title, "La Creación");
    assert_eq!(story.frames.len(), 16);
    assert!(
        story.frames[0]
            .text
            .starts_with("Así fue como Dios hizo todo en el principio.")
    );
    // Frame 1 is two lines of one paragraph.
    assert_eq!(story.frames[0].text.lines().count(), 2);
    assert_eq!(story.frames[2].text, "");
    assert_eq!(story.frames[1].image.picture(), Some((1, 2)));
    assert_eq!(
        story.frames[1].image.url,
        "https://cdn.door43.org/obs/jpg/360px/obs-en-01-02.jpg"
    );
    assert_eq!(
        story.reference.as_deref(),
        Some("Una historia bíblica de: Génesis 1-2")
    );
}

#[test]
fn all_fifty_template_stories_parse_as_empty_frames() {
    for n in 1..=50 {
        let text = fixture(&format!(
            "tc4/conformance-fixtures/text_stories/ingredients/content/{n:02}.md"
        ));
        let story = parse_obs_story(&text).unwrap_or_else(|e| panic!("story {n}: {e}"));
        assert_eq!(usize::from(story.number), n);
        assert!(!story.frames.is_empty());
        assert!(story.frames.iter().all(|f| f.text.is_empty()));
        assert!(
            story
                .frames
                .iter()
                .enumerate()
                .all(|(i, f)| f.image.picture() == Some((story.number, i as u16 + 1)))
        );
        assert_eq!(parse_obs_story(&write_obs_story(&story)).unwrap(), story);
    }
}

fn story_error(text: &str) -> (usize, StoryProblem) {
    match parse_obs_story(text) {
        Err(Error::Story { line, problem }) => (line, problem),
        other => panic!("{text:?}: {other:?}"),
    }
}

#[test]
fn malformed_stories_are_refused_naming_the_line() {
    let img = "![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg)";
    assert_eq!(story_error("# 1. A\r\n"), (1, StoryProblem::CarriageReturn));
    assert_eq!(story_error("Title\n"), (1, StoryProblem::Title));
    assert_eq!(
        story_error("# 51. Too far\n"),
        (1, StoryProblem::StoryNumber(51))
    );
    assert_eq!(
        story_error("# 1. A\n\nstray\n"),
        (3, StoryProblem::TextBeforeFirstFrame)
    );
    assert_eq!(
        story_error(&format!("# 1. A\n\n{img}\n\none\n\ntwo\n")),
        (7, StoryProblem::SecondParagraph { frame: 1 })
    );
    assert_eq!(
        story_error(&format!("# 1. A\n\n{img}\n\n_ _\n")),
        (5, StoryProblem::EmptyReference)
    );
    assert_eq!(
        story_error("# 1. A\n\n_ref_\n"),
        (3, StoryProblem::ReferenceWithoutFrames)
    );
}

fn frame_text() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-zA-Z0-9 ,.;:'’“”!?áéíóúñ-]{1,40}", 0..3).prop_map(|lines| {
        lines
            .into_iter()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty() && !(l.starts_with('_') && l.ends_with('_')))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

proptest! {
    #[test]
    fn stories_round_trip(number in 1u8..=50,
                          title in "[a-zA-Z ]{0,20}",
                          frames in prop::collection::vec(frame_text(), 1..20),
                          reference in prop::option::of("[a-zA-Z0-9 :-]{1,30}")) {
        let story = ObsStory {
            number,
            title: title.trim().to_owned(),
            frames: frames
                .into_iter()
                .enumerate()
                .map(|(i, text)| ObsFrame {
                    image: ObsImage {
                        alt: "OBS Image".into(),
                        url: format!("https://cdn.door43.org/obs/jpg/360px/obs-en-{number:02}-{:02}.jpg", i + 1),
                    },
                    text,
                })
                .collect(),
            reference: reference.map(|r| r.trim().to_owned()).filter(|r| !r.is_empty()),
        };
        prop_assert_eq!(parse_obs_story(&write_obs_story(&story)).unwrap(), story);
    }

    #[test]
    fn parsing_never_panics(text in "[\\PC\n]{0,300}") {
        let _ = parse_obs_story(&text);
        let _ = parse_obs_story(&format!("# 1. T\n\n{text}"));
    }
}
