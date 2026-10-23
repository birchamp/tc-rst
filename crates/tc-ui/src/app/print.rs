//! Community Checking's decisions (unit B8), the parts of tC4's `CommunityChecking.jsx`,
//! `ExportMenu.jsx` and `print/*` that are not drawing. Pure, so they are tested on the host.
//!
//! A preview page's flow is tC4's print DOM (`PrintParts`, `tC4:src/views/print/PrintBook.jsx`)
//! written as markup ([`flow_markup`]): the page measures its sheets with it, as tC4 renders
//! `PrintParts` to static markup in its measuring sheet, and the sheets show the same markup,
//! so what was measured is what shows. The parts come from `tc_api::model::print`.

use tc_api::export::{Run, StoryRun};
use tc_api::home::ProjectKind;
use tc_api::model::print::{FlowBlock, Opening, PAGE_MARGIN_MM, PageSetup, VerseLine};
use tc_api::shown::Direction;

use crate::css::Css;
use crate::i18n::{Key, keys, t};

/// React's escaping of text and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

pub fn dir_attr(dir: Direction) -> &'static str {
    match dir {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// The line for a run `from`–`to`: `one` words a run of one, `many` a longer run.
fn run_text(one: Key, many: Key, from: u16, to: u16) -> String {
    if from == to {
        t(one, &[("n", &from.to_string())])
    } else {
        t(
            many,
            &[("from", &from.to_string()), ("to", &to.to_string())],
        )
    }
}

/// "[ chapter 3 not yet drafted ]", "[ chapters 5–7 not yet drafted ]" (`chapterGapText`).
pub fn chapter_gap_text(run: Run) -> String {
    run_text(
        keys::cc::CHAPTER_NOT_DRAFTED,
        keys::cc::CHAPTERS_NOT_DRAFTED,
        run.from,
        run.to,
    )
}

/// "[ frame 2 not yet drafted ]" (`frameGapText`).
pub fn frame_gap_text(run: Run) -> String {
    run_text(
        keys::cc::FRAME_NOT_DRAFTED,
        keys::cc::FRAMES_NOT_DRAFTED,
        run.from,
        run.to,
    )
}

/// "[ stories 2–3 not yet drafted ]" (`storyGapText`).
pub fn story_gap_text(run: StoryRun) -> String {
    run_text(
        keys::cc::STORY_NOT_DRAFTED,
        keys::cc::STORIES_NOT_DRAFTED,
        u16::from(run.from.get()),
        u16::from(run.to.get()),
    )
}

/// Which sheet a flow's markup is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    /// The measuring sheet: `renderToStaticMarkup`'s markup, as tC4 measures with.
    Measure,
    /// A page: the DOM React draws, with the preview's test handles, and a verse's text and
    /// the space after it as two text nodes, as React writes two children. Chromium sets a
    /// justified line by its text nodes, so one node would move glyphs by a fraction of a
    /// pixel.
    Page,
}

