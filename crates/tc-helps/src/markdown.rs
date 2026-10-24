//! The markdown dialect help text is held in, and the conversion of Aquifer's HTML into it.
//!
//! # The dialect
//!
//! Door43 helps are already markdown, and the app renders them with a small renderer.
//! The dialect is the CommonMark subset that renderer needs:
//!
//! - `#` to `######` headings;
//! - paragraphs separated by a blank line; a single newline inside a block is a line
//!   break;
//! - `* ` bullet items and `1. ` numbered items; nested content is indented;
//! - `> ` block quotes, which may nest;
//! - `**strong**` and `*emphasis*`;
//! - `[label](href)` links and `[[href]]` links that show the target's title;
//! - `![alt](src)` images;
//! - fenced code blocks and `` `code` ``;
//! - backslash escapes for characters that would otherwise be marks.
//!
//! Link targets are resolved by [`crate::links::resolve_link`].
//!
//! # What the HTML conversion loses
//!
//! [`html_to_markdown`] keeps every word of text and every link. It loses:
//!
//! - text direction (`<span style="direction: …">`); the UI sets direction per language;
//! - underline (`<u>`) and superscript or subscript (`<sup>`, `<sub>`): the text stays,
//!   the formatting goes, so a superscript verse number runs into the next word;
//! - table layout: each row becomes one line of `|`-separated cells. Some Aquifer tables
//!   hold a whole markdown table pasted into one row, and that stays one long line;
//! - `<cite>` and `<i>` become emphasis, and `<b>` becomes strong;
//! - `id`, `class`, `target`, `rel` and `style` attributes, except the `class` and `data-*`
//!   attributes of `<data>` tags, which become `aquifer:` links;
//! - `<a>` tags without an `href` keep only their text (Aquifer has some);
//! - a link inside a link (`<data class="bible-ref"><a href>…</a></data>` in two real
//!   articles): CommonMark cannot nest links, so the inner one keeps only its text and
//!   the outer target stays; an image inside a link keeps only its alt text;
//! - elements nested more than 64 deep (only malformed HTML does this): the deeper tags
//!   are dropped, their text stays;
//! - HTML comments.
//!
//! Hrefs are written with whitespace, `(`, `)` and `|` percent-encoded, so that no href
//! ends early or splits a table cell.

use crate::html::{self, Element, Node};
use crate::links::scan;

/// Converts Aquifer article HTML to the markdown dialect.
pub fn html_to_markdown(html: &str) -> String {
    blocks(&html::parse(html)).join("\n\n")
}

pub(crate) fn nodes_to_markdown(nodes: &[Node]) -> String {
    blocks(nodes).join("\n\n")
}

const BLOCKS: [&str; 18] = [
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "table",
    "hr",
    "section",
    "article",
    "figure",
];

fn is_block(node: &Node) -> bool {
    matches!(node, Node::Element(e) if BLOCKS.contains(&e.name.as_str()))
}

fn blocks(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    let mut run: Vec<&Node> = Vec::new();
    for node in nodes {
        if is_block(node) {
            flush(&mut run, &mut out);
            if let Node::Element(e) = node {
                block(e, &mut out);
            }
        } else {
            run.push(node);
        }
    }
    flush(&mut run, &mut out);
    out
}

fn flush(run: &mut Vec<&Node>, out: &mut Vec<String>) {
    let text = inline_nodes(run.iter().copied());
    run.clear();
    if let Some(p) = paragraph(&text) {
        out.push(p);
    }
}

