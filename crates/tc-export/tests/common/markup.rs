//! The print model written as the markup tC4's print components render
//! (`renderToStaticMarkup` of `PrintBook`, `PrintParts` and `PrintStories`), so a test can
//! compare the model with tC4's own output byte for byte. It only writes what the model
//! holds: if the model lost a distinction tC4's markup makes, the markup would differ.

use serde_json::Value;
use tc_export::book::{BookView, FlowBlock, Opening, VerseLine};
use tc_export::page::ObsLayout;
use tc_export::stories::{FrameBlock, Side, StoriesView, StoryBlock, StoryTitle};
use tc_export::{Direction, PrintText};

use super::vectors;

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

/// A line worded from tC4's English catalog, as `t(key, values)` words it.
pub fn word(text: PrintText) -> String {
    let catalog: &Value = &vectors()["catalog"];
    let mut out = catalog[text.key()]
        .as_str()
        .unwrap_or_else(|| panic!("no catalog entry {}", text.key()))
        .to_owned();
    for (name, value) in text.values() {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    escape(&out)
}

fn dir(d: Direction) -> &'static str {
    match d {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// `PrintParts`.
pub fn flow(blocks: &[FlowBlock<'_>], d: Direction) -> String {
    let d = dir(d);
    let mut out = String::new();
    for block in blocks {
        match block {
            FlowBlock::Gap(text) => {
                out.push_str(&format!(
                    "<p class=\"print-chapter-gap\" dir=\"{d}\">{}</p>",
                    word(*text)
                ));
            }
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
                    Some(Opening::Heading(text)) => out.push_str(&format!(
                        "<h2 class=\"print-chapter-heading\">{}</h2>",
                        word(text)
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
                            word(PrintText::VerseNotDrafted)
                        )),
                    }
                }
                out.push_str("</section>");
            }
        }
    }
    out
}

/// `PrintBook`.
pub fn book(view: &BookView<'_>) -> String {
    let d = dir(view.dir);
    format!(
        "<main class=\"print-book\"><h1 class=\"print-title\" dir=\"{d}\">{}</h1><div class=\"print-flow\" dir=\"{d}\">{}</div></main>",
        escape(view.title),
        flow(&view.flow, view.dir)
    )
}

/// `PrintStories`.
pub fn stories(view: &StoriesView<'_>) -> String {
    let d = dir(view.dir);
    let mut out = String::from(match view.layout {
        ObsLayout::Above => "<main class=\"print-book print-stories\">",
        ObsLayout::Wrapped => "<main class=\"print-book print-stories print-stories-wrapped\">",
    });
    if let Some(text) = view.nothing_drafted() {
        out.push_str(&format!(
            "<p class=\"print-nothing-drafted\" dir=\"{d}\">{}</p>",
            word(text)
        ));
    }
    for block in &view.blocks {
        match block {
            StoryBlock::Gap(text) => out.push_str(&format!(
                "<p class=\"print-story-gap\" dir=\"{d}\">{}</p>",
                word(*text)
            )),
            StoryBlock::Story(story) => {
                let title = match story.title {
                    StoryTitle::Own(title) => escape(title),
                    StoryTitle::Numbered(text) => word(text),
                };
                out.push_str(&format!(
                    "<section class=\"print-story\" dir=\"{d}\"><h1 class=\"print-title print-story-title\">{title}</h1>"
                ));
                for frame in &story.frames {
                    match frame {
                        FrameBlock::Gap(text) => out
                            .push_str(&format!("<p class=\"print-frame-gap\">{}</p>", word(*text))),
                        FrameBlock::Frame(frame) => {
                            out.push_str("<div class=\"print-frame\">");
                            if let Some(picture) = &frame.picture {
                                let style = match picture.float {
                                    None => String::new(),
                                    Some(Side::Left) => " style=\"float:left\"".into(),
                                    Some(Side::Right) => " style=\"float:right\"".into(),
                                };
                                out.push_str(&format!(
                                    "<img class=\"print-frame-picture\" src=\"{}\" alt=\"{}\"{style}/>",
                                    escape(picture.src),
                                    word(picture.alt)
                                ));
                            }
                            out.push_str(&format!(
                                "<p class=\"print-frame-text\">{}</p></div>",
                                escape(frame.text)
                            ));
                        }
                    }
                }
                if let Some(reference) = story.reference {
                    out.push_str(&format!(
                        "<p class=\"print-story-reference\">{}</p>",
                        escape(reference)
                    ));
                }
                out.push_str("</section>");
            }
        }
    }
    out.push_str("</main>");
    out
}
