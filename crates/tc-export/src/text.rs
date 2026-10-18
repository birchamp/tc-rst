//! The lines and labels of a print that the interface language words.

/// A run of numbered parts, `from` to `to`; one part when they are equal. tC4 writes it
/// `[from, to]` and words a run of one apart from a longer run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Run<T> {
    pub from: T,
    pub to: T,
}

impl<T: Copy + PartialEq> Run<T> {
    pub fn single(n: T) -> Self {
        Self { from: n, to: n }
    }

    pub fn is_single(&self) -> bool {
        self.from == self.to
    }
}

/// A line or label of a print, as tC4's catalog key ([`PrintText::key`]) and the values
/// its `{name}` placeholders take ([`PrintText::values`]). The caller words it from the
/// catalog of the language it shows, as tC4's `t(key, values)` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintText {
    /// A chapter's heading when drop caps are off: "Chapter 3".
    ChapterHeading { chapter: u16 },
    /// The line for a run of undrafted chapters: "[ chapter 3 not yet drafted ]",
    /// "[ chapters 5–7 not yet drafted ]".
    ChaptersNotDrafted(Run<u16>),
    /// An undrafted verse, after its number: "[ verse not yet drafted ]".
    VerseNotDrafted,
    /// The line for a run of undrafted frames: "[ frame 2 not yet drafted ]".
    FramesNotDrafted(Run<usize>),
    /// The line for a run of undrafted stories: "[ stories 3–6 not yet drafted ]".
    StoriesNotDrafted(Run<u8>),
    /// An untitled story's title: "Story 2".
    StoryNumber { story: u8 },
    /// A frame picture's alternative text: "Story frame 1".
    FramePicture { frame: usize },
    /// The one line of an OBS print with no story drafted: "Nothing is drafted in this
    /// project yet."
    NothingDrafted,
}

impl PrintText {
    /// The catalog key (`src/i18n/en.json` in tC4).
    pub fn key(self) -> &'static str {
        match self {
            Self::ChapterHeading { .. } => "cc.chapterHeading",
            Self::ChaptersNotDrafted(run) if run.is_single() => "cc.chapterNotDrafted",
            Self::ChaptersNotDrafted(_) => "cc.chaptersNotDrafted",
            Self::VerseNotDrafted => "cc.notYetDrafted",
            Self::FramesNotDrafted(run) if run.is_single() => "cc.frameNotDrafted",
            Self::FramesNotDrafted(_) => "cc.framesNotDrafted",
            Self::StoriesNotDrafted(run) if run.is_single() => "cc.storyNotDrafted",
            Self::StoriesNotDrafted(_) => "cc.storiesNotDrafted",
            Self::StoryNumber { .. } => "storyDraft.storyNumber",
            Self::FramePicture { .. } => "storyDraft.imageAlt",
            Self::NothingDrafted => "cc.nothingDraftedObs",
        }
    }

    /// The values of the key's placeholders, by name (`n`, or `from` and `to`).
    pub fn values(self) -> Vec<(&'static str, u64)> {
        fn run<T: Copy + PartialEq + Into<u64>>(run: Run<T>) -> Vec<(&'static str, u64)> {
            if run.is_single() {
                vec![("n", run.from.into())]
            } else {
                vec![("from", run.from.into()), ("to", run.to.into())]
            }
        }
        // A frame number is a usize; every target this builds for has at most 64-bit usize.
        let frame = |n: usize| n as u64;
        match self {
            Self::ChapterHeading { chapter } => vec![("n", chapter.into())],
            Self::ChaptersNotDrafted(r) => run(r),
            Self::FramesNotDrafted(r) => run(Run {
                from: frame(r.from),
                to: frame(r.to),
            }),
            Self::StoriesNotDrafted(r) => run(r),
            Self::StoryNumber { story } => vec![("n", story.into())],
            Self::FramePicture { frame: n } => vec![("n", frame(n))],
            Self::VerseNotDrafted | Self::NothingDrafted => Vec::new(),
        }
    }
}
