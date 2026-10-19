//! Pure models the server and the UI share (APP-DESIGN §3.8): logic both run, here so the two
//! cannot drift.
//!
//! [`draft`] is the draft machine of C1b. [`ArticleBlock`] is a block of help text as the helps
//! panes draw it: tc-helps renders it on the server (`tc_helps::blocks`, a port of tC4's
//! `renderArticleBlocks`), and the UI cuts a long note's preview with [`take_spans`], tC4's
//! `takeSpans`. A differential test in tc-app holds this copy to tc-helps' own.

pub mod draft;

use serde::{Deserialize, Serialize};

/// What a block is: tC4's `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
    /// A heading.
    H,
    /// A paragraph.
    P,
    /// A list item.
    Li,
    /// A numbered list item (#628).
    Ol,
    /// A table row (#628).
    Tr,
}

/// One run of text with its style.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ArticleSpan {
    pub text: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One line of help text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ArticleBlock {
    pub kind: BlockKind,
    /// The depth of the `>` quote around the block: 0 for none.
    pub quote: u8,
    /// A row's are its cells', a space between.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub spans: Vec<ArticleSpan>,
    /// A list item's nesting under the items above it: 0 at the left edge (#628).
    pub depth: u8,
    /// A numbered item's number as written.
    pub number: Option<String>,
    /// A table row's cells.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few_lists()"))]
    pub cells: Vec<Vec<ArticleSpan>>,
    /// A table's heading row.
    pub header: bool,
}

impl ArticleBlock {
    /// A block that is not in a list or a table.
    pub fn new(kind: BlockKind, quote: u8, spans: Vec<ArticleSpan>) -> Self {
        Self {
            kind,
            quote,
            spans,
            depth: 0,
            number: None,
            cells: Vec::new(),
            header: false,
        }
    }

    /// The block's plain text: its spans joined (tC4's `text`).
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// The length of `text` as JavaScript counts it: UTF-16 code units, the unit tC4's preview
/// counts.
pub fn js_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The first `units` UTF-16 code units of `text`, never half a character.
fn js_slice(text: &str, units: usize) -> &str {
    let mut used = 0;
    for (at, c) in text.char_indices() {
        if used + c.len_utf16() > units {
            return text.get(..at).unwrap_or(text);
        }
        used += c.len_utf16();
    }
    text
}

/// The first `length` UTF-16 code units of `spans`, with each span's style kept (tC4's
/// `takeSpans`, `tC4:src/data/articles.ts:127-136`). A character outside the Basic
/// Multilingual Plane is never split in two: it is left out whole.
pub fn take_spans(spans: &[ArticleSpan], length: usize) -> Vec<ArticleSpan> {
    let mut out = Vec::new();
    let mut room = length;
    for span in spans {
        if room == 0 {
            break;
        }
        let len = js_len(&span.text);
        if len <= room {
            out.push(span.clone());
            room -= len;
        } else {
            out.push(ArticleSpan {
                text: js_slice(&span.text, room).to_owned(),
                ..span.clone()
            });
            room = 0;
        }
    }
    out
}
