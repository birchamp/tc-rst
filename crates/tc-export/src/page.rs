//! The page setup of a print (`src/data/export/pageSetup.ts`, and the page values of
//! `src/views/print/PrintBook.jsx:14-27` in tC4). The preview's page setup is view state:
//! it starts at [`PageSetup::default`] each time Community Checking opens, and every
//! export of that preview takes it.

/// The columns a Bible book's text runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Columns {
    #[default]
    One,
    Two,
}

impl Columns {
    pub fn count(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

/// The line spacing: double is twice single.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Spacing {
    #[default]
    Single,
    Double,
}

impl Spacing {
    /// What single leading is multiplied by (`PAGE_SPACING_FACTOR`).
    pub fn factor(self) -> u8 {
        match self {
            Self::Single => 1,
            Self::Double => 2,
        }
    }
}

/// The paper a print is set on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paper {
    /// 210 × 297 mm (595 × 842 pt).
    #[default]
    A4,
    /// US Letter, 215.9 × 279.4 mm (612 × 792 pt).
    Letter,
}

/// A width and a height in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeMm {
    pub width: f64,
    pub height: f64,
}

impl Paper {
    /// The paper's size (`PAPER_MM`): the PDF's page and the preview's sheets.
    pub fn size_mm(self) -> SizeMm {
        match self {
            Self::A4 => SizeMm {
                width: 210.0,
                height: 297.0,
            },
            Self::Letter => SizeMm {
                width: 215.9,
                height: 279.4,
            },
        }
    }

    /// The paper's name as a CSS `@page` size (`PAGE_SIZE` in `pdf.ts`).
    pub fn css_size(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::Letter => "letter",
        }
    }
}

/// How a chapter of a Bible book opens: its number as a drop cap, or a "Chapter N" heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChapterOpening {
    #[default]
    DropCap,
    Heading,
}

/// Whether a drafted verse shows its number. An undrafted verse always does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerseNumbers {
    #[default]
    Shown,
    Hidden,
}

/// Whether an OBS frame shows its picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pictures {
    #[default]
    Shown,
    Hidden,
}

/// Where an OBS frame's picture sits: above its text, or at the start corner of the frame
/// with the text wrapped around it, a quarter of the page wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObsLayout {
    #[default]
    Above,
    Wrapped,
}

/// Every choice of the page setup. [`PageSetup::default`] is tC4's `DEFAULT_PAGE_SETUP`.
/// Columns, chapter openings, verse numbers and spacing apply to a Bible book; pictures and
/// the layout to an OBS project; the paper to both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageSetup {
    pub columns: Columns,
    pub chapter_opening: ChapterOpening,
    pub verse_numbers: VerseNumbers,
    pub spacing: Spacing,
    pub paper: Paper,
    pub pictures: Pictures,
    pub obs_layout: ObsLayout,
}

/// Single leading, as a multiple of the verse text's size (`PRINT_LEADING`).
pub const PRINT_LEADING: f64 = 1.4;

/// The page margins in millimetres. The bottom margin holds the page number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub top: f64,
    pub side: f64,
    pub bottom: f64,
}

/// The margins of every page, on both papers (`PAGE_MARGIN_MM`).
pub const PAGE_MARGIN_MM: Margins = Margins {
    top: 18.0,
    side: 16.0,
    bottom: 20.0,
};

/// The values the print stylesheet reads from the page setup (`printVariables`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrintVariables {
    /// `--print-columns`.
    pub columns: u8,
    /// `--print-leading`.
    pub leading: f64,
}

impl PrintVariables {
    pub const COLUMNS: &'static str = "--print-columns";
    pub const LEADING: &'static str = "--print-leading";
}

impl PageSetup {
    /// The line height, as a multiple of the verse text's size.
    pub fn leading(&self) -> f64 {
        PRINT_LEADING * f64::from(self.spacing.factor())
    }

    pub fn variables(&self) -> PrintVariables {
        PrintVariables {
            columns: self.columns.count(),
            leading: self.leading(),
        }
    }
}
