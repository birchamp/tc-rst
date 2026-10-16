//! The story file codec reads and writes without losing a byte, and its records render back
//! into the file they came from.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{SETS, stories};
use proptest::prelude::*;
use tc_helps::obs::{ObsFrame, ObsImage, ObsStory, parse_obs_story};
use tc_obs::story::{
    StoryState, apply_story_state, decode_story, parse_story, seed_story, write_frame,
    write_reference, write_title,
};
use tc_obs::{FrameNo, StoryFile, TextProblem, WriteError};

#[test]
fn every_shipped_story_reads_back_byte_for_byte() {
    for set in SETS {
        for (i, text) in stories(set).into_iter().enumerate() {
            let file =
                StoryFile::parse(text.clone()).unwrap_or_else(|e| panic!("{set} {}: {e}", i + 1));
            assert_eq!(file.as_str(), text, "{set} {}", i + 1);
            let story = file.story();
            assert_eq!(parse_story(&text).unwrap(), story);
            // tc-helps reads gateway stories with its own parser; both agree.
            assert_eq!(parse_obs_story(&text).unwrap(), story, "{set} {}", i + 1);
            assert_eq!(usize::from(file.number().get()), i + 1);
            assert_eq!(file.frame_count(), story.frames.len());
            for (f, frame) in story.frames.iter().enumerate() {
                assert_eq!(file.text(FrameNo(f as u16 + 1)), Some(frame.text.as_str()));
                assert_eq!(
                    file.image_line(f + 1),
                    Some(frame.image.to_string().as_str())
                );
            }
            assert_eq!(file.text(FrameNo(story.frames.len() as u16 + 1)), None);
            assert_eq!(file.text(FrameNo::TITLE), Some(story.title.as_str()));
            assert_eq!(file.reference(), story.reference.as_deref());
            assert_eq!(file.into_string(), text);
        }
    }
}

#[test]
fn every_shipped_story_renders_back_from_its_records() {
    let template = stories("template");
    for set in SETS {
        for (i, text) in stories(set).into_iter().enumerate() {
            let records = decode_story(&text).unwrap();
            assert_eq!(records.render().unwrap(), text, "{set} {}", i + 1);
            let story = parse_story(&text).unwrap();
            assert_eq!(records.title.as_deref(), Some(story.title.as_str()));
            assert_eq!(records.reference, story.reference);
            assert_eq!(
                records.frames,
                story
                    .frames
                    .iter()
                    .map(|f| Some(f.text.clone()))
                    .collect::<Vec<_>>()
            );
            // The base is the template's seed form: the gateway's own stories and every
            // project story share the template's image lines.
            assert_eq!(
                records.base,
                seed_story(&template[i]).unwrap(),
                "{set} {}",
                i + 1
            );
        }
    }
}

#[test]
fn a_frame_write_changes_only_that_frames_paragraph() {
    let text = &stories("en_obs")[0];
    let written = write_frame(text, FrameNo(3), "Al tercer día.\nDios separó el agua.").unwrap();
    let before: Vec<&str> = text.split('\n').collect();
    let after: Vec<&str> = written.split('\n').collect();
    // Frame 3's one line became two; every other line is where it was, shifted by one.
    let at = before
        .iter()
        .position(|l| l.starts_with("On the second day of creation"))
        .unwrap_or(0);
    assert!(at > 0, "frame 3 of story 1");
    assert_eq!(before[..at], after[..at]);
    assert_eq!(
        after[at..at + 2],
        ["Al tercer día.", "Dios separó el agua."]
    );
    assert_eq!(before[at + 1..], after[at + 2..]);
    let file = StoryFile::parse(written).unwrap();
    assert_eq!(
        file.text(FrameNo(3)),
        Some("Al tercer día.\nDios separó el agua.")
    );
}

#[test]
fn parts_the_writers_would_refuse_stay_in_the_base_as_written() {
    let text = "# 1. Title with a trailing space \n\n![a](b)\n\nuno\n# 2. looks like a title\n\n![a](c)\n\ndos\n\n_ spaced _\n";
    let records = decode_story(text).unwrap();
    assert_eq!(records.title, None);
    assert_eq!(records.frames, [None, Some("dos".to_owned())]);
    assert_eq!(records.reference, None);
    assert_eq!(
        records.base,
        "# 1. Title with a trailing space \n\n![a](b)\n\nuno\n# 2. looks like a title\n\n![a](c)\n\n\n\n_ spaced _\n"
    );
    assert_eq!(records.render().unwrap(), text);
}

#[test]
fn whitespace_in_a_frame_region_is_layout_not_text() {
    let a = "# 4. T\n\n![a](b)\n\nuno\n\n![a](c)\n\ndos\n\n_R_\n";
    let b = "# 4. T\n\n![a](b)\nuno\n \n\u{a0}\n![a](c)\n\n\n\ndos\n\t\n_R_\n\n\n";
    assert_eq!(decode_story(a).unwrap(), decode_story(b).unwrap());
    assert_eq!(decode_story(b).unwrap().render().unwrap(), a);
}

