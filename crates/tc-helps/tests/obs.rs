//! Open Bible Stories story files.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::fixture;
use proptest::prelude::*;
use tc_helps::obs::{ObsFrame, ObsImage, ObsStory, parse_obs_story, write_obs_story};
use tc_helps::{Error, StoryField, StoryProblem, StoryWriteProblem};

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
        // The seed files are tC4's own layout, so writing them changes no byte.
        assert_eq!(write_obs_story(&story).unwrap(), text, "story {n}");
    }
}

#[test]
fn drafted_stories_write_back_byte_for_byte() {
    for n in 1..=50 {
        let text = fixture(&format!(
            "tc4/sample-burrito-obs/ingredients/content/{n:02}.md"
        ));
        let story = parse_obs_story(&text).unwrap_or_else(|e| panic!("story {n}: {e}"));
        assert_eq!(write_obs_story(&story).unwrap(), text, "story {n}");
    }
    // An edit to one frame changes only that frame's lines.
    let text = fixture("tc4/sample-burrito-obs/ingredients/content/01.md");
    let mut story = parse_obs_story(&text).unwrap();
    story.frames[2].text = "Al tercer día Dios separó el agua de la tierra.".into();
    let written = write_obs_story(&story).unwrap();
    let changed: Vec<(&str, &str)> = text
        .lines()
        .zip(written.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(text.lines().count(), written.lines().count());
    assert_eq!(
        changed,
        [("", "Al tercer día Dios separó el agua de la tierra.")]
    );
}

#[test]
fn a_byte_order_mark_and_crlf_line_ends_read_as_the_story() {
    let text = fixture("tc4/sample-burrito-obs/ingredients/content/01.md");
    let story = parse_obs_story(&text).unwrap();
    let windows = format!("\u{feff}{}", text.replace('\n', "\r\n"));
    assert_eq!(parse_obs_story(&windows).unwrap(), story);
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
    assert_eq!(
        story_error(&format!("# 1. A\n\n{img}\n\none\rtwo\n")),
        (5, StoryProblem::CarriageReturn)
    );
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

fn write_error(story: &ObsStory) -> StoryWriteProblem {
    match write_obs_story(story) {
        Err(Error::StoryWrite(problem)) => problem,
        other => panic!("{story:?}: {other:?}"),
    }
}

fn one_frame(text: &str, reference: Option<&str>) -> ObsStory {
    ObsStory {
        number: 1,
        title: "La Creación".into(),
        frames: vec![ObsFrame {
            image: ObsImage {
                alt: "OBS Image".into(),
                url: "https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg".into(),
            },
            text: text.into(),
        }],
        reference: reference.map(str::to_owned),
    }
}

#[test]
fn stories_the_format_cannot_carry_are_refused() {
    // Two paragraphs in a frame, as a translator might type them.
    assert_eq!(
        write_error(&one_frame("Uno.\n\nDos.", None)),
        StoryWriteProblem::BlankLine { frame: 1 }
    );
    // The last line of the last frame would move into the reference.
    assert_eq!(
        write_error(&one_frame("_emphasised line_", None)),
        StoryWriteProblem::ReadsAsReference { frame: 1 }
    );
    // With a reference after it, the same text is carried.
    let carried = one_frame("_emphasised line_", Some("Génesis 1-2"));
    assert_eq!(
        parse_obs_story(&write_obs_story(&carried).unwrap()).unwrap(),
        carried
    );
    assert_eq!(
        write_error(&one_frame("![not](an image)", None)),
        StoryWriteProblem::ImageLine { frame: 1 }
    );
    assert_eq!(
        write_error(&one_frame("a\r", None)),
        StoryWriteProblem::CarriageReturn { frame: 1 }
    );
    assert_eq!(
        write_error(&one_frame("x", Some(" "))),
        StoryWriteProblem::EmptyReference
    );
    assert_eq!(
        write_error(&one_frame("x", Some("a\nb"))),
        StoryWriteProblem::LineBreak {
            field: StoryField::Reference,
            frame: None
        }
    );
    let mut story = one_frame("x", None);
    story.number = 0;
    assert_eq!(write_error(&story), StoryWriteProblem::StoryNumber(0));
    story.number = 51;
    assert_eq!(write_error(&story), StoryWriteProblem::StoryNumber(51));
    let mut story = one_frame("x", None);
    story.title = "A\nB".into();
    assert_eq!(
        write_error(&story),
        StoryWriteProblem::LineBreak {
            field: StoryField::Title,
            frame: None
        }
    );
    let mut story = one_frame("x", None);
    story.frames[0].image.alt = "a]b".into();
    assert_eq!(
        write_error(&story),
        StoryWriteProblem::ImageForm { frame: 1 }
    );
    story.frames[0].image.alt = "a\nb".into();
    assert_eq!(
        write_error(&story),
        StoryWriteProblem::LineBreak {
            field: StoryField::ImageAlt,
            frame: Some(1)
        }
    );
    let mut story = one_frame("x", Some("Génesis 1-2"));
    story.frames.clear();
    assert_eq!(
        write_error(&story),
        StoryWriteProblem::ReferenceWithoutFrames
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
        prop_assert_eq!(parse_obs_story(&write_obs_story(&story).unwrap()).unwrap(), story);
    }

    /// Any text at all: the writer refuses it, or it reads back as written.
    #[test]
    fn written_stories_read_back_or_are_refused(
        number in prop_oneof![9 => 1u8..=50, 1 => 0u8..=255],
        title in prop_oneof![9 => "[a-z ]{0,8}", 1 => "[a-z \n\r]{0,8}"],
        frames in prop::collection::vec(
            (prop_oneof![
                Just(String::new()),
                "[a-z_ \n\r!\\[\\]()]{1,24}",
                Just("![x](y)".to_owned()),
                Just("x\n_x_".to_owned()),
                Just("a\n\nb".to_owned()),
                Just("\na".to_owned()),
                Just(" ".to_owned()),
            ],
             prop_oneof![9 => "[a-z ]{0,4}", 1 => "[a-z\\]\n]{0,4}"],
             prop_oneof![9 => "[a-z./]{0,6}", 1 => "[a-z.)/\n]{0,6}"]),
            0..5,
        ),
        reference in prop::option::of(prop_oneof![
            8 => "[a-z]{1,8}",
            1 => "[a-z _\n]{0,8}",
            1 => prop_oneof![Just(String::new()), Just(" ".to_owned())],
        ]),
    ) {
        let story = ObsStory {
            number,
            title,
            frames: frames
                .into_iter()
                .map(|(text, alt, url)| ObsFrame { image: ObsImage { alt, url }, text })
                .collect(),
            reference,
        };
        if let Ok(text) = write_obs_story(&story) {
            prop_assert_eq!(parse_obs_story(&text).unwrap(), story);
        }
    }

    #[test]
    fn parsing_never_panics(text in "[\\PC\n]{0,300}") {
        let _ = parse_obs_story(&text);
        let _ = parse_obs_story(&format!("# 1. T\n\n{text}"));
    }
}
