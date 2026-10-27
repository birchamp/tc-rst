//! The story file codec reads and writes without losing a byte, and its records render back
//! into the file they came from.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{SETS, stories};
use proptest::prelude::*;
use tc_helps::obs::{ObsFrame, ObsImage, ObsStory, parse_obs_story};
use tc_obs::story::{
    StoryRecords, StoryState, apply_story_state, decode_story, frame_text_problem, parse_story,
    seed_story, write_frame, write_reference, write_title,
};
use tc_obs::{FrameNo, Reformatted, StoryError, StoryFile, StoryNo, TextProblem, WriteError};

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
                assert_eq!(
                    file.text(FrameNo::new(f as u16 + 1)),
                    Some(frame.text.as_str())
                );
                assert_eq!(
                    file.image_line(f + 1),
                    Some(frame.image.to_string().as_str())
                );
            }
            assert_eq!(file.text(FrameNo::new(story.frames.len() as u16 + 1)), None);
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
            let decoded = decode_story(&text).unwrap();
            assert_eq!(decoded.reformatted, [], "{set} {}", i + 1);
            let records = decoded.records;
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

/// The keys whose decoded value differs from the value written, as STORAGE §11.1 X3 finds
/// them for a story: `StoryBase`, `Frame` (frame 0 the title) and `StoryRef`.
fn changed_keys(found: &StoryRecords, written: &StoryRecords) -> Vec<String> {
    let mut keys = Vec::new();
    if found.base != written.base {
        keys.push("StoryBase".to_owned());
    }
    if found.title != written.title {
        keys.push("Frame 0".to_owned());
    }
    let frames = found.frames.len().max(written.frames.len());
    for i in 0..frames {
        if found.frames.get(i).cloned().flatten() != written.frames.get(i).cloned().flatten() {
            keys.push(format!("Frame {}", i + 1));
        }
    }
    if found.reference != written.reference {
        keys.push("StoryRef".to_owned());
    }
    keys
}

/// What a new project's store holds for a story: its `StoryBase`, the template's seed form,
/// and no other record (tC4 journals nothing at creation; STORAGE §5.15).
fn new_project_story(n: usize, template: &str) -> StoryRecords {
    StoryRecords {
        story: StoryNo::new(u8::try_from(n).unwrap()).unwrap(),
        base: seed_story(template).unwrap(),
        title: None,
        frames: vec![None; parse_story(template).unwrap().frames.len()],
        reference: None,
    }
}

#[test]
fn an_untouched_story_of_a_new_project_decodes_to_the_records_its_store_shows() {
    for (i, template) in stories("template").iter().enumerate() {
        let store = new_project_story(i + 1, template);
        let file = store.render().unwrap();
        assert_eq!(file, store.base);
        let found = decode_story(&file).unwrap();
        assert_eq!(found.reformatted, []);
        // The decoder records the empty title and each empty frame, which the store never
        // wrote: compared with the store's own records, all of them read as changed.
        assert_eq!(
            changed_keys(&found.records, &store).len(),
            1 + store.frames.len()
        );
        // Compared with what those records decode to, nothing changed.
        let written = store.as_decoded().unwrap();
        assert_eq!(changed_keys(&found.records, &written), Vec::<String>::new());
        assert_eq!(written.base, store.base);
        assert_eq!(written.title.as_deref(), Some(""));
        assert!(written.frames.iter().all(|f| f.as_deref() == Some("")));
        assert_eq!(written.reference, None);
    }
}

#[test]
fn an_outside_edit_of_a_new_projects_story_changes_only_the_parts_it_edits() {
    let template = &stories("template")[0];
    let store = new_project_story(1, template);
    let edited = write_title(&store.render().unwrap(), "La creación").unwrap();
    let edited = write_frame(&edited, FrameNo::new(3), "Al tercer día.").unwrap();
    let edited = write_reference(&edited, "Génesis 1-2").unwrap();
    let found = decode_story(&edited).unwrap().records;
    assert_eq!(
        changed_keys(&found, &store.as_decoded().unwrap()),
        ["Frame 0", "Frame 3", "StoryRef"]
    );
    // A frame cleared outside the app is a change of a frame the store holds text for.
    let mut drafted = store.clone();
    drafted.frames[1] = Some("Dios creó.".to_owned());
    let cleared = write_frame(&drafted.render().unwrap(), FrameNo::new(2), "").unwrap();
    assert_eq!(cleared, store.render().unwrap());
    assert_eq!(
        changed_keys(
            &decode_story(&cleared).unwrap().records,
            &drafted.as_decoded().unwrap()
        ),
        ["Frame 2"]
    );
}