#[test]
fn a_title_with_a_line_separator_is_refused_where_tc4_would_write_an_unreadable_story() {
    let seed = seed_story(&stories("template")[0]).unwrap();
    for title in ["a\u{2028}b", "a\u{2029}b"] {
        assert_eq!(
            write_title(&seed, title),
            Err(WriteError::TitleText(TextProblem::NotOneLine))
        );
        assert_eq!(
            write_frame(&seed, FrameNo::TITLE, title),
            Err(WriteError::TitleText(TextProblem::NotOneLine))
        );
        // What tC4 would have written cannot be read.
        assert!(parse_story(&seed.replacen("# 1.", &format!("# 1. {title}"), 1)).is_err());
    }
    // A separator inside a frame's paragraph or the reference reads back.
    let framed = write_frame(&seed, FrameNo(1), "a\u{2028}b").unwrap();
    let referenced = write_reference(&framed, "r\u{2028}s").unwrap();
    let story = parse_story(&referenced).unwrap();
    assert_eq!(story.frames[0].text, "a\u{2028}b");
    assert_eq!(story.reference.as_deref(), Some("r\u{2028}s"));
}

// ---------------------------------------------------------------------------------------
// Properties over generated stories
// ---------------------------------------------------------------------------------------

/// A story's content, and the spacing it is written with.
#[derive(Debug, Clone)]
struct Generated {
    number: u8,
    title: String,
    frames: Vec<(String, Vec<String>)>,
    reference: Option<String>,
    after_title: Vec<String>,
    /// For each frame, the blank lines before and after its paragraph.
    gaps: Vec<(Vec<String>, Vec<String>)>,
    after_reference: Vec<String>,
    final_newline: bool,
}

impl Generated {
    /// The file in this spacing, or in the writers' own when `canonical`.
    fn text(&self, canonical: bool) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(if self.title.is_empty() {
            format!("# {}.", self.number)
        } else {
            format!("# {}. {}", self.number, self.title)
        });
        lines.extend(self.after_title.iter().cloned());
        let last = self.frames.len();
        for (i, ((image, text), (before, after))) in self.frames.iter().zip(&self.gaps).enumerate()
        {
            lines.push(image.clone());
            if !canonical {
                lines.extend(before.iter().cloned());
                lines.extend(text.iter().cloned());
                lines.extend(after.iter().cloned());
            } else if text.is_empty() {
                let at_end = i + 1 == last && self.reference.is_none();
                lines.extend(std::iter::repeat_n(
                    String::new(),
                    if at_end { 6 } else { 3 },
                ));
            } else {
                lines.push(String::new());
                lines.extend(text.iter().cloned());
                lines.push(String::new());
            }
        }
        if let Some(r) = &self.reference {
            lines.push(format!("_{r}_"));
            if canonical {
                lines.push(String::new());
            } else {
                lines.extend(self.after_reference.iter().cloned());
            }
        }
        let mut text = lines.join("\n");
        if !canonical && self.final_newline {
            text.push('\n');
        }
        text
    }

    fn story(&self) -> ObsStory {
        ObsStory {
            number: self.number,
            title: self.title.clone(),
            frames: self
                .frames
                .iter()
                .map(|(image, text)| {
                    let inner = &image[2..image.len() - 1];
                    let (alt, url) = inner.split_once("](").unwrap();
                    ObsFrame {
                        image: ObsImage {
                            alt: alt.into(),
                            url: url.into(),
                        },
                        text: text.join("\n"),
                    }
                })
                .collect(),
            reference: self.reference.clone(),
        }
    }
}

fn blank() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "", "", "", " ", "\t", "\u{a0}", "\u{feff}", "\u{3000}", "\u{2028}",
    ])
    .prop_map(str::to_owned)
}

fn blanks(max: usize) -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(blank(), 0..=max)
}

/// A line of a paragraph the writers accept: not blank, and not an image, reference or
/// title line.
fn text_line() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9áéíñ«»“” ,.;:!?'()#_\\[\\]-]{1,24}".prop_filter("a writable line", |line| {
        tc_obs::story::frame_text_problem(line).is_none()
    })
}

fn image_line() -> impl Strategy<Value = String> {
    prop_oneof![
        (1u8..=50, 1u8..=20).prop_map(|(s, f)| format!(
            "![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-{s:02}-{f:02}.jpg)"
        )),
        ("[a-z (]{0,6}", "[a-z0-9./: (]{0,12}").prop_map(|(alt, url)| format!("![{alt}]({url})")),
    ]
}

fn one_line(max: usize) -> impl Strategy<Value = String> {
    proptest::string::string_regex(&format!("[a-zA-Z0-9áé :,._-]{{0,{max}}}"))
        .unwrap()
        .prop_map(|s| s.trim_matches(' ').to_owned())
}

