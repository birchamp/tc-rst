//! The story file `content/NN.md`: tC4's one reader and its three writers
//! (`journal/story.mjs`, BURRITO-SPEC §10).
//!
//! A story file is one title line `# N. Title`, then per frame one image line and exactly
//! one paragraph, then an optional reference line `_…_`, separated by blank lines. The
//! image line splits frames (R-10.3.1). LF line ends only.
//!
//! [`StoryFile`] keeps the bytes it read and where each part lies in them, so writing it
//! gives back the same bytes. The writers are line splices, never re-serializations: a
//! frame write replaces the lines of that frame's paragraph region and nothing else
//! (R-10.3.4).

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ops::Range;

use tc_helps::obs::{ObsFrame, ObsImage, ObsStory};

use crate::error::{StoryError, TextProblem, WriteError};
use crate::ids::{FrameNo, StoryNo};
use crate::js::{is_blank, trim};

thread_local! {
    static LINES_TOUCHED: Cell<u64> = const { Cell::new(0) };
}

/// How many story lines this thread has read or written since it started: each line a
/// read splits off, and each line a writer, the decoder or a projection puts out. The
/// operation count the performance tests assert (docs/PERFORMANCE.md rule 2).
pub fn lines_touched() -> u64 {
    LINES_TOUCHED.with(Cell::get)
}

fn touch(lines: usize) {
    LINES_TOUCHED.with(|n| n.set(n.get() + lines as u64));
}

/// Joins the lines of a file a writer puts out.
fn emit(lines: &[&str]) -> String {
    touch(lines.len());
    lines.join("\n")
}

/// The number of the frame at `index` (from 0) of a story `split` read, which refuses a
/// story with more frames than a `FrameNo` can number.
#[expect(
    clippy::unreachable,
    reason = "split refuses a story with more frames than a FrameNo can number"
)]
fn frame_no(index: usize) -> FrameNo {
    u16::try_from(index + 1)
        .map(FrameNo::new)
        .unwrap_or_else(|_| unreachable!("split reads at most {} frames", u16::MAX))
}

/// The title line `# N. title`, or `# N.` for an empty title (`writeTitle`,
/// `journal/story.mjs:151-158`).
fn title_line_of(number: StoryNo, title: &str) -> String {
    if title.is_empty() {
        format!("# {number}.")
    } else {
        format!("# {number}. {title}")
    }
}

/// `TITLE_RE` (`journal/story.mjs:15`): `# `, ASCII digits, `.`, then nothing or a space and
/// the title. The title is everything after that space, so it is empty for `# 1. `.
/// JavaScript's `.` refuses U+2028 and U+2029, so a title holding one is not a title.
fn title_line(line: &str) -> Option<(&str, Option<Range<usize>>)> {
    let rest = line.strip_prefix("# ")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let after = rest[digits..].strip_prefix('.')?;
    let title = if after.is_empty() {
        None
    } else {
        let text = after.strip_prefix(' ')?;
        if text.contains(['\u{2028}', '\u{2029}']) {
            return None;
        }
        let start = line.len() - text.len();
        Some(start..line.len())
    };
    Some((&rest[..digits], title))
}

/// `IMAGE_RE` (`journal/story.mjs:16`): `![alt](url)`, the alt without `]`, the url without
/// `)`.
fn is_image_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("![") else {
        return false;
    };
    let Some(close) = rest.find(']') else {
        return false;
    };
    let Some(url) = rest[close + 1..]
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
    else {
        return false;
    };
    !url.contains(')')
}

/// `REF_RE` (`journal/story.mjs:20`): a line that starts and ends with `_`, at least two
/// characters long.
fn is_reference_form(line: &str) -> bool {
    line.len() >= 2 && line.starts_with('_') && line.ends_with('_')
}

/// One frame's lines: its image line and the first and last line of its paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameLines {
    image: usize,
    text: Option<(usize, usize)>,
}

/// A story file split into lines (`splitStory`, `journal/story.mjs:75-113`).
struct Split<'a> {
    number: StoryNo,
    title: Option<Range<usize>>,
    lines: Vec<&'a str>,
    frames: Vec<FrameLines>,
    reference: Option<usize>,
}

