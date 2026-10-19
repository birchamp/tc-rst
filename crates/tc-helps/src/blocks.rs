//! The blocks the helps panes draw: a port of tC4's `renderArticleBlocks` and `takeSpans`
//! (`tC4:src/data/articles.ts:100-175`, #619).
//!
//! tC4's renderer is deliberately small: a line is a heading (`#` to `######`), a list item
//! (`- ` or `* `) or a paragraph, inside `>` quotes of some depth; links show their label,
//! `[[target]]` its target; `***`, `**`, `__`, `*` and `_` mark bold and italic runs. Anything
//! else stays literal, so a line never loses a word. The screens of tC4 4.0 draw these blocks,
//! so the rebuild draws the same ones (the parity screens compare them). tC4 #628 asks for
//! fuller markdown; that waits for tC4.
//!
//! Lengths are counted as JavaScript counts them, in UTF-16 code units, because tC4 cuts a
//! long note at 400 of them (`previewBlocks`); [`take_spans`] cuts the same text tC4 cuts,
//! except that it never splits a character outside the Basic Multilingual Plane in two, as
//! JavaScript's `slice` does: such a character is left out whole.

use std::sync::LazyLock;

use fancy_regex::{Captures, Regex};

/// What a block is: tC4's `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// A heading, `#` to `######`.
    Heading,
    /// A list item, `- ` or `* `.
    Item,
    Paragraph,
}

/// One run of inline text, with no markdown marks left in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

/// One rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// A heading's level, 1 to 6; `None` for other blocks.
    pub level: Option<u8>,
    /// The depth of the `>` quote that holds the block: 0 for none, 1 for `>`, 2 for `> >`.
    pub quote: u8,
    /// The block's spans; their texts joined are the block's plain text.
    pub spans: Vec<Span>,
}

impl Block {
    /// The plain text of the block: the spans joined (tC4's `text`).
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// JavaScript's `\s`: the characters tC4's `\s`, `\s+` and `trim()` treat as space. Rust's
/// `\s` adds U+0085 and leaves out U+FEFF.
const JS_SPACE: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// A pattern of this module. Each is a constant that the tests compile and run, so a failure
/// to compile is a bug in this file, found by every test of it.
fn pattern(text: &str) -> Regex {
    #[expect(
        clippy::expect_used,
        reason = "a constant pattern that every test of this module compiles"
    )]
    Regex::new(text).expect("a pattern of the block renderer")
}

static QUOTE_MARKS: LazyLock<Regex> = LazyLock::new(|| pattern(&format!("^(?:>[{JS_SPACE}]*)+")));
static HEADING: LazyLock<Regex> = LazyLock::new(|| {
    pattern(&format!(
        "^(#{{1,6}})[{JS_SPACE}]+([^\\n\\r\\x{{2028}}\\x{{2029}}]*)$"
    ))
});
static ITEM: LazyLock<Regex> = LazyLock::new(|| pattern(&format!("^[-*][{JS_SPACE}]+")));
static WIKI_LINK: LazyLock<Regex> = LazyLock::new(|| pattern(r"\[\[([^\]]+)\]\]"));
static LINK: LazyLock<Regex> = LazyLock::new(|| pattern(r"\[([^\]]*)\]\([^)]*\)"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| pattern(&format!("[{JS_SPACE}]+")));
static BOLD_ITALIC: LazyLock<Regex> = LazyLock::new(|| pattern(r"\*\*\*(?!\*)(.+?)(?<!\*)\*\*\*"));
// The resources have marks with a space inside (`**nose **`, `__Israel __`) and `__` after a
// story reference (`17:2__David…__`). These are bold too. A blank of underscores to fill in
// (`________`) is not.
static BOLD: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"\*\*(?!\*)(.+?)(?<!\*)\*\*|(?<!\p{L})__(?!_)(.+?)(?<!_)__"));
// The `*` of an `rc://*/…` link sits next to a `/`: a wildcard, not a mark. Nor is an
// escaped `\*`. `_` opens and closes only at a word edge, so `figs_metaphor` stays literal.
static ITALIC: LazyLock<Regex> = LazyLock::new(|| {
    pattern(&format!(
        r"(?<![*\\])\*(?![{s}*/])([^*]+?)(?<![{s}/])\*(?!\*)|(?<![\p{{L}}\p{{N}}_])_(?![{s}_])([^_]+?)(?<![{s}])_(?![\p{{L}}\p{{N}}_])",
        s = JS_SPACE
    ))
});

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// JavaScript's `trim()`.
fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// The length of `text` as JavaScript counts it: UTF-16 code units.
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