fn generated() -> impl Strategy<Value = Generated> {
    (
        1u8..=50,
        one_line(20),
        prop::collection::vec(
            (image_line(), prop::collection::vec(text_line(), 0..=3)),
            0..=6,
        )
        .prop_filter("distinct image lines", |frames| {
            let mut images: Vec<&String> = frames.iter().map(|(image, _)| image).collect();
            images.sort();
            images.dedup();
            images.len() == frames.len()
        }),
        prop::option::of(one_line(24).prop_filter("not empty", |r| !r.is_empty())),
        blanks(2),
        prop::collection::vec((blanks(3), blanks(3)), 6),
        blanks(3),
        any::<bool>(),
    )
        .prop_map(
            |(
                number,
                title,
                frames,
                reference,
                after_title,
                gaps,
                after_reference,
                final_newline,
            )| {
                let reference = if frames.is_empty() { None } else { reference };
                let gaps = gaps.into_iter().take(frames.len()).collect();
                Generated {
                    number,
                    title,
                    frames,
                    reference,
                    after_title,
                    gaps,
                    after_reference,
                    final_newline,
                }
            },
        )
}

proptest! {
    #[test]
    fn any_story_reads_back_byte_for_byte(g in generated()) {
        for canonical in [false, true] {
            let text = g.text(canonical);
            let file = StoryFile::parse(text.clone()).unwrap();
            prop_assert_eq!(file.as_str(), text.as_str());
            prop_assert_eq!(file.story(), g.story());
        }
    }

    #[test]
    fn records_render_back_and_decode_the_same(g in generated()) {
        if g.frames.is_empty() {
            // Everything after a frameless story's title is template lines, kept as written.
            let text = g.text(false);
            prop_assert_eq!(decode_story(&text).unwrap().render().unwrap(), text);
            return Ok(());
        }
        let canonical = g.text(true);
        let records = decode_story(&canonical).unwrap();
        prop_assert_eq!(records.render().unwrap(), canonical.clone());
        // Any spacing decodes to the same records, which render in the writers' layout.
        let spaced = decode_story(&g.text(false)).unwrap();
        prop_assert_eq!(&spaced, &records);
        // Rendering then decoding gives back every record.
        let again = decode_story(&records.render().unwrap()).unwrap();
        prop_assert_eq!(again, records);
    }

    #[test]
    fn a_frame_write_reads_back_and_touches_only_its_region(
        g in generated(),
        lines in prop::collection::vec(text_line(), 0..=3),
        pick in any::<prop::sample::Index>(),
    ) {
        prop_assume!(!g.frames.is_empty());
        let text = g.text(false);
        let f = pick.index(g.frames.len()) + 1;
        let new = lines.join("\n");
        let written = write_frame(&text, FrameNo(f as u16), &new).unwrap();
        let before = StoryFile::parse(text.clone()).unwrap();
        let after = StoryFile::parse(written.clone()).unwrap();
        prop_assert_eq!(after.text(FrameNo(f as u16)), Some(new.as_str()));
        prop_assert_eq!(after.title(), before.title());
        prop_assert_eq!(after.reference(), before.reference());
        for other in (1..=g.frames.len()).filter(|o| *o != f) {
            prop_assert_eq!(after.text(FrameNo(other as u16)), before.text(FrameNo(other as u16)));
            prop_assert_eq!(after.image_line(other), before.image_line(other));
        }
        // Every byte up to the frame's image line, and from the next image line or the
        // reference line on, is unchanged.
        let image = before.image_line(f).unwrap();
        let head = &text[..text.find(&format!("\n{image}")).unwrap() + image.len() + 1];
        prop_assert!(written.starts_with(head));
        if let Some(next) = before.image_line(f + 1) {
            let tail = &text[text.find(&format!("\n{next}")).unwrap()..];
            prop_assert!(written.ends_with(tail));
        }
        // Writing the same text again changes nothing.
        prop_assert_eq!(write_frame(&written, FrameNo(f as u16), &new).unwrap(), written);
    }

    #[test]
    fn title_and_reference_writes_read_back(g in generated(), title in one_line(16), reference in one_line(16)) {
        let text = g.text(false);
        let titled = write_title(&text, &title).unwrap();
        prop_assert_eq!(parse_story(&titled).unwrap().title, title.clone());
        if !g.frames.is_empty() && !reference.is_empty() {
            let referenced = write_reference(&titled, &reference).unwrap();
            let story = parse_story(&referenced).unwrap();
            prop_assert_eq!(story.reference.as_deref(), Some(reference.as_str()));
            prop_assert_eq!(story.title, title);
            prop_assert_eq!(story.frames, g.story().frames);
            prop_assert_eq!(write_reference(&referenced, &reference).unwrap(), referenced);
        }
    }

    #[test]
    fn a_state_applied_onto_its_own_base_is_the_file(g in generated()) {
        let canonical = g.text(true);
        let records = decode_story(&canonical).unwrap();
        prop_assert_eq!(apply_story_state(&records.base, &records.state()).unwrap(), canonical);
        // An empty state leaves any file as it is.
        let text = g.text(false);
        prop_assert_eq!(apply_story_state(&text, &StoryState::default()).unwrap(), text);
    }
}
