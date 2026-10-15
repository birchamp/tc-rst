//! Open Bible Stories story files (`content/NN.md`): a title line `# N. Title`, then per
//! frame one image line and one paragraph, then an optional reference line `_…_`.
//! Every image line starts a frame.
//!
//! The reader skips a leading byte order mark and reads CRLF line ends as LF. The writer
//! writes tC4's layout (`journal/story.mjs`), which the drafted and seed stories in the
//! fixtures use byte for byte: one blank line after the title; after an image line, one
//! blank line, the text, and one blank line, or three blank lines for an empty frame (six
//! at the end of a story with no reference); the reference line, then a final LF. A file
//! in another layout is rewritten in this one, and LF only.

use std::borrow::Cow;

use crate::Error;
use crate::articles::without_bom;
use crate::error::{StoryField, StoryProblem, StoryWriteProblem};
use crate::refs::ObsRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsStory {
    /// 1 to 50.
    pub number: u8,
    /// The title text after `# N. `; empty in a seed file (`# 2.`).
    pub title: String,
    pub frames: Vec<ObsFrame>,
    /// The text between the underscores of the reference line, such as
    /// `A Bible story from: Genesis 1-2`.
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsFrame {
    pub image: ObsImage,
    /// One paragraph; its lines joined with `\n`. Empty in a seed file.
    pub text: String,
}

/// The image line `![alt](url)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsImage {
    pub alt: String,
    pub url: String,
}

impl ObsImage {
    fn parse(line: &str) -> Option<Self> {
        let inner = line.strip_prefix("![")?.strip_suffix(')')?;
        let (alt, url) = inner.split_once("](")?;
        if alt.contains(']') || url.contains(')') {
            return None;
        }
        Some(Self {
            alt: alt.to_owned(),
            url: url.to_owned(),
        })
    }

    /// The story and frame the picture shows, read from a file name such as
    /// `obs-en-01-02.jpg`. Packs resolve images by this name.
    pub fn picture(&self) -> Option<(u8, u16)> {
        let file = self.url.rsplit('/').next()?;
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let mut parts = stem.rsplit('-');
        let frame = parts.next()?.parse().ok()?;
        let story = parts.next()?.parse().ok()?;
        Some((story, frame))
    }
}

impl std::fmt::Display for ObsImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "![{}]({})", self.alt, self.url)
    }
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn reference_form(line: &str) -> bool {
    line.len() >= 2 && line.starts_with('_') && line.ends_with('_')
}

fn title_line(line: &str) -> Option<(u32, String)> {
    let rest = line.strip_prefix("# ")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let number = rest[..digits].parse().ok()?;
    let after = rest[digits..].strip_prefix('.')?;
    let title = if after.is_empty() {
        String::new()
    } else {
        after.strip_prefix(' ')?.to_owned()
    };
    Some((number, title))
}

fn fail(line: usize, problem: StoryProblem) -> Error {
    Error::Story { line, problem }
}