/// The flow of a preview page (`PrintParts`).
pub fn flow_markup(blocks: &[FlowBlock<'_>], dir: Direction, sheet: Sheet) -> String {
    let d = dir_attr(dir);
    let test_ids = sheet == Sheet::Page;
    // A comment parts two text nodes and is no part of the layout.
    let space = if test_ids { "<!-- --> " } else { " " };
    let id = |name: &str| {
        if test_ids {
            format!(" data-testid=\"{name}\"")
        } else {
            String::new()
        }
    };
    let mut out = String::new();
    for block in blocks {
        match block {
            FlowBlock::Gap(run) => out.push_str(&format!(
                "<p class=\"print-chapter-gap\" dir=\"{d}\"{}>{}</p>",
                id("cc-chapter-gap"),
                escape(&chapter_gap_text(*run))
            )),
            FlowBlock::Chapter(chapter) => {
                let class = if chapter.continues {
                    "print-chapter print-chapter-continues"
                } else {
                    "print-chapter"
                };
                out.push_str(&format!(
                    "<section class=\"{class}\" dir=\"{d}\"{}>",
                    id("cc-chapter")
                ));
                match chapter.opening {
                    Some(Opening::DropCap) => out.push_str(&format!(
                        "<span class=\"print-dropcap\">{}</span>",
                        chapter.chapter
                    )),
                    Some(Opening::Heading) => out.push_str(&format!(
                        "<h2 class=\"print-chapter-heading\"{}>{}</h2>",
                        id("cc-chapter-heading"),
                        escape(&t(
                            keys::cc::CHAPTER_HEADING,
                            &[("n", &chapter.chapter.to_string())]
                        ))
                    )),
                    None => {}
                }
                for verse in &chapter.verses {
                    match verse {
                        VerseLine::Text { number, text } => {
                            out.push_str("<span>");
                            if let Some(n) = number {
                                out.push_str(&format!(
                                    "<sup class=\"print-verse-number\">{}</sup>",
                                    escape(n)
                                ));
                            }
                            out.push_str(&format!("{}{space}</span>", escape(text)));
                        }
                        VerseLine::NotDrafted { number } => out.push_str(&format!(
                            "<span class=\"print-undrafted\"><sup class=\"print-verse-number\">{}</sup>{}{space}</span>",
                            escape(number),
                            escape(&t(keys::cc::NOT_YET_DRAFTED, &[]))
                        )),
                    }
                }
                out.push_str("</section>");
            }
        }
    }
    out
}

/// Millimetres in CSS pixels.
pub fn px(mm: f64) -> f64 {
    mm * 96.0 / 25.4
}

/// A sheet of the preview (`Sheet`, `PrintPages.jsx:100-124`): the paper with the print's
/// margins, the page-setup variables print.css reads; `hidden` is the measuring sheet.
pub fn sheet_style(setup: &PageSetup, hidden: bool) -> Css {
    let (w, h) = setup.paper.size_mm();
    let (top, side, bottom) = PAGE_MARGIN_MM;
    let mut style = crate::css! {
        "--print-columns": f64::from(setup.columns.count()),
        "--print-leading": setup.leading(),
        "width": px(w),
        "height": px(h),
        "padding": format!(
            "{}px {}px {}px",
            crate::css::js_number(px(top)),
            crate::css::js_number(px(side)),
            crate::css::js_number(px(bottom))
        ),
        "box-sizing": "border-box",
        "display": "flex",
        "flex-direction": "column",
        "position": if hidden { "absolute" } else { "relative" },
        "background": "#fff",
    };
    if hidden {
        style.extend(&crate::css! {
            "visibility": "hidden",
            "pointer-events": "none",
            "inset-inline-start": 0,
            "top": 0,
        });
    } else {
        style.extend(&crate::css! {
            "box-shadow": "var(--shadow-page)",
            "border-radius": 4,
            "margin": "0 auto 24px",
        });
    }
    style
}

/// A sheet's flow (`FLOW`): it fills column by column; the last sheet balances its columns,
/// as the PDF's last page does.
pub fn flow_style(last: bool) -> Css {
    crate::css! {
        "flex": 1,
        "min-height": 0,
        "column-fill": if last { "balance" } else { "auto" },
        "overflow": "hidden",
    }
}

/// The page number in the sheet's bottom margin (`PAGE_NUMBER`).
pub fn page_number_style() -> Css {
    crate::css! {
        "position": "absolute",
        "inset-inline": 0,
        "bottom": px(PAGE_MARGIN_MM.2) / 2.0 - 7.0,
        "text-align": "center",
        "font-family": "\"Charis SIL\", \"PT Serif\", Georgia, \"Times New Roman\", serif",
        "font-size": 11,
        "color": "#626F78",
    }
}

/// An item of the export menu: a producer that applies to the open project, by its catalog
/// key (`tC4:src/data/export/producers.ts`).
pub fn export_items(kind: ProjectKind, pdf_bridge: bool) -> Vec<Key> {
    let mut items = Vec::new();
    // The PDF prints through the desktop shell; a browser has no PDF item (`pdf.ts:120`).
    if pdf_bridge {
        items.push(keys::cc::EXPORT_PDF);
    }
    match kind {
        ProjectKind::Bible => {
            items.push(keys::cc::EXPORT_USFM_ALIGNED);
            items.push(keys::cc::EXPORT_USFM_PLAIN);
        }
        ProjectKind::Stories => items.push(keys::cc::EXPORT_STORY_MARKDOWN),
    }
    items.push(keys::cc::EXPORT_BURRITO_ZIP);
    items
}