fn split(bytes: &str) -> Result<Split<'_>, StoryError> {
    if bytes.contains('\r') {
        return Err(StoryError::CarriageReturn);
    }
    let lines: Vec<&str> = bytes.split('\n').collect();
    touch(lines.len());
    let first = lines[0];
    let (digits, title) = title_line(first).ok_or_else(|| StoryError::NotTitle {
        start: String::from_utf16_lossy(&first.encode_utf16().take(40).collect::<Vec<u16>>()),
    })?;
    let number = story_number(digits).ok_or_else(|| StoryError::StoryNumber {
        digits: digits.to_owned(),
    })?;

    let mut last = lines.len() - 1;
    while last > 0 && is_blank(lines[last]) {
        last -= 1;
    }
    let mut reference = None;
    if last > 0 && is_reference_form(lines[last]) && !is_image_line(lines[last]) {
        let line = lines[last];
        if is_blank(&line[1..line.len() - 1]) {
            return Err(StoryError::BlankReference {
                story: number,
                line: line.to_owned(),
            });
        }
        reference = Some(last);
    }
    let end = reference.unwrap_or(lines.len());
    let mut frames: Vec<FrameLines> = Vec::new();
    for (i, line) in lines.iter().enumerate().take(end).skip(1) {
        if is_image_line(line) {
            if frames.len() == usize::from(u16::MAX) {
                return Err(StoryError::TooManyFrames {
                    story: number,
                    line: i + 1,
                });
            }
            frames.push(FrameLines {
                image: i,
                text: None,
            });
            continue;
        }
        if is_blank(line) {
            continue;
        }
        let Some(frame) = frames.last_mut() else {
            return Err(StoryError::TextBeforeFirstImage { line: i + 1 });
        };
        frame.text = match frame.text {
            None => Some((i, i)),
            Some((first, last)) if last + 1 == i => Some((first, i)),
            Some(_) => {
                return Err(StoryError::SecondParagraph {
                    story: number,
                    frame: frames.len(),
                    line: i + 1,
                });
            }
        };
    }
    if frames.is_empty() && reference.is_some() {
        return Err(StoryError::ReferenceWithoutFrame { story: number });
    }
    Ok(Split {
        number,
        title,
        lines,
        frames,
        reference,
    })
}

/// `Number(m[1])` checked by `storyNumberError`: leading zeros are allowed.
fn story_number(digits: &str) -> Option<StoryNo> {
    let significant = digits.trim_start_matches('0');
    if significant.len() > 2 {
        return None;
    }
    StoryNo::new(significant.parse().unwrap_or(0))
}

impl Split<'_> {
    fn frame_text(&self, frame: &FrameLines) -> String {
        frame
            .text
            .map(|(first, last)| self.lines[first..=last].join("\n"))
            .unwrap_or_default()
    }

    fn reference_text(&self) -> Option<&str> {
        self.reference.map(|i| {
            let line = self.lines[i];
            &line[1..line.len() - 1]
        })
    }

    fn title_text(&self) -> &str {
        self.title.clone().map_or("", |range| &self.lines[0][range])
    }

    /// The lines of frame `f` (1-based): after its image line, up to the next image line,
    /// the reference line or the end of the file (`frameRegion`, `journal/story.mjs:130-134`).
    fn frame_region(&self, f: usize) -> Range<usize> {
        let start = self.frames[f - 1].image + 1;
        let end = match self.frames.get(f) {
            Some(next) => next.image,
            None => self.reference.unwrap_or(self.lines.len()),
        };
        start..end
    }
}

fn image_of(line: &str) -> ObsImage {
    let inner = &line[2..line.len() - 1];
    // An image line holds `](` after its alt text: `is_image_line` found it.
    let (alt, url) = inner.split_once("](").unwrap_or((inner, ""));
    ObsImage {
        alt: alt.to_owned(),
        url: url.to_owned(),
    }
}

/// One story file as read: its exact bytes and where each part lies in them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryFile {
    text: String,
    number: StoryNo,
    title: Option<Range<usize>>,
    /// The byte range of each line, without its `\n`.
    lines: Vec<Range<usize>>,
    frames: Vec<FrameLines>,
    reference: Option<usize>,
}

