//! What tc-obs refuses, and why. The messages follow tC4's.

use std::fmt;

use crate::ids::{FrameNo, StoryNo};

/// Any refusal of this crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Story(#[from] StoryError),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    NewProject(#[from] NewProjectError),
}

/// A file that is not a story file (`splitStory`, `journal/story.mjs:75-113`). Lines
/// count from 1.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoryError {
    #[error("story contains a carriage return (§10: LF line ends)")]
    CarriageReturn,
    /// `start` is the line's first 40 UTF-16 code units, as tC4 quotes it.
    #[error("story line 1 \"{start}\" is not the title form `# N.` or `# N. Title` (§10)")]
    NotTitle { start: String },
    #[error(
        "story title line names story {digits}, which {} is not a §10 story number (1..50) (§10)",
        number_text(digits)
    )]
    StoryNumber { digits: String },
    #[error("story {story} reference line \"{line}\" has no text between the underscores (§10)")]
    BlankReference { story: StoryNo, line: String },
    #[error(
        "story line {line} carries text before the first image line (§10: the title line is followed by frames)"
    )]
    TextBeforeFirstImage { line: usize },
    #[error(
        "story {story} frame {frame} carries more than one paragraph (line {line}) — exactly one paragraph per frame (§10)"
    )]
    SecondParagraph {
        story: StoryNo,
        frame: usize,
        line: usize,
    },
    #[error("story {story} carries a reference line but no frame (§10)")]
    ReferenceWithoutFrame { story: StoryNo },
}

/// `Number(digits)` as JavaScript prints it, for the digits of a title line short enough to
/// be exact.
fn number_text(digits: &str) -> &str {
    match digits.trim_start_matches('0') {
        "" => "0",
        significant => significant,
    }
}

/// What a frame, title or reference text cannot be (`frameTextError`, `titleTextError`,
/// `refTextError`, `journal/story.mjs:47-70`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextProblem {
    CarriageReturn,
    /// A frame is exactly one paragraph of non-blank lines.
    BlankLine,
    /// The image line splits frames.
    ImageLine,
    /// A line `_…_` would read back as the reference line.
    ReferenceLine,
    /// A line `# N.` would read back as a title.
    TitleLine,
    NotOneLine,
    OuterWhitespace,
    /// No write removes the reference line.
    Empty,
}

impl fmt::Display for TextProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CarriageReturn => "contains a carriage return",
            Self::BlankLine => {
                "contains a blank line (or starts or ends with one) — a frame is exactly ONE paragraph of non-blank lines (§10)"
            }
            Self::ImageLine => "contains an image line — the image line splits frames (§10)",
            Self::ReferenceLine => "contains a line in the reference-line form `_…_` (§10)",
            Self::TitleLine => "contains a line in the title form `# N.` (§10)",
            Self::NotOneLine => "is not a single line",
            Self::OuterWhitespace => "has leading or trailing whitespace",
            Self::Empty => "is empty — the reference line is removed by no operation (§10)",
        })
    }
}

/// A write the story module refuses (`writeFrame`, `writeTitle`, `writeRef`,
/// `journal/story.mjs:135-174`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("frame text {0}")]
    FrameText(TextProblem),
    #[error("title text {0}")]
    TitleText(TextProblem),
    #[error("reference text {0}")]
    ReferenceText(TextProblem),
    #[error(
        "story {story} has {frames} frames — frame {frame} does not exist (§10: the frame set is fixed by the source)"
    )]
    NoSuchFrame {
        story: StoryNo,
        frames: usize,
        frame: FrameNo,
    },
    #[error("story {story} has no frame — a reference line follows the last frame (§10)")]
    NoFrameForReference { story: StoryNo },
    #[error(transparent)]
    Story(#[from] StoryError),
}

/// A prepared OBS template that is not the pinned one (`validateObsTemplate`,
/// `scripts/fix-obs-template.mjs:246-291`), or template metadata the `localizedNames` fix
/// cannot place (`withLocalizedNames`, `:233-239`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("OBS template metadata has no \"ingredients\" key")]
    NoIngredientsKey,
    #[error("OBS metadata is missing: {0}")]
    MetadataMissing(String),
    #[error("OBS metadata is missing \"localizedNames\": {0}")]
    LocalizedNamesMissing(String),
    #[error("OBS content set mismatch at {dir}: expected {expected}, found {found}")]
    ContentSet {
        dir: String,
        expected: String,
        found: String,
    },
    #[error("OBS reference story is missing: {0}")]
    ReferenceMissing(String),
    /// `what` is `story` for a story file, `content` for the front and back matter.
    #[error("OBS {what} contains carriage return: {path}")]
    CarriageReturn { what: &'static str, path: String },
    #[error("OBS {what} bytes differ from the pinned fixture: {path}")]
    BytesDiffer { what: &'static str, path: String },
}

/// A new OBS project the create step refuses before anything exists
/// (`src/data/serverApi.ts:220-238`, `:653-662`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NewProjectError {
    #[error("a project name cannot contain a quote, a backslash or a control character: {0:?}")]
    UnsafeName(String),
    #[error("unsafe path segment {0:?}")]
    UnsafeAbbreviation(String),
    #[error(
        "content_language_name is REQUIRED (non-null) when content_language_code starts with 'x-'"
    )]
    PrivateLanguageNeedsName,
}
