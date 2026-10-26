//! The print model written as the markup tC4's print components render
//! (`renderToStaticMarkup` of `PrintBook`, `PrintParts` and `PrintStories`), by
//! `tc_export::html`, worded from tC4's English catalog, so a test can compare the print
//! document's body with tC4's own output byte for byte.

use serde_json::Value;
use tc_api::model::print::FlowBlock;
use tc_export::book::BookView;
use tc_export::stories::StoriesView;
use tc_export::{Direction, PrintText, html};

use super::vectors;

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
    out
}

/// `PrintParts`.
pub fn flow(blocks: &[FlowBlock<'_>], d: Direction) -> String {
    html::flow_markup(blocks, d, &word)
}

/// `PrintBook`.
pub fn book(view: &BookView<'_>) -> String {
    html::book_markup(view, &word)
}

/// `PrintStories`.
pub fn stories(view: &StoriesView<'_>) -> String {
    html::stories_markup(view, &word)
}