fn block(e: &Element, out: &mut Vec<String>) {
    match e.name.as_str() {
        h @ ("h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
            let level = usize::from(h.as_bytes()[1] - b'0');
            let mut text = one_line(&inline_nodes(e.children.iter()));
            // A heading that ends in `#` would lose it as a closing sequence.
            if text.ends_with('#') {
                let run = text.bytes().rev().take_while(|b| *b == b'#').count();
                text.insert(text.len() - run, '\\');
            }
            if !text.is_empty() {
                out.push(format!("{} {}", "#".repeat(level), text));
            }
        }
        "ul" | "ol" => {
            let start = e
                .attr("start")
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(1);
            let mut items = Vec::new();
            for (k, item) in e
                .children
                .iter()
                .filter(|n| !matches!(n, Node::Text(t) if t.trim().is_empty()))
                .enumerate()
            {
                let marker = if e.name == "ol" {
                    format!("{}. ", start + k)
                } else {
                    "* ".to_owned()
                };
                let content = match item {
                    Node::Element(li) if li.name == "li" => blocks(&li.children),
                    other => blocks(std::slice::from_ref(other)),
                };
                if let Some(text) = list_item(&marker, &content) {
                    items.push(text);
                }
            }
            if !items.is_empty() {
                out.push(items.join("\n"));
            }
        }
        "li" => {
            if let Some(text) = list_item("* ", &blocks(&e.children)) {
                out.push(text);
            }
        }
        "blockquote" => {
            let inner = blocks(&e.children).join("\n\n");
            if !inner.is_empty() {
                out.push(
                    inner
                        .lines()
                        .map(|l| {
                            if l.is_empty() {
                                ">".to_owned()
                            } else {
                                format!("> {l}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
        }
        "pre" => {
            let code = e.text();
            let code = code.trim_matches('\n');
            if !code.is_empty() {
                let fence = if code.contains("```") { "~~~" } else { "```" };
                out.push(format!("{fence}\n{code}\n{fence}"));
            }
        }
        "table" => {
            let mut rows = Vec::new();
            collect_rows(&e.children, &mut rows);
            let lines: Vec<String> = rows
                .iter()
                .map(|cells| {
                    let cells: Vec<String> = cells
                        .iter()
                        .map(|c| one_line(&inline_nodes(c.children.iter())).replace('|', "\\|"))
                        .collect();
                    format!("| {} |", cells.join(" | "))
                })
                .collect();
            if !lines.is_empty() {
                out.push(lines.join("\n"));
            }
        }
        "hr" => out.push("---".to_owned()),
        _ => {
            if e.children.iter().any(is_block) {
                out.extend(blocks(&e.children));
            } else if let Some(p) = paragraph(&inline_nodes(e.children.iter())) {
                out.push(p);
            }
        }
    }
}

fn collect_rows<'a>(nodes: &'a [Node], rows: &mut Vec<Vec<&'a Element>>) {
    for node in nodes {
        if let Node::Element(e) = node {
            if e.name == "tr" {
                rows.push(
                    e.children
                        .iter()
                        .filter_map(|n| match n {
                            Node::Element(c) if c.name == "td" || c.name == "th" => Some(c),
                            _ => None,
                        })
                        .collect(),
                );
            } else {
                collect_rows(&e.children, rows);
            }
        }
    }
}

fn list_item(marker: &str, content: &[String]) -> Option<String> {
    if content.is_empty() {
        return None;
    }
    let indent = " ".repeat(marker.chars().count());
    let body = content.join("\n\n");
    let mut out = String::new();
    for (i, line) in body.lines().enumerate() {
        if i == 0 {
            out.push_str(marker);
            out.push_str(line);
        } else {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(&indent);
                out.push_str(line);
            }
        }
    }
    Some(out)
}

/// Trims each line, drops empty lines, collapses runs of spaces, and escapes a line start
/// that would read as a heading, quote or list item.
fn paragraph(text: &str) -> Option<String> {
    let lines: Vec<String> = text
        .split('\n')
        .map(|l| escape_line_start(&collapse_spaces(l.trim())))
        .filter(|l| !l.is_empty())
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn one_line(text: &str) -> String {
    collapse_spaces(text.replace('\n', " ").trim_matches(' '))
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c == ' ' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

fn escape_line_start(line: &str) -> String {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let numbered = digits > 0 && matches!(line.as_bytes().get(digits), Some(b'.' | b')'));
    let marker = line.starts_with('#')
        || line.starts_with('>')
        || line.starts_with("- ")
        || line.starts_with("+ ")
        || line == "-"
        || line == "+";
    if marker {
        format!("\\{line}")
    } else if numbered {
        format!("{}\\{}", &line[..digits], &line[digits..])
    } else {
        line.to_owned()
    }
}

/// HTML collapses runs of these into one space.
fn html_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// Where inline markdown is being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inline {
    Text,
    /// Inside a link label. A link cannot nest in another, so an inner `<a>` or `<data>`
    /// keeps only its text, and every bracket is escaped so the label ends where it should.
    Label,
}

fn escape_text(text: &str, at: Inline) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if html_space(c) {
            if !space {
                out.push(' ');
            }
            space = true;
            continue;
        }
        space = false;
        match c {
            '\\' | '*' | '_' | '`' | '<' => {
                out.push('\\');
                out.push(c);
            }
            '[' | ']' if at == Inline::Label => {
                out.push('\\');
                out.push(c);
            }
            '[' if chars.peek() == Some(&'[') => out.push_str("\\["),
            ']' if chars.peek() == Some(&'(') => out.push_str("\\]"),
            c => out.push(c),
        }
    }
    out
}

fn inline_nodes<'a>(nodes: impl Iterator<Item = &'a Node>) -> String {
    inline_in(nodes, Inline::Text)
}

fn inline_in<'a>(nodes: impl Iterator<Item = &'a Node>, at: Inline) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&escape_text(t, at)),
            Node::Element(e) => inline_element(e, at, &mut out),
        }
    }
    out
}

