//! Layer 2, the DOM (APP-DESIGN 7.3): the snapshot of parity/app/observe.mjs `domSnapshot`,
//! normalized on each side by the listed rules and nothing else, then compared line by line.
//!
//! A snapshot line is an element, `<tag name="value" ...[ style=@key][ .prop ...]>`, or a text
//! run, `"text"`, each indented two spaces per depth under `#root`. The rules, in order:
//!
//! 1. expand each style key into its text;
//! 2. select the lines compared: `<html>`, `#root`'s subtree and any element outside it, or,
//!    for a DOM-first capture, the subtree of the scope element alone;
//! 3. on the Rust side, rename project folders to tC4's repo paths in attribute values;
//! 4. reduce same-origin asset paths (`src`, `href`, `poster`, `url(/...)`) to the file name
//!    without a Vite hash;
//! 5. move each mask's element and its subtree out of the lines into the mask's part;
//! 6. renumber generated ids (`tc-f`, `tc-g`, `tc-dlg`, `tc-cfm`, `tc-rg`) per prefix by
//!    first appearance, in the lines and in each part on its own, so a masked difference does
//!    not shift the ids after it.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::inputs::{Scope, TargetSpec, Which};

/// A DOM file of either side, or a step of a behaviour recording.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    pub html: String,
    pub outside: Vec<String>,
    pub root: Vec<String>,
    pub styles: BTreeMap<String, String>,
}

/// Why a comparison could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomError {
    /// A line that is not in the snapshot format, or a style key with no text.
    Format(String),
    /// The scope matches nothing on a side.
    Scope(String),
    /// A mask's element is not on a side.
    Mask(String),
}