#[test]
fn a_cleared_frame_and_a_frame_never_written_are_the_same_file_and_the_same_records() {
    let template = &stories("template")[0];
    let never = new_project_story(1, template);
    let mut cleared = never.clone();
    cleared.frames[2] = Some(String::new());
    cleared.title = Some(String::new());
    assert_eq!(cleared.render().unwrap(), never.render().unwrap());
    assert_eq!(cleared.as_decoded().unwrap(), never.as_decoded().unwrap());
    // A record with text is its text.
    let mut drafted = never.clone();
    drafted.frames[2] = Some("Al tercer día.".to_owned());
    assert_eq!(
        drafted.as_decoded().unwrap().frames[2].as_deref(),
        Some("Al tercer día.")
    );
}

#[test]
fn a_frame_write_changes_only_that_frames_paragraph() {
    let text = &stories("en_obs")[0];
    let written = write_frame(
        text,
        FrameNo::new(3),
        "Al tercer día.\nDios separó el agua.",
    )
    .unwrap();
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
        file.text(FrameNo::new(3)),
        Some("Al tercer día.\nDios separó el agua.")
    );
}

#[test]
fn parts_the_writers_would_refuse_stay_in_the_base_as_written() {
    let text = "# 1. Title with a trailing space \n\n![a](b)\n\nuno\n# 2. looks like a title\n\n![a](c)\n\ndos\n\n_ spaced _\n";
    let records = decode_story(text).unwrap().records;
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
fn whitespace_no_record_keeps_is_named_where_rendering_rewrites_it() {
    let a = "# 4. T\n\n![a](b)\n\nuno\n\n![a](c)\n\ndos\n\n_R_\n";
    let b = "# 04. T\n\n![a](b)\nuno\n \n\u{a0}\n![a](c)\n\n\n\ndos\n\t\n_R_\n\n\n";
    let (a, b) = (decode_story(a).unwrap(), decode_story(b).unwrap());
    assert_eq!(a.reformatted, []);
    // The same records: the layout is no record's.
    assert_eq!(b.records, a.records);
    // So rendering them writes the writers' layout, and the decoder says where.
    assert_eq!(
        b.reformatted,
        [
            Reformatted::Title,
            Reformatted::Frame(FrameNo::new(1)),
            Reformatted::Frame(FrameNo::new(2)),
            Reformatted::Reference,
        ]
    );
    // One blank line each side, but one of them holds a space or a tab.
    for spaced in [
        "# 4.\n\n![a](b)\n\nuno\n \n![a](c)\n\ndos\n",
        "# 4.\n\n![a](b)\n\t\nuno\n\n![a](c)\n\ndos\n",
    ] {
        assert_eq!(
            decode_story(spaced).unwrap().reformatted,
            [Reformatted::Frame(FrameNo::new(1))]
        );
    }
    // Empty frames, and a file without a line end after its reference line.
    let empty = "# 4.\n\n![a](b)\n\n\n![a](c)\n\n\n\n_R_";
    assert_eq!(
        decode_story(empty).unwrap().reformatted,
        [Reformatted::Frame(FrameNo::new(1)), Reformatted::Reference]
    );
    let last = "# 4.\n\n![a](b)\n\n\n\n![a](c)\n\n\n\n";
    assert_eq!(
        decode_story(last).unwrap().reformatted,
        [Reformatted::Frame(FrameNo::new(2))]
    );
    // A frame the writers refuse keeps its layout in the base.
    let kept = "# 4.\n\n![a](b)\n \nuno\n_x_\n\n\n![a](c)\n\ndos\n";
    let decoded = decode_story(kept).unwrap();
    assert_eq!(decoded.records.frames, [None, Some("dos".to_owned())]);
    assert_eq!(decoded.reformatted, []);
    assert_eq!(decoded.records.render().unwrap(), kept);
}

/// The reference line stays in the base when the last frame's paragraph does: a last line
/// in the reference form must not become the base's reference line.
#[test]
fn a_last_paragraph_the_writers_refuse_keeps_its_closing_underscore_line() {
    for (text, frames) in [
        (
            "# 1. T\n\n![a](b)\n\nIn the beginning.\n_God made it all._\n\n_A Bible story from: Genesis 1-2_\n",
            vec![None],
        ),
        (
            "# 1. T\n\n![a](b)\n\nIn the beginning.\n_God made it all._\n_A Bible story from: Genesis 1-2_\n",
            vec![None],
        ),
        (
            "# 1. T\n\n![a](b)\n\nuno\n\n![a](c)\n\ndos\n_emphasis_\n\n_A Bible story from: Genesis 1-2_\n",
            vec![Some("uno".to_owned()), None],
        ),
        (
            "# 1. T\n\n![a](b)\n\nIn the beginning.\n__\n\n_A Bible story from: Genesis 1-2_\n",
            vec![None],
        ),
    ] {
        let decoded = decode_story(text).unwrap();
        assert_eq!(decoded.records.frames, frames, "{text:?}");
        assert_eq!(
            decoded.records.reference.as_deref(),
            Some("A Bible story from: Genesis 1-2")
        );
        assert_eq!(decoded.reformatted, []);
        let rendered = decoded.records.render().unwrap();
        assert_eq!(rendered, text);
        assert_eq!(parse_story(&rendered), parse_story(text));
        // A new reference replaces the line in place, and the paragraph stays.
        let mut records = decoded.records.clone();
        records.reference = Some("Génesis 1-2".to_owned());
        let story = parse_story(&records.render().unwrap()).unwrap();
        assert_eq!(story.reference.as_deref(), Some("Génesis 1-2"));
        assert_eq!(story.frames, parse_story(text).unwrap().frames);
    }
}

#[test]
fn a_story_reads_up_to_the_frames_a_frame_number_can_count() {
    let story = |frames: usize| {
        let mut text = String::from("# 1. Title\n\n");
        for f in 1..=frames {
            text.push_str(&format!("![a](b{f})\n\nframe {f}\n\n"));
        }
        // The writers end the file one line end after the last paragraph.
        text.pop();
        text
    };
    let most = story(usize::from(u16::MAX));
    let file = StoryFile::parse(most.as_str()).unwrap();
    assert_eq!(file.text(FrameNo::new(u16::MAX)), Some("frame 65535"));
    let parsed = file.story();
    assert_eq!(parsed.frames.len(), 65_535);
    assert_eq!(parsed.frames[65_534].text, "frame 65535");
    let records = decode_story(&most).unwrap().records;
    assert_eq!(records.render().unwrap(), most);
    let mut state = StoryState::default();
    state
        .frames
        .insert(FrameNo::new(u16::MAX), "last".to_owned());
    let applied = apply_story_state(&most, &state).unwrap();
    assert_eq!(
        StoryFile::parse(applied)
            .unwrap()
            .text(FrameNo::new(u16::MAX)),
        Some("last")
    );

    let over = story(65_536);
    let refused = StoryError::TooManyFrames {
        story: StoryNo::new(1).unwrap(),
        line: 3 + 4 * 65_535,
    };
    assert_eq!(StoryFile::parse(over.as_str()).err(), Some(refused.clone()));
    assert_eq!(decode_story(&over).err(), Some(refused.clone()));
    assert_eq!(
        apply_story_state(&over, &state),
        Err(WriteError::Story(refused))
    );
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
    let framed = write_frame(&seed, FrameNo::new(1), "a\u{2028}b").unwrap();
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

/// A paragraph line the reader keeps in its frame but the writers refuse: the reference
/// form, which only the last line of a file takes as the reference, and the title form,
/// which only the first line takes as the title.
fn refused_line() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "_x_",
        "__",
        "_ _",
        "_God made it all._",
        "# 2. looks like a title",
        "# 7.",
    ])
    .prop_map(str::to_owned)
}