impl StoryFile {
    /// Reads a story file (tC4 `parseStory`, `journal/story.mjs:114-121`).
    pub fn parse(text: impl Into<String>) -> Result<StoryFile, StoryError> {
        let text = text.into();
        let (number, title, frames, reference, lines) = {
            let s = split(&text)?;
            let mut at = 0;
            let lines = s
                .lines
                .iter()
                .map(|line| {
                    let range = at..at + line.len();
                    at = range.end + 1;
                    range
                })
                .collect();
            (s.number, s.title, s.frames, s.reference, lines)
        };
        Ok(StoryFile {
            text,
            number,
            title,
            lines,
            frames,
            reference,
        })
    }

    /// The bytes read, unchanged.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn into_string(self) -> String {
        self.text
    }

    pub fn number(&self) -> StoryNo {
        self.number
    }

    /// The title without `# N. `; empty in a seed file (`# 2.`).
    pub fn title(&self) -> &str {
        // The title line is the first line, which starts the text.
        self.title.clone().map_or("", |range| &self.text[range])
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The image line of frame `frame` (1-based), as written.
    pub fn image_line(&self, frame: usize) -> Option<&str> {
        let f = self.frames.get(frame.checked_sub(1)?)?;
        Some(&self.text[self.lines[f.image].clone()])
    }

    /// The paragraph of frame `frame` (1-based), its lines joined by `\n`, or the title for
    /// frame 0. `None` for a frame the story does not have.
    pub fn text(&self, frame: FrameNo) -> Option<&str> {
        if frame.is_title() {
            return Some(self.title());
        }
        let f = self.frames.get(usize::from(frame.get()) - 1)?;
        Some(match f.text {
            // The lines of one paragraph are adjacent, so the paragraph is one slice.
            Some((first, last)) => &self.text[self.lines[first].start..self.lines[last].end],
            None => "",
        })
    }

    /// The reference line's text without the underscores.
    pub fn reference(&self) -> Option<&str> {
        self.reference.map(|i| {
            let line = &self.text[self.lines[i].clone()];
            &line[1..line.len() - 1]
        })
    }

    /// The story as tC4's `parseStory` returns it.
    pub fn story(&self) -> ObsStory {
        ObsStory {
            number: self.number.get(),
            title: self.title().to_owned(),
            frames: (0..self.frames.len())
                .map(|i| ObsFrame {
                    image: image_of(self.image_line(i + 1).unwrap_or_default()),
                    text: self.text(frame_no(i)).unwrap_or_default().to_owned(),
                })
                .collect(),
            reference: self.reference().map(str::to_owned),
        }
    }
}

/// tC4 `parseStory` (`journal/story.mjs:114-121`): the title, the frames in file order and
/// the reference line's text.
pub fn parse_story(bytes: &str) -> Result<ObsStory, StoryError> {
    StoryFile::parse(bytes).map(|file| file.story())
}

/// Why `text` cannot be a frame's paragraph (tC4 `frameTextError`, `journal/story.mjs:47-57`):
/// a frame is exactly one paragraph of non-blank lines, and no line of it may read back as
/// an image line, a reference line or a title line.
pub fn frame_text_problem(text: &str) -> Option<TextProblem> {
    if text.contains('\r') {
        return Some(TextProblem::CarriageReturn);
    }
    if !text.is_empty() && text.split('\n').any(is_blank) {
        return Some(TextProblem::BlankLine);
    }
    for line in text.split('\n') {
        if is_image_line(line) {
            return Some(TextProblem::ImageLine);
        }
        if is_reference_form(line) {
            return Some(TextProblem::ReferenceLine);
        }
        if title_line(line).is_some() {
            return Some(TextProblem::TitleLine);
        }
    }
    None
}

/// Why `text` cannot be a title (tC4 `titleTextError`, `journal/story.mjs:60-64`).
///
/// tC4 accepts a title holding U+2028 or U+2029 and writes it, and its reader then refuses
/// the file, since a title line is `.*` and JavaScript's `.` stops at those characters:
/// the whole story could no longer be read. Here such a title is not one line, so the
/// write is refused and the story stays readable.
pub fn title_text_problem(text: &str) -> Option<TextProblem> {
    if text.contains(['\r', '\n', '\u{2028}', '\u{2029}']) {
        Some(TextProblem::NotOneLine)
    } else if text != trim(text) {
        Some(TextProblem::OuterWhitespace)
    } else {
        None
    }
}