impl DomError {
    pub fn message(&self) -> &str {
        match self {
            DomError::Format(m) | DomError::Scope(m) | DomError::Mask(m) => m,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Element {
    depth: usize,
    tag: String,
    attrs: Vec<(String, String)>,
    style: Option<String>,
    props: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Element(Element),
    Text { depth: usize, text: String },
}

impl Node {
    fn depth(&self) -> usize {
        match self {
            Node::Element(e) => e.depth,
            Node::Text { depth, .. } => *depth,
        }
    }

    fn set_depth(&mut self, to: usize) {
        match self {
            Node::Element(e) => e.depth = to,
            Node::Text { depth, .. } => *depth = to,
        }
    }

    fn element(&self) -> Option<&Element> {
        match self {
            Node::Element(e) => Some(e),
            Node::Text { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Html,
    Root,
    Outside,
}

#[derive(Clone, Debug)]
struct Part {
    group: Group,
    node: Node,
}

fn json(text: &str) -> String {
    // A string always serializes.
    serde_json::to_string(text).unwrap_or_default()
}

fn render(part: &Part) -> String {
    let line = match &part.node {
        Node::Text { depth, text } => format!("{}\"{text}\"", "  ".repeat(*depth)),
        Node::Element(e) => {
            let attrs: Vec<String> = e
                .attrs
                .iter()
                .map(|(n, v)| format!("{n}={}", json(v)))
                .collect();
            let style = e
                .style
                .as_deref()
                .map(|s| format!(" style={}", json(s)))
                .unwrap_or_default();
            let props = if e.props.is_empty() {
                String::new()
            } else {
                format!(" {}", e.props.join(" "))
            };
            format!(
                "{}<{} {}{style}{props}>",
                "  ".repeat(e.depth),
                e.tag,
                attrs.join(" ")
            )
        }
    };
    match part.group {
        Group::Root => line,
        Group::Html => format!("html: {line}"),
        Group::Outside => format!("outside: {line}"),
    }
}

/// The end of the JSON string that starts at `s[0]` (a `"`), past its closing quote.
fn json_string_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn parse_line(line: &str, styles: &BTreeMap<String, String>) -> Result<Node, DomError> {
    let bad = |why: &str| DomError::Format(format!("{why}: {}", cut(line, 120)));
    let body = line.trim_start_matches(' ');
    let pad = line.len() - body.len();
    if !pad.is_multiple_of(2) {
        return Err(bad("an odd indentation"));
    }
    let depth = pad / 2;
    if let Some(text) = body.strip_prefix('"') {
        let text = text
            .strip_suffix('"')
            .ok_or_else(|| bad("a text run without its closing quote"))?;
        return Ok(Node::Text {
            depth,
            text: text.to_owned(),
        });
    }
    let inner = body
        .strip_prefix('<')
        .and_then(|b| b.strip_suffix('>'))
        .ok_or_else(|| bad("neither an element nor a text run"))?;
    let (tag, mut rest) = inner
        .split_once(' ')
        .ok_or_else(|| bad("an element without its space"))?;
    let mut element = Element {
        depth,
        tag: tag.to_owned(),
        attrs: Vec::new(),
        style: None,
        props: Vec::new(),
    };
    loop {
        rest = rest.trim_start_matches(' ');
        if rest.is_empty() {
            break;
        }
        if let Some(after) = rest.strip_prefix("style=@") {
            let key = after.get(..12).ok_or_else(|| bad("a short style key"))?;
            let text = styles
                .get(key)
                .ok_or_else(|| bad(&format!("style @{key} is not in the style table")))?;
            element.style = Some(text.clone());
            rest = &after[12..];
        } else if rest.starts_with('.') || rest.starts_with('#') {
            let end = match rest.find('=') {
                Some(eq) if rest[..eq].find(' ').is_none() && rest[eq + 1..].starts_with('"') => {
                    eq + 1
                        + json_string_end(&rest[eq + 1..])
                            .ok_or_else(|| bad("an unterminated value"))?
                }
                _ => rest.find(' ').unwrap_or(rest.len()),
            };
            element.props.push(rest[..end].to_owned());
            rest = &rest[end..];
        } else {
            let eq = rest
                .find('=')
                .ok_or_else(|| bad("an attribute without a value"))?;
            let name = &rest[..eq];
            let quoted = &rest[eq + 1..];
            let end = json_string_end(quoted)
                .filter(|_| quoted.starts_with('"'))
                .ok_or_else(|| bad("an attribute value that is not a JSON string"))?;
            let value: String =
                serde_json::from_str(&quoted[..end]).map_err(|e| bad(&e.to_string()))?;
            element.attrs.push((name.to_owned(), value));
            rest = &quoted[end..];
        }
    }
    Ok(Node::Element(element))
}

/// One side of a comparison: its snapshot, and the project folders to rename (the Rust side's
/// folder -> repo path pairs, longest first; none for tC4, whose ids are the reference).
#[derive(Clone, Copy, Debug)]
pub struct Side<'a> {
    pub name: &'a str,
    pub snapshot: &'a Snapshot,
    pub rename: &'a [(String, String)],
}

/// A mask as the DOM layer applies it: its id and its element on each side, with `<OBS>` and
/// `<BIBLE>` resolved to tC4's repo paths.
#[derive(Clone, Debug)]
pub struct DomMask {
    pub id: String,
    pub tc4: TargetSpec,
    pub rust: TargetSpec,
    /// The rebuild has no such element yet (`Mask::rust_absent`).
    pub rust_absent: bool,
}

/// One side, normalized: the lines compared and each mask's part (None: not on this side).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub lines: Vec<String>,
    pub parts: Vec<Option<Vec<String>>>,
}

fn expand(snapshot: &Snapshot) -> Result<Vec<Part>, DomError> {
    let mut parts = vec![Part {
        group: Group::Html,
        node: parse_line(&snapshot.html, &snapshot.styles)?,
    }];
    for line in &snapshot.root {
        parts.push(Part {
            group: Group::Root,
            node: parse_line(line, &snapshot.styles)?,
        });
    }
    for line in &snapshot.outside {
        parts.push(Part {
            group: Group::Outside,
            node: parse_line(line, &snapshot.styles)?,
        });
    }
    Ok(parts)
}

/// The subtree at `start` (its node and every following node deeper than it), re-indented so
/// it starts at depth 0, and the index after it.
fn subtree(parts: &[Part], start: usize) -> (Vec<Part>, usize) {
    let depth = parts[start].node.depth();
    let mut end = start + 1;
    while end < parts.len() && parts[end].group == Group::Root && parts[end].node.depth() > depth {
        end += 1;
    }
    let taken = parts[start..end]
        .iter()
        .map(|p| {
            let mut p = p.clone();
            let d = p.node.depth() - depth;
            p.node.set_depth(d);
            p
        })
        .collect();
    (taken, end)
}

fn has_attr(e: &Element, attr: &str, value: &str) -> bool {
    e.attrs.iter().any(|(n, v)| n == attr && v == value)
}

fn select(parts: Vec<Part>, scope: Option<&Scope>, side: &str) -> Result<Vec<Part>, DomError> {
    let Some(scope) = scope else { return Ok(parts) };
    let matches = parts
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.group == Group::Root
                && p.node
                    .element()
                    .is_some_and(|e| has_attr(e, &scope.attr, &scope.value))
        })
        .map(|(i, _)| i);
    let at = match scope.which {
        Which::First => matches.min(),
        Which::Last => matches.max(),
    }
    .ok_or_else(|| {
        DomError::Scope(format!(
            "scope {}=\"{}\" matches nothing in the {side} DOM",
            scope.attr, scope.value
        ))
    })?;
    Ok(subtree(&parts, at).0)
}

fn rename(parts: &mut [Part], pairs: &[(String, String)]) {
    if pairs.is_empty() {
        return;
    }
    for part in parts {
        let Node::Element(e) = &mut part.node else {
            continue;
        };
        for (_, value) in &mut e.attrs {
            for (folder, repo) in pairs {
                let renamed = if value == folder {
                    Some(repo.clone())
                } else {
                    value
                        .strip_suffix(folder.as_str())
                        .filter(|head| head.ends_with(['-', '/']))
                        .map(|head| format!("{head}{repo}"))
                };
                if let Some(renamed) = renamed {
                    *value = renamed;
                    break;
                }
            }
        }
    }
}

static VITE_HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-[A-Za-z0-9_-]{8}(\.[A-Za-z0-9]+)$").unwrap());
static STYLE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"url\((['"]?)(/[^'")]*)(['"]?)\)"#).unwrap());
static GENERATED_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\btc-(f|g|dlg|cfm|rg)(\d+)").unwrap());

/// `/a/b/name-AbCd1234.png?x` -> `name.png?x`.
fn file_name(path: &str) -> String {
    let split = path.find(['?', '#']).unwrap_or(path.len());
    let (path, suffix) = path.split_at(split);
    let last = path.rsplit('/').next().unwrap_or(path);
    format!("{}{suffix}", VITE_HASH.replace(last, "$1"))
}

fn assets(parts: &mut [Part]) {
    for part in parts {
        let Node::Element(e) = &mut part.node else {
            continue;
        };
        for (name, value) in &mut e.attrs {
            if matches!(name.as_str(), "src" | "href" | "poster") && value.starts_with('/') {
                *value = file_name(value);
            }
        }
        if let Some(style) = &mut e.style
            && STYLE_URL.is_match(style)
        {
            *style = STYLE_URL
                .replace_all(style, |c: &regex::Captures<'_>| {
                    format!("url({}{}{})", &c[1], file_name(&c[2]), &c[3])
                })
                .into_owned();
        }
    }
}

fn renumber(parts: &mut [Part]) {
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut next: BTreeMap<String, usize> = BTreeMap::new();
    for part in parts {
        let Node::Element(e) = &mut part.node else {
            continue;
        };
        for (_, value) in &mut e.attrs {
            if !GENERATED_ID.is_match(value) {
                continue;
            }
            *value = GENERATED_ID
                .replace_all(value, |c: &regex::Captures<'_>| {
                    let key = (c[1].to_owned(), c[2].to_owned());
                    let k = *seen.entry(key).or_insert_with(|| {
                        let n = next.entry(c[1].to_owned()).or_insert(0);
                        *n += 1;
                        *n
                    });
                    format!("tc-{}#{k}", &c[1])
                })
                .into_owned();
        }
    }
}