fn paragraph_line() -> impl Strategy<Value = String> {
    prop_oneof![8 => text_line(), 1 => refused_line()]
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
            (image_line(), prop::collection::vec(paragraph_line(), 0..=3)),
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
                mut frames,
                reference,
                after_title,
                gaps,
                after_reference,
                final_newline,
            )| {
                let reference = if frames.is_empty() { None } else { reference };
                // Without a reference line, the file's last line in the reference form
                // is its reference.
                if reference.is_none()
                    && let Some((_, text)) = frames.last_mut()
                    && text
                        .last()
                        .is_some_and(|l| l.len() >= 2 && l.starts_with('_') && l.ends_with('_'))
                {
                    text.push("fin".to_owned());
                }
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
        for canonical in [true, false] {
            let text = g.text(canonical);
            let decoded = decode_story(&text).unwrap();
            let rendered = decoded.records.render().unwrap();
            // No text is lost or changed.
            prop_assert_eq!(parse_story(&rendered).unwrap(), g.story());
            // The bytes come back unless the decoder names what rendering reformats, which
            // a file in the writers' layout never has.
            prop_assert_eq!(decoded.reformatted.is_empty(), rendered == text);
            if canonical {
                prop_assert_eq!(&decoded.reformatted, &[]);
            }
            // Rendering then decoding gives back every record.
            let again = decode_story(&rendered).unwrap();
            prop_assert_eq!(&again.records, &decoded.records);
            prop_assert_eq!(again.reformatted, []);
        }
        // Where every paragraph is recorded, its spacing is no part of any record. (A
        // frameless story is all template lines, kept as written.)
        let writable = g.frames.iter().all(|(_, t)| frame_text_problem(&t.join("\n")).is_none());
        if writable && !g.frames.is_empty() {
            prop_assert_eq!(
                decode_story(&g.text(false)).unwrap().records,
                decode_story(&g.text(true)).unwrap().records
            );
        }
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
        let written = write_frame(&text, FrameNo::new(f as u16), &new).unwrap();
        let before = StoryFile::parse(text.clone()).unwrap();
        let after = StoryFile::parse(written.clone()).unwrap();
        prop_assert_eq!(after.text(FrameNo::new(f as u16)), Some(new.as_str()));
        prop_assert_eq!(after.title(), before.title());
        prop_assert_eq!(after.reference(), before.reference());
        for other in (1..=g.frames.len()).filter(|o| *o != f) {
            prop_assert_eq!(after.text(FrameNo::new(other as u16)), before.text(FrameNo::new(other as u16)));
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
        prop_assert_eq!(write_frame(&written, FrameNo::new(f as u16), &new).unwrap(), written);
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
    fn a_part_with_no_record_decodes_as_the_empty_text_its_base_holds(
        g in generated(),
        unwritten in prop::collection::vec(any::<bool>(), 41),
    ) {
        let records = decode_story(&g.text(false)).unwrap().records;
        // Decoded records decode to themselves.
        prop_assert_eq!(&records.as_decoded().unwrap(), &records);
        // Take away some of the records: those parts show the base's empty text.
        let mut store = records.clone();
        let mut empty = records.clone();
        if unwritten[40] && store.title.is_some() {
            store.title = None;
            empty.title = Some(String::new());
        }
        for (i, gone) in unwritten.iter().take(40).enumerate() {
            if *gone && store.frames.get(i).is_some_and(Option::is_some) {
                store.frames[i] = None;
                empty.frames[i] = Some(String::new());
            }
        }
        prop_assert_eq!(store.render().unwrap(), empty.render().unwrap());
        let shown = store.as_decoded().unwrap();
        prop_assert_eq!(&shown, &empty.as_decoded().unwrap());
        prop_assert_eq!(&shown.title, &empty.title);
        prop_assert_eq!(&shown.frames, &empty.frames);
        prop_assert_eq!(&shown.reference, &records.reference);
    }

    #[test]
    fn a_state_applied_onto_its_own_base_is_the_file(g in generated()) {
        let canonical = g.text(true);
        let records = decode_story(&canonical).unwrap().records;
        prop_assert_eq!(apply_story_state(&records.base, &records.state()).unwrap(), canonical);
        // An empty state leaves any file as it is.
        let text = g.text(false);
        prop_assert_eq!(apply_story_state(&text, &StoryState::default()).unwrap(), text);
    }
}

/// tC4's `applyStoryState`: one writer per part, each reading the whole file again.
fn apply_one_by_one(base: &str, state: &StoryState) -> Result<String, WriteError> {
    let mut out = base.to_owned();
    for (frame, text) in &state.frames {
        out = write_frame(&out, *frame, text)?;
    }
    if let Some(reference) = &state.reference {
        out = write_reference(&out, reference)?;
    }
    Ok(out)
}

fn part_text() -> impl Strategy<Value = String> {
    prop_oneof![
        6 => prop::collection::vec(text_line(), 0..=3).prop_map(|lines| lines.join("\n")),
        1 => prop::sample::select(vec!["a\n\nb", " ", "![x](y)", "_r_", "# 3.", "x\r", " T"]).prop_map(str::to_owned),
    ]
}

proptest! {
    #[test]
    fn a_state_applies_in_one_pass_as_tc4s_writers_apply_it(
        g in generated(),
        parts in prop::collection::btree_map(0u16..9, part_text(), 0..5),
        reference in prop::option::of(prop_oneof![4 => one_line(12), 1 => Just(" R".to_owned())]),
        broken in prop::bool::weighted(0.1),
    ) {
        let base = if broken { format!("{}\n\nloose text", g.text(false).replacen('#', "", 1)) } else { g.text(false) };
        // A title must be one trimmed line.
        let frames = parts
            .into_iter()
            .map(|(f, t)| (FrameNo::new(f), if f == 0 { t.replace('\n', " ").trim().to_owned() } else { t }))
            .collect();
        let state = StoryState { frames, reference };
        prop_assert_eq!(apply_story_state(&base, &state), apply_one_by_one(&base, &state));
    }
}