/// Why `text` cannot be a reference line's text (tC4 `refTextError`,
/// `journal/story.mjs:65-70`). No write removes the reference line, so it is never empty.
pub fn reference_text_problem(text: &str) -> Option<TextProblem> {
    if text.is_empty() {
        Some(TextProblem::Empty)
    } else if text.contains(['\r', '\n']) {
        Some(TextProblem::NotOneLine)
    } else if text != trim(text) {
        Some(TextProblem::OuterWhitespace)
    } else {
        None
    }
}

/// The template's exact form of an empty frame between frames, and at the end of the file
/// (`journal/story.mjs:128-129`).
const EMPTY_FRAME_MID: [&str; 3] = ["", "", ""];
const EMPTY_FRAME_LAST: [&str; 6] = ["", "", "", "", "", ""];

fn splice(lines: &[&str], range: Range<usize>, replacement: &[&str]) -> String {
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + replacement.len());
    out.extend_from_slice(&lines[..range.start]);
    out.extend_from_slice(replacement);
    out.extend_from_slice(&lines[range.end..]);
    emit(&out)
}

/// Sets frame `frame`'s paragraph, or the title for frame 0 (tC4 `writeFrame`,
/// `journal/story.mjs:135-150`). Only that frame's region changes; an empty text writes
/// the template's empty form.
pub fn write_frame(bytes: &str, frame: FrameNo, text: &str) -> Result<String, WriteError> {
    if frame.is_title() {
        return write_title(bytes, text);
    }
    if let Some(problem) = frame_text_problem(text) {
        return Err(WriteError::FrameText(problem));
    }
    let s = split(bytes)?;
    let f = usize::from(frame.get());
    if f > s.frames.len() {
        return Err(WriteError::NoSuchFrame {
            story: s.number,
            frames: s.frames.len(),
            frame,
        });
    }
    let region = s.frame_region(f);
    let followed = region.end < s.lines.len();
    let lines: Vec<&str>;
    let replacement: &[&str] = if text.is_empty() {
        if followed {
            &EMPTY_FRAME_MID
        } else {
            &EMPTY_FRAME_LAST
        }
    } else {
        lines = std::iter::once("")
            .chain(text.split('\n'))
            .chain(std::iter::once(""))
            .collect();
        &lines
    };
    Ok(splice(&s.lines, region, replacement))
}

/// Sets the title: `# N. text`, or `# N.` for an empty text (tC4 `writeTitle`,
/// `journal/story.mjs:151-158`).
pub fn write_title(bytes: &str, text: &str) -> Result<String, WriteError> {
    if let Some(problem) = title_text_problem(text) {
        return Err(WriteError::TitleText(problem));
    }
    let s = split(bytes)?;
    Ok(splice(&s.lines, 0..1, &[&title_line_of(s.number, text)]))
}

/// Sets the reference line's text (tC4 `writeRef`, `journal/story.mjs:159-174`). A story
/// with no reference line gets one after its last frame; after an empty last frame the
/// template's spacing comes first, as `write_frame` leaves it when a reference follows.
pub fn write_reference(bytes: &str, text: &str) -> Result<String, WriteError> {
    if let Some(problem) = reference_text_problem(text) {
        return Err(WriteError::ReferenceText(problem));
    }
    let s = split(bytes)?;
    let line = format!("_{text}_");
    if let Some(i) = s.reference {
        return Ok(splice(&s.lines, i..i + 1, &[&line]));
    }
    let Some(last) = s.frames.last() else {
        return Err(WriteError::NoFrameForReference { story: s.number });
    };
    let end = s.lines.len();
    Ok(match last.text {
        None => splice(&s.lines, last.image + 1..end, &["", "", "", &line, ""]),
        Some((_, last_text)) => splice(&s.lines, last_text + 1..end, &["", &line, ""]),
    })
}

