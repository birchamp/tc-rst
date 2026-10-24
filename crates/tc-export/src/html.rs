//! The print document the window prints to PDF (tC4 `src/data/export/pdf.ts`): the page
//! setup as CSS, the print stylesheet, and the print DOM of a book (`PrintBook.jsx`) or of an
//! OBS project (`PrintStories.jsx`) as React's `renderToStaticMarkup` writes it. What prints
//! is [`crate::book::print_book`]'s and [`crate::stories::print_stories`]'s, on tc-api's
//! print model, so the preview and the PDF read one model.

use tc_api::export::PrintItem;
use tc_api::model::print::{FlowBlock, ObsLayout, Opening, PageSetup, VerseLine};
use tc_obs::print::PrintedStory;

use crate::book::{BookView, print_book};
use crate::page::{PAGE_MARGIN_MM, PrintVariables, css_size, variables};
use crate::stories::{
    FrameBlock, Side, StoriesView, StoryBlock, StoryPictures, StoryTitle, print_stories,
};
use crate::{Direction, ExportError, PrintText, Run};

/// The words of a print's lines in the interface language: tC4's `t(key, values)`.
pub type Words<'a> = &'a dyn Fn(PrintText) -> String;

/// A book's print.
pub struct PrintBookInput<'a> {
    /// The book's name in the interface language (`bookName`), the print's heading.
    pub title: String,
    /// The document's `<title>`, which the shell names the saved PDF after: the export's file
    /// name without `.pdf`, where tC4's document carries the book's name.
    pub document_title: String,
    /// [`crate::book::print_items`] of the book.
    pub items: Vec<PrintItem>,
    pub setup: PageSetup,
    pub dir: Direction,
    /// The print stylesheet, tC4's `print.css`.
    pub stylesheet: &'a str,
    pub words: Words<'a>,
}

/// An OBS project's print.
pub struct PrintStoriesInput<'a> {
    /// The project's name.
    pub title: String,
    /// The document's `<title>`, as [`PrintBookInput::document_title`].
    pub document_title: String,
    pub items: &'a [PrintedStory<'a>],
    /// Each printed frame's picture as a `data:` URL: the document stands alone, so the print
    /// window needs no address of the app's.
    pub pictures: StoryPictures,
    pub setup: PageSetup,
    pub dir: Direction,
    pub stylesheet: &'a str,
    pub words: Words<'a>,
}

/// A book's print document (`printDocument`). A book with no drafted verse has nothing to
/// print.
pub fn book_document(input: &PrintBookInput<'_>) -> Result<String, ExportError> {
    if input.items.is_empty() {
        return Err(ExportError::NothingDrafted);
    }
    let view = print_book(&input.title, &input.items, &input.setup, input.dir);
    Ok(shell(
        &input.document_title,
        &book_markup(&view, input.words),
        &input.setup,
        input.dir,
        input.stylesheet,
    ))
}

/// An OBS project's print document (`printStoriesDocument`).
pub fn stories_document(input: &PrintStoriesInput<'_>) -> String {
    let view = print_stories(input.items, &input.pictures, &input.setup, input.dir);
    shell(
        &input.document_title,
        &stories_markup(&view, input.words),
        &input.setup,
        input.dir,
        input.stylesheet,
    )
}

/// `PrintBook`: the whole book as one flow.
pub fn book_markup(view: &BookView<'_>, words: Words<'_>) -> String {
    let d = dir_attr(view.dir);
    format!(
        "<main class=\"print-book\"><h1 class=\"print-title\" dir=\"{d}\">{}</h1><div class=\"print-flow\" dir=\"{d}\">{}</div></main>",
        escape(view.title),
        flow_markup(&view.flow, view.dir, words)
    )
}