/// The first text run among the element's own children, as the snapshot lists it.
fn first_text(parts: &[Part], at: usize) -> Option<&str> {
    let depth = parts[at].node.depth();
    parts[at + 1..]
        .iter()
        .take_while(|p| p.group == Group::Root && p.node.depth() > depth)
        .find_map(|p| match &p.node {
            Node::Text { depth: d, text } if *d == depth + 1 => Some(text.as_str()),
            _ => None,
        })
}

fn find(parts: &[Part], target: &TargetSpec) -> Option<usize> {
    parts.iter().enumerate().position(|(i, p)| {
        p.group == Group::Root
            && match (&p.node, target) {
                (Node::Element(e), TargetSpec::Testid(id)) => has_attr(e, "data-testid", id),
                (Node::Element(_), TargetSpec::Text(text)) => {
                    first_text(parts, i).is_some_and(|t| t.starts_with(text.as_str()))
                }
                (Node::Text { .. }, _) => false,
            }
    })
}

/// Normalizes one side: rules 1 to 6 of the module's list. A mask whose element is not on this
/// side has no part.
pub fn prepare(
    side: Side<'_>,
    scope: Option<&Scope>,
    masks: &[(String, TargetSpec)],
) -> Result<Prepared, DomError> {
    let mut parts = select(expand(side.snapshot)?, scope, side.name)?;
    rename(&mut parts, side.rename);
    assets(&mut parts);
    let mut taken = Vec::new();
    for (_, target) in masks {
        match find(&parts, target) {
            Some(at) => {
                let (mut part, end) = subtree(&parts, at);
                parts.drain(at..end);
                renumber(&mut part);
                taken.push(Some(part.iter().map(render).collect()));
            }
            None => taken.push(None),
        }
    }
    renumber(&mut parts);
    Ok(Prepared {
        lines: parts.iter().map(render).collect(),
        parts: taken,
    })
}

