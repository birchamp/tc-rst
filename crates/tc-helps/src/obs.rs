//! Open Bible Stories story files (`content/NN.md`): a title line `# N. Title`, then per
//! frame one image line and one paragraph, then an optional reference line `_…_`.
//! Every image line starts a frame.

use crate::Error;
use crate::error::StoryProblem;

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
    if text.contains('\r') {
        return Err(fail(1, StoryProblem::CarriageReturn));
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let (number, title) = title_line(lines.first().copied().unwrap_or_default())
        .ok_or_else(|| fail(1, StoryProblem::Title))?;
    let number = u8::try_from(number)
        .ok()
        .filter(|n| (1..=50).contains(n))
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

/// Writes a story in the published layout: blank lines between title, frames and
/// reference, and a final newline.
pub fn write_obs_story(story: &ObsStory) -> String {
    let mut out = format!("# {}.", story.number);
    if !story.title.is_empty() {
        out.push(' ');
        out.push_str(&story.title);
    }
    out.push_str("\n\n");
    for frame in &story.frames {
        out.push_str(&frame.image.to_string());
        out.push_str("\n\n");
        if !frame.text.is_empty() {
            out.push_str(&frame.text);
            out.push_str("\n\n");
        }
    }
    if let Some(reference) = &story.reference {
        out.push('_');
        out.push_str(reference);
        out.push_str("_\n");
    }
    out
}