/// `PrintStories`.
pub fn stories_markup(view: &StoriesView<'_>, words: Words<'_>) -> String {
    let d = dir_attr(view.dir);
    let mut body = String::from(match view.layout {
        ObsLayout::Wrapped => "<main class=\"print-book print-stories print-stories-wrapped\">",
        ObsLayout::Above => "<main class=\"print-book print-stories\">",
    });
    if let Some(line) = view.nothing_drafted() {
        body.push_str(&format!(
            "<p class=\"print-nothing-drafted\" dir=\"{d}\">{}</p>",
            escape(&words(line))
        ));
    }
    for block in &view.blocks {
        match block {
            StoryBlock::Gap(line) => body.push_str(&format!(
                "<p class=\"print-story-gap\" dir=\"{d}\">{}</p>",
                escape(&words(*line))
            )),
            StoryBlock::Story(story) => {
                let title = match story.title {
                    StoryTitle::Own(title) => title.to_owned(),
                    StoryTitle::Numbered(line) => words(line),
                };
                body.push_str(&format!(
                    "<section class=\"print-story\" dir=\"{d}\"><h1 class=\"print-title print-story-title\">{}</h1>",
                    escape(&title)
                ));
                for frame in &story.frames {
                    match frame {
                        FrameBlock::Gap(line) => body.push_str(&format!(
                            "<p class=\"print-frame-gap\">{}</p>",
                            escape(&words(*line))
                        )),
                        FrameBlock::Frame(frame) => {
                            body.push_str("<div class=\"print-frame\">");
                            if let Some(picture) = &frame.picture {
                                let style = match picture.float {
                                    Some(Side::Left) => " style=\"float:left\"",
                                    Some(Side::Right) => " style=\"float:right\"",
                                    None => "",
                                };
                                body.push_str(&format!(
                                    "<img class=\"print-frame-picture\" src=\"{}\" alt=\"{}\"{style}/>",
                                    escape(picture.src),
                                    escape(&words(picture.alt))
                                ));
                            }
                            body.push_str(&format!(
                                "<p class=\"print-frame-text\">{}</p></div>",
                                escape(frame.text)
                            ));
                        }
                    }
                }
                if let Some(reference) = story.reference {
                    body.push_str(&format!(
                        "<p class=\"print-story-reference\">{}</p>",
                        escape(reference)
                    ));
                }
                body.push_str("</section>");
            }
        }
    }
    body.push_str("</main>");
    body
}

/// The flow's parts (`PrintParts` without the preview's test handles).
pub fn flow_markup(blocks: &[FlowBlock<'_>], dir: Direction, words: Words<'_>) -> String {
    let mut out = String::new();
    for block in blocks {
        flow_block(&mut out, block, dir_attr(dir), words);
    }
    out
}

fn flow_block(out: &mut String, block: &FlowBlock<'_>, d: &str, words: Words<'_>) {
    match block {
        FlowBlock::Gap(run) => out.push_str(&format!(
            "<p class=\"print-chapter-gap\" dir=\"{d}\">{}</p>",
            escape(&words(PrintText::ChaptersNotDrafted(Run {
                from: run.from,
                to: run.to,
            })))
        )),
        FlowBlock::Chapter(chapter) => {
            let class = if chapter.continues {
                "print-chapter print-chapter-continues"
            } else {
                "print-chapter"
            };
            out.push_str(&format!("<section class=\"{class}\" dir=\"{d}\">"));
            match chapter.opening {
                Some(Opening::DropCap) => out.push_str(&format!(
                    "<span class=\"print-dropcap\">{}</span>",
                    chapter.chapter
                )),
                Some(Opening::Heading) => out.push_str(&format!(
                    "<h2 class=\"print-chapter-heading\">{}</h2>",
                    escape(&words(PrintText::ChapterHeading {
                        chapter: chapter.chapter
                    }))
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
                        out.push_str(&format!("{} </span>", escape(text)));
                    }
                    VerseLine::NotDrafted { number } => out.push_str(&format!(
                        "<span class=\"print-undrafted\"><sup class=\"print-verse-number\">{}</sup>{} </span>",
                        escape(number),
                        escape(&words(PrintText::VerseNotDrafted))
                    )),
                }
            }
            out.push_str("</section>");
        }
    }
}

/// `printShell`: the page (its size, margins and number), the page setup as CSS variables,
/// the print stylesheet, then `body`.
fn shell(title: &str, body: &str, page: &PageSetup, dir: Direction, stylesheet: &str) -> String {
    let vars = variables(page);
    let setup = format!(
        "@page {{ size: {}; margin: {}mm {}mm {}mm;\n  @bottom-center {{ content: counter(page); font-family: \"Charis SIL\", \"PT Serif\", Georgia, \"Times New Roman\", serif; font-size: 11px; color: #626F78; }} }}\nhtml {{ background: #fff; }}\nbody {{ margin: 0; }}\n:root {{ {}: {}; {}: {}; }}",
        css_size(page.paper),
        PAGE_MARGIN_MM.top,
        PAGE_MARGIN_MM.side,
        PAGE_MARGIN_MM.bottom,
        PrintVariables::COLUMNS,
        vars.columns,
        PrintVariables::LEADING,
        vars.leading,
    );
    format!(
        "<!doctype html>\n<html dir=\"{}\"><head><meta charset=\"utf-8\"><title>{}</title>\n<style>{setup}\n{}</style></head>\n<body>{body}</body></html>",
        dir_attr(dir),
        escape_title(title),
        stylesheet,
    )
}

fn dir_attr(dir: Direction) -> &'static str {
    match dir {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// React's escaping of text and attribute values.
fn escape(text: &str) -> String {
    escape_with(text, true)
}

/// `pdf.ts`'s `escapeHtml`, which leaves `'` as it is.
fn escape_title(text: &str) -> String {
    escape_with(text, false)
}

fn escape_with(text: &str, apostrophe: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' if apostrophe => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}