/// What the DOM layer found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomResult {
    /// The lines outside the masks are equal, and so is every mask's part.
    pub same: bool,
    /// The masks whose parts differ while every other line is equal.
    pub declared: Vec<String>,
    /// The first five lines that differ outside the masks.
    pub first_diffs: Vec<String>,
}

pub fn cut(text: &str, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

/// Index-wise differences of two line lists, at most `limit`.
pub fn line_diffs(tc4: &[String], rust: &[String], limit: usize) -> Vec<String> {
    let none = "(none)".to_owned();
    (0..tc4.len().max(rust.len()))
        .filter(|&i| tc4.get(i) != rust.get(i))
        .take(limit)
        .map(|i| {
            format!(
                "line {}: tC4 {} | rust {}",
                i + 1,
                cut(tc4.get(i).unwrap_or(&none), 300),
                cut(rust.get(i).unwrap_or(&none), 300)
            )
        })
        .collect()
}

/// Compares the two sides. A mask must be found on both sides, except under a scope, where a
/// mask found on neither side lies outside the part compared, and except a mask whose element
/// the rebuild does not draw yet, which must then be found on tC4's side only.
pub fn compare(
    tc4: Side<'_>,
    rust: Side<'_>,
    scope: Option<&Scope>,
    masks: &[DomMask],
    screen: &str,
) -> Result<DomResult, DomError> {
    let a = prepare(
        tc4,
        scope,
        &masks
            .iter()
            .map(|m| (m.id.clone(), m.tc4.clone()))
            .collect::<Vec<_>>(),
    )?;
    let b = prepare(
        rust,
        scope,
        &masks
            .iter()
            .map(|m| (m.id.clone(), m.rust.clone()))
            .collect::<Vec<_>>(),
    )?;
    let mut declared = Vec::new();
    let mut parts_equal = true;
    for (i, mask) in masks.iter().enumerate() {
        match (&a.parts[i], &b.parts[i]) {
            (None, None) if scope.is_some() => {}
            (Some(_), None) if mask.rust_absent => parts_equal = false,
            (Some(_), Some(_)) if mask.rust_absent => {
                return Err(DomError::Mask(format!(
                    "mask {}: the {} DOM of {screen} has its element {:?} now: remove its `rustAbsent`",
                    mask.id, rust.name, mask.rust
                )));
            }
            (Some(x), Some(y)) => {
                if x != y {
                    parts_equal = false;
                    declared.push(mask.id.clone());
                }
            }
            (found_tc4, _) => {
                let (missing, target) = if found_tc4.is_none() {
                    (tc4.name, &mask.tc4)
                } else {
                    (rust.name, &mask.rust)
                };
                return Err(DomError::Mask(format!(
                    "mask {}: its element {target:?} is not in the {missing} DOM of {screen}",
                    mask.id
                )));
            }
        }
    }
    let first_diffs = line_diffs(&a.lines, &b.lines, 5);
    let rest_equal = first_diffs.is_empty();
    Ok(DomResult {
        same: rest_equal && parts_equal,
        declared: if rest_equal { declared } else { Vec::new() },
        first_diffs,
    })
}