/// The seed form of a template story: the title reduced to `# N.`, everything else as the
/// template has it (tC4 `seedStory`, `journal/story.mjs:179`; R-10.2.4).
pub fn seed_story(template: &str) -> Result<String, WriteError> {
    write_title(template, "")
}

/// A story's folded state: the text of each frame that has one (frame 0 is the title) and
/// the reference line's text (tC4's fold output `stories[n]`, BURRITO-SPEC §10.7).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoryState {
    pub frames: BTreeMap<FrameNo, String>,
    pub reference: Option<String>,
}

/// Applies a folded state onto a base file: each frame in ascending order, then the
/// reference line (tC4 `applyStoryState`, `journal/story.mjs:184-190`). A part with no
/// value keeps the base's text.
///
/// tC4 runs one writer per part, and each writer reads the whole file again, so a story's
/// cost grows with its frames times its lines. This reads the base once and gives the same
/// bytes and the same refusal: the writers only change their own regions, and a written
/// story always reads again with the same frames.
pub fn apply_story_state(base: &str, state: &StoryState) -> Result<String, WriteError> {
    let parts: Vec<(Option<FrameNo>, &str)> = state
        .frames
        .iter()
        .map(|(frame, text)| (Some(*frame), text.as_str()))
        .chain(state.reference.as_deref().map(|r| (None, r)))
        .collect();
    let Some(&(frame, text)) = parts.first() else {
        return Ok(base.to_owned());
    };
    // Each writer checks its text before it reads the file.
    if let Some(error) = text_error(frame, text) {
        return Err(error);
    }
    let s = split(base)?;
    for &(frame, text) in &parts {
        if let Some(error) = text_error(frame, text) {
            return Err(error);
        }
        match frame {
            Some(f) if usize::from(f.get()) > s.frames.len() => {
                return Err(WriteError::NoSuchFrame {
                    story: s.number,
                    frames: s.frames.len(),
                    frame: f,
                });
            }
            None if s.reference.is_none() && s.frames.is_empty() => {
                return Err(WriteError::NoFrameForReference { story: s.number });
            }
            _ => {}
        }
    }

    let title = state
        .frames
        .get(&FrameNo::TITLE)
        .map(|t| title_line_of(s.number, t));
    let reference = state.reference.as_ref().map(|r| format!("_{r}_"));
    let lines = &s.lines;
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + 8);
    out.push(title.as_deref().unwrap_or(lines[0]));
    let first_image = s.frames.first().map_or(lines.len(), |f| f.image);
    out.extend_from_slice(&lines[1..first_image]);
    let count = s.frames.len();
    for (i, frame) in s.frames.iter().enumerate() {
        let f = i + 1;
        out.push(lines[frame.image]);
        let written = state.frames.get(&frame_no(i));
        let last = f == count;
        if let (true, None, Some(reference)) = (last, s.reference, reference.as_deref()) {
            // `write_reference` on a story with no reference line: after the last frame's
            // paragraph, or after the template's spacing when the frame is empty.
            let text_end = match written {
                Some(t) if !t.is_empty() => {
                    out.push("");
                    out.extend(t.split('\n'));
                    true
                }
                Some(_) => false,
                None => match frame.text {
                    Some((_, last_text)) => {
                        out.extend_from_slice(&lines[frame.image + 1..=last_text]);
                        true
                    }
                    None => false,
                },
            };
            if text_end {
                out.extend(["", reference, ""]);
            } else {
                out.extend(["", "", "", reference, ""]);
            }
            continue;
        }
        match written {
            None => out.extend_from_slice(&lines[s.frame_region(f)]),
            Some(t) if t.is_empty() => {
                let followed = !last || s.reference.is_some();
                out.extend_from_slice(if followed {
                    &EMPTY_FRAME_MID
                } else {
                    &EMPTY_FRAME_LAST
                });
            }
            Some(t) => {
                out.push("");
                out.extend(t.split('\n'));
                out.push("");
            }
        }
    }
    if let Some(at) = s.reference {
        out.push(reference.as_deref().unwrap_or(lines[at]));
        out.extend_from_slice(&lines[at + 1..]);
    }
    Ok(emit(&out))
}

