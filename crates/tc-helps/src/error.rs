use std::fmt;

use crate::collection::HelpsPart;
use crate::metadata::HelpsSource;

/// Every way this crate refuses input. Each variant names where the problem is, so a
/// refused resource can be reported to the user precisely.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("{table} TSV header {found:?} is not a known version of the table")]
    TsvHeader { table: TsvTable, found: String },

    #[error("{table} TSV line {line} has {found} columns; the header has {expected}")]
    TsvColumnCount {
        table: TsvTable,
        line: usize,
        found: usize,
        expected: usize,
    },

    #[error("{table} TSV line {line}, column {column}: {problem}")]
    TsvCell {
        table: TsvTable,
        line: usize,
        column: &'static str,
        problem: CellProblem,
    },

    #[error("cannot write {table} TSV: {problem}")]
    TsvWrite {
        table: TsvTable,
        problem: WriteProblem,
    },

    #[error("link {href:?}: {problem}")]
    Link { href: String, problem: LinkProblem },

    #[error("{file}: {problem}")]
    Yaml { file: YamlFile, problem: String },

    #[error("{file}: {problem}")]
    Json { file: JsonFile, problem: String },

    #[error("OBS story line {line}: {problem}")]
    Story { line: usize, problem: StoryProblem },

    #[error("Aquifer article {content_id:?}, field {field}: {problem}")]
    AquiferArticle {
        content_id: String,
        field: &'static str,
        problem: String,
    },

    #[error("pin does not match the resource: {0}")]
    PinMismatch(PinMismatch),

    #[error("the helps set holds {part} from {held}, so it cannot take them from {offered} too")]
    PinConflict {
        part: HelpsPart,
        held: Box<HelpsSource>,
        offered: Box<HelpsSource>,
    },

    #[error("cannot write OBS story: {0}")]
    StoryWrite(StoryWriteProblem),

    #[error("invalid {what} {value:?}")]
    InvalidValue { what: &'static str, value: String },

    #[error("alignment {0} does not exist in this verse")]
    UnknownAlignment(usize),
}

/// The tables a TSV parser reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsvTable {
    Notes,
    WordLinks,
    Questions,
}

impl fmt::Display for TsvTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TsvTable::Notes => "translation notes",
            TsvTable::WordLinks => "translation words links",
            TsvTable::Questions => "translation questions",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CellProblem {
    #[error("{0:?} is not a reference this table allows")]
    Reference(String),
    #[error("{0:?} is not a row id")]
    RowId(String),
    #[error("{0:?} is not an occurrence")]
    Occurrence(String),
    #[error("{0:?} is not a book code")]
    Book(String),
    #[error("the row is for {found}, but the file is for {expected}")]
    WrongBook { found: String, expected: String },
    #[error("{0}")]
    Link(LinkProblem),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteProblem {
    #[error("a {what} from Aquifer has no TSV form")]
    AquiferItem { what: &'static str },
    #[error("reference {0} has no form in this table")]
    Reference(String),
    #[error("the table has no {column} column, so the value would be lost")]
    NoColumn { column: &'static str },
    #[error("the table is for {expected}, but an item is for {found}")]
    WrongBook { expected: String, found: String },
    #[error("the legacy nine-column table holds only Bible books")]
    LegacyObs,
    #[error(
        "tag {0:?} would read back as other tags: a tag is not empty, has no `;` and no spaces at either end"
    )]
    Tag(String),
    #[error("cell text {0:?} contains a tab or line break the column cannot carry")]
    Unwritable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinkProblem {
    #[error("this kind of link is not supported")]
    Unsupported,
    #[error("the path leaves the resource")]
    OutsideResource,
    #[error("the link names no resource article or passage")]
    NoTarget,
    #[error("{0:?} is not a book code")]
    Book(String),
    #[error("{0:?} is not a chapter or verse number")]
    Number(String),
    #[error("{0:?} is not a translation academy manual")]
    Manual(String),
    #[error("{0:?} is not a translation words category")]
    Category(String),
    #[error("{0:?} is not an article slug")]
    Slug(String),
    #[error("{0:?} is not an Aquifer content id")]
    ContentId(String),
    #[error("{0}")]
    Reference(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YamlFile {
    Manifest,
    AcademyToc,
    AcademyConfig,
}

impl fmt::Display for YamlFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            YamlFile::Manifest => "manifest.yaml",
            YamlFile::AcademyToc => "toc.yaml",
            YamlFile::AcademyConfig => "config.yaml",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonFile {
    BurritoMetadata,
    AquiferMetadata,
    AquiferArticles,
}

impl fmt::Display for JsonFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            JsonFile::BurritoMetadata => "Scripture Burrito metadata.json",
            JsonFile::AquiferMetadata => "Aquifer metadata.json",
            JsonFile::AquiferArticles => "Aquifer content JSON",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoryProblem {
    #[error("a carriage return that does not end a line; OBS stories use LF line ends")]
    CarriageReturn,
    #[error("the first line is not the title form `# N.` or `# N. Title`")]
    Title,
    #[error("story number {0} is outside 1 to 50")]
    StoryNumber(u32),
    #[error("text before the first image line")]
    TextBeforeFirstFrame,
    #[error("frame {frame} has more than one paragraph")]
    SecondParagraph { frame: usize },
    #[error("the reference line has no text between the underscores")]
    EmptyReference,
    #[error("a reference line but no frame")]
    ReferenceWithoutFrames,
}

/// The one-line fields of an OBS story.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryField {
    Title,
    Reference,
    ImageAlt,
    ImageUrl,
}

impl fmt::Display for StoryField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StoryField::Title => "title",
            StoryField::Reference => "reference",
            StoryField::ImageAlt => "image alt text",
            StoryField::ImageUrl => "image address",
        })
    }
}

/// What a story file cannot carry. Frames count from 1.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoryWriteProblem {
    #[error("story number {0} is outside 1 to 50")]
    StoryNumber(u8),
    #[error("the {field} has a line break (frame {frame:?})")]
    LineBreak {
        field: StoryField,
        frame: Option<usize>,
    },
    #[error("the image of frame {frame} has `]` in its alt text or `)` in its address")]
    ImageForm { frame: usize },
    #[error("the text of frame {frame} has a carriage return")]
    CarriageReturn { frame: usize },
    #[error("the text of frame {frame} has a blank line, which would start a second paragraph")]
    BlankLine { frame: usize },
    #[error("a line of frame {frame} is an image line, which would start another frame")]
    ImageLine { frame: usize },
    #[error("the last line of frame {frame} is in the `_…_` form and would read as the reference")]
    ReadsAsReference { frame: usize },
    #[error("the reference is blank")]
    EmptyReference,
    #[error("a reference but no frame")]
    ReferenceWithoutFrames,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PinMismatch {
    #[error("the pin names repository {pinned:?}, the resource says {found:?}")]
    Repository { pinned: String, found: String },
    #[error("the pin names commit {pinned}, the resource says {found}")]
    Revision { pinned: String, found: String },
    #[error("the resource does not state its revision")]
    NoRevision,
    #[error("the pin names language {pinned:?}, the resource says {found:?}")]
    Language { pinned: String, found: String },
}