/// Parses a story file. Line numbers in errors count from 1.
pub fn parse_obs_story(text: &str) -> Result<ObsStory, Error> {
    let text = without_bom(text);
    let text = if text.contains("\r\n") {
        Cow::Owned(text.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(text)
    };
    if let Some(at) = text.find('\r') {
        let line = 1 + text[..at].matches('\n').count();
        return Err(fail(line, StoryProblem::CarriageReturn));
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let (number, title) = title_line(lines.first().copied().unwrap_or_default())
        .ok_or_else(|| fail(1, StoryProblem::Title))?;
    let number = u8::try_from(number)
        .ok()
        .filter(|n| (1..=ObsRef::STORIES).contains(n))
        .ok_or_else(|| fail(1, StoryProblem::StoryNumber(number)))?;

    let mut last = lines.len() - 1;
    while last > 0 && is_blank(lines[last]) {
        last -= 1;
    }
    let mut reference = None;
    let mut end = lines.len();
    if last > 0 && reference_form(lines[last]) && ObsImage::parse(lines[last]).is_none() {
        let inner = &lines[last][1..lines[last].len() - 1];
        if is_blank(inner) {
            return Err(fail(last + 1, StoryProblem::EmptyReference));
        }
        reference = Some(inner.to_owned());
        end = last;
    }

    let mut frames: Vec<ObsFrame> = Vec::new();
    let mut last_text: Option<usize> = None;
    for (i, line) in lines.iter().enumerate().take(end).skip(1) {
        if let Some(image) = ObsImage::parse(line) {
            frames.push(ObsFrame {
                image,
                text: String::new(),
            });
            last_text = None;
            continue;
        }
        if is_blank(line) {
            continue;
        }
        let Some(frame) = frames.last_mut() else {
            return Err(fail(i + 1, StoryProblem::TextBeforeFirstFrame));
        };
        match last_text {
            Some(prev) if prev + 1 != i => {
                return Err(fail(
                    i + 1,
                    StoryProblem::SecondParagraph {
                        frame: frames.len(),
                    },
                ));
            }
            Some(_) => frame.text.push('\n'),
            None => {}
        }
        frame.text.push_str(line);
        last_text = Some(i);
    }
    if frames.is_empty() && reference.is_some() {
        return Err(fail(end + 1, StoryProblem::ReferenceWithoutFrames));
    }
    Ok(ObsStory {
        number,
        title,
        frames,
        reference,
    })
}

/// Writes a story in tC4's layout (see the module documentation). A story the format
/// cannot carry, so that it would read back changed, is refused.
pub fn write_obs_story(story: &ObsStory) -> Result<String, Error> {
    writable(story).map_err(Error::StoryWrite)?;
    let mut lines: Vec<Cow<str>> = Vec::new();
    lines.push(if story.title.is_empty() {
        Cow::Owned(format!("# {}.", story.number))
    } else {
        Cow::Owned(format!("# {}. {}", story.number, story.title))
    });
    lines.push(Cow::Borrowed(""));
    for (i, frame) in story.frames.iter().enumerate() {
        lines.push(Cow::Owned(frame.image.to_string()));
        if frame.text.is_empty() {
            let at_end = i + 1 == story.frames.len() && story.reference.is_none();
            let blanks = if at_end { 6 } else { 3 };
            lines.extend(std::iter::repeat_n(Cow::Borrowed(""), blanks));
        } else {
            lines.push(Cow::Borrowed(""));
            lines.extend(frame.text.split('\n').map(Cow::Borrowed));
            lines.push(Cow::Borrowed(""));
        }
    }
    if let Some(reference) = &story.reference {
        lines.push(Cow::Owned(format!("_{reference}_")));
        lines.push(Cow::Borrowed(""));
    }
    Ok(lines.join("\n"))
}

fn one_line(text: &str, field: StoryField, frame: Option<usize>) -> Result<(), StoryWriteProblem> {
    if text.contains(['\n', '\r']) {
        Err(StoryWriteProblem::LineBreak { field, frame })
    } else {
        Ok(())
    }
}

/// Refuses what [`parse_obs_story`] would read back differently.
fn writable(story: &ObsStory) -> Result<(), StoryWriteProblem> {
    if !(1..=ObsRef::STORIES).contains(&story.number) {
        return Err(StoryWriteProblem::StoryNumber(story.number));
    }
    one_line(&story.title, StoryField::Title, None)?;
    for (i, f) in story.frames.iter().enumerate() {
        let frame = i + 1;
        one_line(&f.image.alt, StoryField::ImageAlt, Some(frame))?;
        one_line(&f.image.url, StoryField::ImageUrl, Some(frame))?;
        if f.image.alt.contains(']') || f.image.url.contains(')') {
            return Err(StoryWriteProblem::ImageForm { frame });
        }
        if f.text.is_empty() {
            continue;
        }
        if f.text.contains('\r') {
            return Err(StoryWriteProblem::CarriageReturn { frame });
        }
        for line in f.text.split('\n') {
            if is_blank(line) {
                return Err(StoryWriteProblem::BlankLine { frame });
            }
            if ObsImage::parse(line).is_some() {
                return Err(StoryWriteProblem::ImageLine { frame });
            }
        }
        let last_line = f.text.rsplit('\n').next().unwrap_or_default();
        if frame == story.frames.len() && story.reference.is_none() && reference_form(last_line) {
            return Err(StoryWriteProblem::ReadsAsReference { frame });
        }
    }
    if let Some(reference) = &story.reference {
        one_line(reference, StoryField::Reference, None)?;
        if is_blank(reference) {
            return Err(StoryWriteProblem::EmptyReference);
        }
        if story.frames.is_empty() {
            return Err(StoryWriteProblem::ReferenceWithoutFrames);
        }
    }
    Ok(())
}