/// The refusal of a part's text: the title's for frame 0, a paragraph's for the other
/// frames, the reference line's for `None`.
fn text_error(frame: Option<FrameNo>, text: &str) -> Option<WriteError> {
    match frame {
        Some(f) if f.is_title() => title_text_problem(text).map(WriteError::TitleText),
        Some(_) => frame_text_problem(text).map(WriteError::FrameText),
        None => reference_text_problem(text).map(WriteError::ReferenceText),
    }
}

/// The records a story file stands for, in STORAGE §2.2's terms: the base (`StoryBase`),
/// the title (frame 0) and each frame's paragraph (`Frame`), and the reference line's
/// text (`StoryRef`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryRecords {
    pub story: StoryNo,
    /// The template lines: the file with each recorded part reduced to the seed form —
    /// the title to `# N.`, each recorded frame's region to the template's empty form,
    /// and a recorded reference line removed with what follows it when the last frame's
    /// paragraph is recorded too. Otherwise the reference line stays where it is, and the
    /// `StoryRef` record replaces it there.
    pub base: String,
    /// `None` when the writers would refuse the title as it stands; it then stays in the
    /// base, as written.
    pub title: Option<String>,
    /// Frames 1 and up, in order; `None` for a paragraph the writers would refuse, which
    /// stays in the base.
    pub frames: Vec<Option<String>>,
    /// `None` when the file has no reference line, or one the writers would refuse, which
    /// then stays in the base.
    pub reference: Option<String>,
}

impl StoryRecords {
    /// The state that, applied onto the base, gives the file again. A frame past
    /// 65,535 has no number and is left out; no base can hold it, so rendering such
    /// records is refused.
    pub fn state(&self) -> StoryState {
        let mut frames = BTreeMap::new();
        if let Some(title) = &self.title {
            frames.insert(FrameNo::TITLE, title.clone());
        }
        for (number, text) in (1..=u16::MAX).zip(&self.frames) {
            if let Some(text) = text {
                frames.insert(FrameNo::new(number), text.clone());
            }
        }
        StoryState {
            frames,
            reference: self.reference.clone(),
        }
    }

    /// The file these records stand for (STORAGE §7.7's `content/NN.md` projection).
    pub fn render(&self) -> Result<String, WriteError> {
        apply_story_state(&self.base, &self.state())
    }

    /// These records as decoding their file gives them back. A decoded file is compared
    /// with these, not with the records as a store holds them, to find the parts someone
    /// changed (STORAGE §11.1 X3's `written(k)`).
    ///
    /// A part with no record shows the text its base holds there. A record of that same
    /// text renders the same bytes, so no decoder can tell the two apart, and
    /// [`decode_story`] records every part the writers accept, the empty ones too. A new
    /// project's store holds only each story's `StoryBase` (tC4 journals nothing at
    /// creation, `src/data/journal/journalingStore.ts:1204-1212`), and its untouched file
    /// decodes to an empty title record and an empty record for each frame. Against the
    /// store's own records, each of those would be an outside change of a part nobody
    /// wrote. Against these, an untouched file changes nothing and an outside edit
    /// changes only the parts it edits. A cleared frame and a frame never written both
    /// decode as the empty text.
    pub fn as_decoded(&self) -> Result<StoryRecords, WriteError> {
        Ok(decode_story(&self.render()?)?.records)
    }
}

/// A part of a story file that rendering its records writes in the writers' form where
/// the file has another. Only the layout differs: blank lines, or how the title line
/// writes its number. Every text stays as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reformatted {
    /// The title line: its number written with leading zeros (`# 01.`), or a space after
    /// an empty title (`# 1. `).
    Title,
    /// The blank lines around this frame's paragraph, or of this empty frame: lines that
    /// hold only spaces, or more or fewer blank lines than the writers write.
    Frame(FrameNo),
    /// The lines after the reference line: blank lines, or no line end after it.
    Reference,
}

