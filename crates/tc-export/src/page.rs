//! What the PDF reads of the page setup beyond tc-api's model (`tc_api::model::print`): the
//! margins as named values, the variables the print stylesheet reads, and the paper's CSS
//! name (`src/data/export/pageSetup.ts` and `src/data/export/pdf.ts` in tC4). The page setup
//! itself, its choices and their defaults, is tc-api's, so the preview and the PDF read one.

use tc_api::model::print::{self, PageSetup, Paper};

/// The page margins in millimetres. The bottom margin holds the page number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub top: f64,
    pub side: f64,
    pub bottom: f64,
}

/// The margins of every page, on both papers (`PAGE_MARGIN_MM`).
pub const PAGE_MARGIN_MM: Margins = Margins {
    top: print::PAGE_MARGIN_MM.0,
    side: print::PAGE_MARGIN_MM.1,
    bottom: print::PAGE_MARGIN_MM.2,
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

pub fn variables(setup: &PageSetup) -> PrintVariables {
    PrintVariables {
        columns: setup.columns.count(),
        leading: setup.leading(),
    }
}

/// The paper's name as a CSS `@page` size (`PAGE_SIZE` in `pdf.ts`).
pub fn css_size(paper: Paper) -> &'static str {
    match paper {
        Paper::A4 => "A4",
        Paper::Letter => "letter",
    }
}