/// `text` with each match of `re` replaced by its first group. A match the engine gives up
/// on (its backtracking limit) ends the replacing; the rest stays as written.
fn replace_with_group(re: &Regex, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for caps in re.captures_iter(text) {
        let Ok(caps) = caps else { break };
        let (Some(whole), group) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        out.push_str(text.get(at..whole.start()).unwrap_or_default());
        out.push_str(group.map_or("", |g| g.as_str()));
        at = whole.end();
    }
    out.push_str(text.get(at..).unwrap_or_default());
    out
}

/// The group a mark's match holds: its first group, else its second (tC4's `m[1] ?? m[2]`).
fn marked<'t>(caps: &Captures<'t, str>) -> &'t str {
    caps.get(1)
        .or_else(|| caps.get(2))
        .map_or("", |g| g.as_str())
}

/// Splits `span` at each match of `mark`; the matched part gets the style `bold` and
/// `italic` adds (tC4's `splitSpans`).
fn split_spans(span: Span, mark: &Regex, bold: bool, italic: bool) -> Vec<Span> {
    let mut out = Vec::new();
    let mut at = 0;
    for caps in mark.captures_iter(span.text.as_str()) {
        let Ok(caps) = caps else { break };
        let Some(whole) = caps.get(0) else { continue };
        if whole.start() > at {
            out.push(Span {
                text: span
                    .text
                    .get(at..whole.start())
                    .unwrap_or_default()
                    .to_owned(),
                ..span.clone()
            });
        }
        out.push(Span {
            text: marked(&caps).to_owned(),
            bold: span.bold || bold,
            italic: span.italic || italic,
        });
        at = whole.end();
    }
    if at < span.text.len() {
        out.push(Span {
            text: span.text.get(at..).unwrap_or_default().to_owned(),
            ..span
        });
    }
    out
}

/// tC4's `inline`: links to their text, escaped and real line breaks to one space, the marks
/// to styled spans.
fn inline(text: &str) -> Vec<Span> {
    let plain = replace_with_group(&WIKI_LINK, text);
    let plain = replace_with_group(&LINK, &plain);
    let plain = plain.replace("\\n", " ");
    let plain = SPACES.replace_all(&plain, " ");
    let plain = js_trim(&plain).to_owned();
    split_spans(Span::plain(plain), &BOLD_ITALIC, true, true)
        .into_iter()
        .flat_map(|span| split_spans(span, &BOLD, true, false))
        .flat_map(|span| split_spans(span, &ITALIC, false, true))
        .collect()
}

/// The blocks of `markdown`, line by line, as tC4's `renderArticleBlocks` gives them. An
/// empty line gives no block.
pub fn render_article_blocks(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    for raw in markdown.split('\n') {
        let trimmed = js_trim(raw);
        let marks = QUOTE_MARKS
            .find(trimmed)
            .ok()
            .flatten()
            .map_or("", |m| m.as_str());
        let quote = u8::try_from(marks.chars().filter(|c| *c == '>').count()).unwrap_or(u8::MAX);
        let line = trimmed.get(marks.len()..).unwrap_or_default();
        if line.is_empty() {
            continue;
        }
        if let Ok(Some(heading)) = HEADING.captures(line) {
            let level = heading.get(1).map_or(1, |m| m.as_str().len());
            blocks.push(Block {
                kind: BlockKind::Heading,
                level: u8::try_from(level).ok(),
                quote,
                spans: inline(heading.get(2).map_or("", |m| m.as_str())),
            });
            continue;
        }
        if let Ok(Some(item)) = ITEM.find(line) {
            blocks.push(Block {
                kind: BlockKind::Item,
                level: None,
                quote,
                spans: inline(line.get(item.end()..).unwrap_or_default()),
            });
            continue;
        }
        blocks.push(Block {
            kind: BlockKind::Paragraph,
            level: None,
            quote,
            spans: inline(line),
        });
    }
    blocks
}

/// The first `length` UTF-16 code units of `spans`, with the style of each span kept (tC4's
/// `takeSpans`).
pub fn take_spans(spans: &[Span], length: usize) -> Vec<Span> {
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
            out.push(Span {
                text: js_slice(&span.text, room).to_owned(),
                ..span.clone()
            });
            room = 0;
        }
    }
    out
}