/// A story file as the records it stands for, and the parts of it whose layout no record
/// keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedStory {
    pub records: StoryRecords,
    /// Empty exactly when rendering the records gives back the file's bytes, which it does
    /// for every file in the writers' layout: every story tC4 writes and every shipped
    /// story. Otherwise rendering rewrites the layout of these parts, in file order. No
    /// record holds that layout, so the caller says so (STORAGE §11.1 X3's whitespace-only
    /// notice) rather than rewriting it unnoticed.
    pub reformatted: Vec<Reformatted>,
}

/// Splits a story file into the records it stands for (STORAGE §11.1: an OBS story decodes
/// to `StoryBase`, `Frame` and `StoryRef`) in one pass over its lines.
///
/// Every part the writers accept is recorded, so its layout is the writers' once the
/// records are rendered. An empty part is recorded too: a file holds no sign of whether
/// an empty frame was cleared or never written, so compare the result with
/// [`StoryRecords::as_decoded`] of a store's records, not with those records.
///
/// tC4 keeps a frame's bytes until its journal holds that frame; its base is the committed
/// file. A decoder that left a respaced frame out of the records would leave its text in
/// the base, under a `Frame` record the store already holds, so the layout goes and
/// [`DecodedStory::reformatted`] names where.
pub fn decode_story(bytes: &str) -> Result<DecodedStory, StoryError> {
    let s = split(bytes)?;
    let lines = &s.lines;
    let mut reformatted = Vec::new();

    let title_text = s.title_text();
    let title = title_text_problem(title_text)
        .is_none()
        .then(|| title_text.to_owned());
    let seed_title = title_line_of(s.number, "");
    let first = match &title {
        Some(text) => {
            if lines[0] != title_line_of(s.number, text) {
                reformatted.push(Reformatted::Title);
            }
            seed_title.as_str()
        }
        None => lines[0],
    };
    let frames: Vec<Option<String>> = s
        .frames
        .iter()
        .map(|f| {
            let text = s.frame_text(f);
            frame_text_problem(&text).is_none().then_some(text)
        })
        .collect();
    let reference = s
        .reference_text()
        .filter(|r| reference_text_problem(r).is_none())
        .map(str::to_owned);
    // A paragraph the writers refuse stays in the base, and when it is the last frame's,
    // its last line may be in the reference form: with the reference line gone, that line
    // would read as the reference and the record would overwrite it.
    let cut = s
        .reference
        .filter(|_| reference.is_some() && matches!(frames.last(), Some(Some(_))));
    let base_end = cut.unwrap_or(lines.len());

    let mut base: Vec<&str> = Vec::with_capacity(base_end);
    base.push(first);
    let first_image = s.frames.first().map_or(lines.len(), |f| f.image);
    base.extend_from_slice(&lines[1..first_image]);
    for (i, (frame, text)) in s.frames.iter().zip(&frames).enumerate() {
        base.push(lines[frame.image]);
        let region = s.frame_region(i + 1);
        if text.is_none() {
            base.extend_from_slice(&lines[region]);
            continue;
        }
        base.extend_from_slice(if region.end < base_end {
            &EMPTY_FRAME_MID
        } else {
            &EMPTY_FRAME_LAST
        });
        // What rendering writes here: the paragraph between two empty lines, or an empty
        // frame's form, which is the shorter one whenever an image or reference line
        // follows.
        let writers = match frame.text {
            Some((first, last)) => {
                first == region.start + 1
                    && last + 2 == region.end
                    && lines[region.start].is_empty()
                    && lines[last + 1].is_empty()
            }
            None => {
                let empty = if region.end < lines.len() {
                    EMPTY_FRAME_MID.len()
                } else {
                    EMPTY_FRAME_LAST.len()
                };
                region.len() == empty && lines[region].iter().all(|l| l.is_empty())
            }
        };
        if !writers {
            reformatted.push(Reformatted::Frame(frame_no(i)));
        }
    }
    if !s.frames.is_empty() {
        let tail = s.frame_region(s.frames.len()).end;
        match cut {
            None => base.extend_from_slice(&lines[tail..]),
            Some(at) => {
                if lines[at + 1..] != [""] {
                    reformatted.push(Reformatted::Reference);
                }
            }
        }
    }
    Ok(DecodedStory {
        records: StoryRecords {
            story: s.number,
            base: emit(&base),
            title,
            frames,
            reference,
        },
        reformatted,
    })
}