fn wrap(marker: &str, inner: &str, out: &mut String) {
    let core = inner.trim_matches(' ');
    if core.is_empty() {
        out.push_str(inner);
        return;
    }
    if inner.starts_with(' ') {
        out.push(' ');
    }
    out.push_str(marker);
    out.push_str(core);
    out.push_str(marker);
    if inner.ends_with(' ') {
        out.push(' ');
    }
}

/// Percent-encodes what would end an href early or split it: whitespace, parentheses, and
/// `|`, which a table cell escapes.
fn encode_href(href: &str) -> String {
    let mut out = String::with_capacity(href.len());
    for c in href.chars() {
        if c.is_whitespace() || matches!(c, '(' | ')' | '|') {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn link(label: &str, href: &str, out: &mut String) {
    let label = label.replace('\n', " ");
    let piece = format!("[{}]({})", label.trim_matches(' '), encode_href(href));
    append(out, &piece);
}

/// Appends converted markdown. A `!` right before a link would make it an image, so it is
/// escaped.
fn append(out: &mut String, piece: &str) {
    if piece.starts_with('[') && out.ends_with('!') {
        let before = &out[..out.len() - 1];
        let backslashes = before.bytes().rev().take_while(|b| *b == b'\\').count();
        if backslashes % 2 == 0 {
            out.insert(out.len() - 1, '\\');
        }
    }
    out.push_str(piece);
}

fn inline_element(e: &Element, at: Inline, out: &mut String) {
    let inner = || inline_in(e.children.iter(), at);
    let label = || inline_in(e.children.iter(), Inline::Label);
    match e.name.as_str() {
        "br" => out.push('\n'),
        "strong" | "b" => wrap("**", &inner(), out),
        "em" | "i" | "cite" => wrap("*", &inner(), out),
        "code" => {
            let code = e.text();
            if !code.is_empty() {
                let tick = if code.contains('`') { "`` " } else { "`" };
                let close = if code.contains('`') { " ``" } else { "`" };
                out.push_str(tick);
                out.push_str(&code);
                out.push_str(close);
            }
        }
        "img" if at == Inline::Label => {
            out.push_str(&escape_text(e.attr("alt").unwrap_or(""), at));
        }
        "img" => {
            if let Some(src) = e.attr("src") {
                let alt = escape_text(e.attr("alt").unwrap_or(""), Inline::Label);
                out.push_str(&format!("![{alt}]({})", encode_href(src)));
            }
        }
        "a" | "data" if at == Inline::Label => append(out, &inner()),
        "a" => match e.attr("href").map(str::trim).filter(|h| !h.is_empty()) {
            Some(href) => link(&label(), href, out),
            None => append(out, &inner()),
        },
        "data" => match data_href(e) {
            Some(href) => link(&label(), &href, out),
            None => append(out, &inner()),
        },
        name if BLOCKS.contains(&name) => {
            out.push(' ');
            out.push_str(&inner());
            out.push(' ');
        }
        _ => append(out, &inner()),
    }
}

/// The `aquifer:` link for a `<data>` tag, when it has the attributes one needs.
fn data_href(e: &Element) -> Option<String> {
    let clean = |s: &str| (!s.is_empty() && !s.contains(['/', ' ', ')'])).then(|| s.to_owned());
    if e.has_class("resource-ref") {
        let id = clean(e.attr("data-content-id")?)?;
        let code = clean(e.attr("data-resource-code")?)?;
        let language = clean(e.attr("data-language").unwrap_or("und"))?;
        return Some(format!("aquifer:resource/{code}/{language}/{id}"));
    }
    if e.has_class("bible-ref") {
        let start = clean(e.attr("data-start-ref")?)?;
        let end = e
            .attr("data-end-ref")
            .and_then(clean)
            .unwrap_or_else(|| start.clone());
        return Some(if end == start {
            format!("aquifer:bible/{start}")
        } else {
            format!("aquifer:bible/{start}-{end}")
        });
    }
    None
}

/// The text of dialect markdown with its marks removed: headings, quote and list markers,
/// emphasis, link syntax (the label stays; a `[[href]]` keeps the href) and backslash
/// escapes. Code is kept as written, without its fences or backticks. Lines are kept.
pub fn plain_text(markdown: &str) -> String {
    let mut fence: Option<&str> = None;
    let mut out = Vec::new();
    for line in markdown.lines() {
        let bare = strip_block_marks(line);
        let opens = ["```", "~~~"].into_iter().find(|f| bare.starts_with(f));
        match (fence, opens) {
            (Some(open), Some(f)) if f == open => fence = None,
            (Some(_), _) => out.push(line.to_owned()),
            (None, Some(f)) => fence = Some(f),
            (None, None) => out.push(plain_line(bare)),
        }
    }
    out.join("\n")
}

fn strip_block_marks(line: &str) -> &str {
    let mut line = line.trim_start();
    loop {
        if let Some(rest) = line.strip_prefix('>') {
            line = rest.trim_start();
            continue;
        }
        break;
    }
    if let Some(rest) = line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")) {
        return rest;
    }
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        return &line[hashes + 1..];
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return &line[digits + 2..];
    }
    line
}

fn plain_line(line: &str) -> String {
    let mut links = scan(line).into_iter().peekable();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while let Some(c) = line[at..].chars().next() {
        while links.next_if(|f| f.link.span.start < at).is_some() {}
        if let Some(f) = links.next_if(|f| f.link.span.start == at) {
            match &f.link.label {
                Some(label) => out.push_str(&plain_line(label)),
                None => out.push_str(&f.link.href),
            }
            at = f.link.span.end;
            continue;
        }
        let rest = &line[at..];
        if c == '\\'
            && let Some(next) = rest[1..].chars().next()
            && next.is_ascii_punctuation()
        {
            out.push(next);
            at += 1 + next.len_utf8();
            continue;
        }
        if c == '*' {
            at += 1;
            continue;
        }
        if c == '`' {
            let ticks = rest.bytes().take_while(|b| *b == b'`').count();
            let marker = &rest[..ticks];
            if let Some(end) = rest[ticks..].find(marker) {
                let code = &rest[ticks..ticks + end];
                out.push_str(if ticks > 1 { code.trim() } else { code });
                at += ticks + end + ticks;
                continue;
            }
        }
        out.push(c);
        at += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_aquifer_tag_set() {
        let md = html_to_markdown(
            "<h2>Definition:</h2>\n<p>The <strong>apostles </strong>were <em>sent</em>.</p>\
             <ul>\n<li>One</li>\n<li><p>Two</p><p>more</p></li>\n</ul>\
             <blockquote>\n<p>From <strong>childhood</strong></p>\n</blockquote>\
             <ol start=\"3\"><li>three</li></ol>\
             <p>See <data class=\"resource-ref\" data-content-id=\"33408\" \
             data-resource-code=\"UWTranslationManual\" data-language=\"eng\">How</data> \
             and <data class=\"bible-ref\" data-start-ref=\"65001017\" data-end-ref=\"65001019\">Jude</data>.</p>",
        );
        assert_eq!(
            md,
            "## Definition:\n\n\
             The **apostles** were *sent*.\n\n\
             * One\n* Two\n\n  more\n\n\
             > From **childhood**\n\n\
             3. three\n\n\
             See [How](aquifer:resource/UWTranslationManual/eng/33408) and \
             [Jude](aquifer:bible/65001017-65001019)."
        );
    }

    #[test]
    fn text_that_looks_like_marks_is_escaped() {
        assert_eq!(
            html_to_markdown("<p># not a heading</p>"),
            "\\# not a heading"
        );
        assert_eq!(html_to_markdown("<p>1. not a list</p>"), "1\\. not a list");
        assert_eq!(html_to_markdown("<p>a*b_c</p>"), "a\\*b\\_c");
        assert_eq!(plain_text("1\\. a\\*b [x](y) **z**"), "1. a*b x z");
    }
}
