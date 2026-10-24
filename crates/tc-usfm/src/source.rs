//! The original-language side of a verse as tC4 reads it with usfm-js: which `\w` words are
//! words (the ones usfm-js makes verse objects of), and the text that belongs to a verse.

use crate::Error;
use crate::align::SourceWord;
use crate::book::{Segment, SegmentKind};
use crate::doc::{Document, Node, Span};

/// The `\w` words of an original-language verse text such as UGNT or UHB writes it, in
/// verse order, as usfm-js makes verse objects of them (the rules are `Phrases`'). `lemma`,
/// `strong` (or `strongs`) and `morph` are read with or without the `x-` prefix, and the
/// default attribute is the lemma. Occurrences are left 0, for the caller to count; other
/// attributes are kept in `extra` as written.
pub fn source_words(body: &str) -> Result<Vec<SourceWord>, Error> {
    let doc = Document::parse(body);
    let mut found = Vec::new();
    collect_source_words(doc.nodes(), &mut Phrases::default(), &mut found)?;
    Ok(found)
}

/// Append what `segment` holds before its `\v` or `\c` marker: the headings, paragraph
/// markers and milestones `tc-usfm` gives to the verse or chapter they lead into.
pub fn push_leading(segment: &Segment<'_>, out: &mut String) {
    match segment.kind() {
        SegmentKind::Verse(unit) => out.push_str(unit.leading()),
        SegmentKind::Chapter(_) => {
            let doc = Document::parse(segment.text());
            for node in doc
                .nodes()
                .iter()
                .take_while(|n| !matches!(n, Node::Marker(m) if m.is_chapter()))
            {
                node.write_to(out);
            }
        }
        SegmentKind::Header => {}
    }
}

/// Spans usfm-js keeps as text, not as verse objects, when they open outside every shown
/// span: its markers with an end marker and no `display` flag (`USFM_PROPERTIES` in
/// usfm-js 3.4.3). Notes, figures, sidebars, alternate and published numbers, and the
/// rarer reference and index spans.
const HIDDEN_SPANS: &[&str] = &[
    "ca", "cat", "ef", "esb", "ex", "f", "fa", "fdc", "fe", "fig", "fm", "fqa", "fv", "ior", "iqt",
    "ndx", "pro", "rq", "rt", "sis", "va", "vp", "x", "xdc", "xnt", "xop", "xot", "xt",
];

/// Spans usfm-js shows, with their words as verse objects: its markers with an end marker
/// and the `display` flag, `\w` aside. So are the `\k-s`, `\zaln-s` and `\qt-s` milestones.
const SHOWN_SPANS: &[&str] = &[
    "add", "bd", "bdit", "bk", "dc", "em", "it", "jmp", "k", "li", "lik", "litl", "liv", "liv1",
    "liv2", "liv3", "liv4", "liv5", "nd", "no", "ord", "pn", "png", "qac", "qs", "qt", "qt1",
    "qt2", "qt3", "qt4", "qt5", "rb", "sc", "sig", "sls", "sup", "tl", "wa", "wg", "wh", "wj",
    "zaln",
];

/// The `\+name` forms usfm-js shows. It knows no other nested marker.
const SHOWN_NESTED_SPANS: &[&str] = &[
    "add", "bd", "bdit", "bk", "dc", "em", "it", "k", "nd", "no", "ord", "pn", "png", "qt", "sc",
    "sig", "sls", "sup", "tl", "wj",
];

/// Which `\w` words usfm-js turns into verse objects, which are the words tC4's bootstrap
/// gives cards. usfm-js keeps a hidden span ([`HIDDEN_SPANS`]) that opens outside every
/// shown span as one piece of text up to its own end marker, across paragraphs and
/// headings, or up to the next `\v` or `\c`, where the verse text given here ends, so no
/// word in it counts. Opened inside a shown span, the same marker hides nothing. Any end
/// marker that is not the innermost shown span's also ends every shown span it passes, as
/// usfm-js's `endSpan` pops them.
#[derive(Default)]
struct Phrases<'a> {
    /// The hidden span that is open, and how many spans of its name are open in it.
    hidden: Option<(&'a str, usize)>,
    /// The shown spans that are open, innermost last: nested or not, and base name.
    shown: Vec<(bool, &'a str)>,
}

/// An end marker as usfm-js matches it against the spans that are open.
enum End<'a> {
    /// `\name*` or `\+name*`.
    Star { nested: bool, name: &'a str },
    /// `\name-e\*`, or a milestone that ends nothing it names, such as `\ts\*`.
    Milestone(&'a str),
    /// `\esbe`.
    Sidebar,
}

impl<'a> Phrases<'a> {
    fn counts_words(&self) -> bool {
        self.hidden.is_none()
    }

    fn open(&mut self, nested: bool, name: &'a str) {
        if let Some((hidden, depth)) = &mut self.hidden {
            if !nested && name == *hidden {
                *depth += 1;
            }
            return;
        }
        let milestone = name.strip_suffix("-s").filter(|_| !nested);
        let shown = match milestone {
            Some(base) => matches!(
                base,
                "k" | "zaln" | "qt" | "qt1" | "qt2" | "qt3" | "qt4" | "qt5"
            ),
            None if nested => SHOWN_NESTED_SPANS.contains(&name),
            None => SHOWN_SPANS.contains(&name),
        };
        if shown {
            self.shown.push((nested, milestone.unwrap_or(name)));
        } else if !nested && self.shown.is_empty() && HIDDEN_SPANS.contains(&name) {
            self.hidden = Some((name, 0));
        }
    }

    fn end(&mut self, end: End<'a>) {
        if let Some((hidden, depth)) = &mut self.hidden {
            let closes = match end {
                // A sidebar ends only at `\esbe`, never at `\esb*`.
                End::Star { nested, name } => !nested && name == *hidden && name != "esb",
                End::Sidebar => *hidden == "esb",
                End::Milestone(_) => false,
            };
            if closes {
                match depth.checked_sub(1) {
                    Some(fewer) => *depth = fewer,
                    None => self.hidden = None,
                }
            }
            return;
        }
        let matches = |&(nested, base): &(bool, &str)| match end {
            End::Star { nested: n, name } => nested == n && base == name,
            End::Milestone(name) => !nested && base == name,
            End::Sidebar => false,
        };
        let keep = self.shown.iter().rposition(matches).unwrap_or(0);
        self.shown.truncate(keep);
    }
}

fn collect_source_words<'a>(
    nodes: &'a [Node<'a>],
    phrases: &mut Phrases<'a>,
    out: &mut Vec<SourceWord>,
) -> Result<(), Error> {
    for node in nodes {
        match node {
            Node::Span(span) if span.name() == "w" && !span.open.is_nested() => {
                if phrases.counts_words() {
                    out.push(source_word(span)?);
                }
            }
            Node::Span(span) => {
                phrases.open(span.open.is_nested(), span.name());
                collect_source_words(&span.children, phrases, out)?;
                if let Some(close) = &span.close {
                    phrases.end(End::Star {
                        nested: close.is_nested(),
                        name: close.name(),
                    });
                }
            }
            Node::Marker(marker) => marker_event(
                phrases,
                marker.is_nested(),
                marker.is_closing(),
                marker.name(),
            ),
            Node::Milestone(milestone) => {
                marker_event(
                    phrases,
                    milestone.marker.is_nested(),
                    false,
                    milestone.name(),
                );
            }
            Node::Text(_) | Node::Bom(_) | Node::Stray(_) => {}
        }
    }
    Ok(())
}

/// A marker or milestone outside a span: an opening marker, an end marker, or one that
/// neither opens nor ends anything (a paragraph, a heading, an unknown marker).
fn marker_event<'a>(phrases: &mut Phrases<'a>, nested: bool, closing: bool, name: &'a str) {
    if closing {
        phrases.end(End::Star { nested, name });
    } else if nested {
        phrases.open(nested, name);
    } else if name == "esbe" {
        phrases.end(End::Sidebar);
    } else if matches!(name, "ts" | "ts-s") {
        // usfm-js reads a chunk milestone as an end marker that matches no span.
        phrases.end(End::Milestone(name));
    } else if let Some(base) = name.strip_suffix("-e") {
        phrases.end(End::Milestone(base));
    } else {
        phrases.open(nested, name);
    }
}

fn source_word(span: &Span<'_>) -> Result<SourceWord, Error> {
    let mut content = String::new();
    for child in &span.children {
        if let Node::Text(text) = child {
            content.push_str(text);
        }
    }
    let mut word = SourceWord {
        content,
        strong: None,
        lemma: None,
        morph: None,
        occurrence: 0,
        occurrences: 0,
        extra: Vec::new(),
    };
    if let Some(attributes) = &span.attributes {
        for attribute in attributes.parse()? {
            let value = attribute.value.to_owned();
            match attribute.key {
                None | Some("lemma" | "x-lemma") => word.lemma = Some(value),
                Some("strong" | "x-strong" | "strongs" | "x-strongs") => {
                    word.strong = Some(value);
                }
                Some("morph" | "x-morph") => word.morph = Some(value),
                Some("occurrence" | "x-occurrence" | "occurrences" | "x-occurrences") => {}
                Some(key) => word.extra.push((key.to_owned(), value)),
            }
        }
    }
    Ok(word)
}

/// An aligned verse read as tC4's suggestion benches read en_ult (`verseFromObjects` in
/// `test/helpers/ult-corpus.ts`): one source word per `\zaln-s` in the order they open, and
/// one link per aligned `\w` from the chain of milestones open above it to that one word.
#[derive(Debug, Default)]
pub struct AlignedWalk {
    /// The verse's text without the alignment markup.
    pub plain: String,
    /// One per `\zaln-s`, in the order they open; a missing occurrence reads as 1.
    pub sources: Vec<SourceWord>,
    pub links: Vec<WalkLink>,
    open: Vec<usize>,
}

/// One aligned word of an [`AlignedWalk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkLink {
    /// The positions in [`AlignedWalk::sources`] of the milestones open above the word.
    pub sources: Vec<usize>,
    pub word: String,
    /// The word's written occurrence (1 when it has none).
    pub occurrence: u32,
}

/// The [`AlignedWalk`] of an aligned verse `body`.
pub fn aligned_walk(body: &str) -> Result<AlignedWalk, Error> {
    let doc = Document::parse(body);
    let mut walk = AlignedWalk::default();
    walk.nodes(doc.nodes())?;
    Ok(walk)
}

impl AlignedWalk {
    fn nodes(&mut self, nodes: &[Node<'_>]) -> Result<(), Error> {
        let number = |v: Option<&str>| v.and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
        for node in nodes {
            match node {
                Node::Text(text) => self.plain.push_str(text),
                Node::Milestone(m) if m.name() == "zaln-s" => {
                    let get = |key: &str| -> Result<Option<String>, Error> {
                        Ok(match &m.attributes {
                            Some(a) => a.get(key)?.map(str::to_owned),
                            None => None,
                        })
                    };
                    let position = self.sources.len();
                    self.sources.push(SourceWord {
                        content: get("x-content")?.unwrap_or_default(),
                        occurrence: number(get("x-occurrence")?.as_deref()),
                        occurrences: number(get("x-occurrences")?.as_deref()),
                        strong: get("x-strong")?,
                        lemma: get("x-lemma")?,
                        morph: get("x-morph")?,
                        extra: Vec::new(),
                    });
                    self.open.push(position);
                }
                Node::Milestone(m) if m.name() == "zaln-e" => {
                    self.open.pop();
                }
                Node::Span(span) if span.name() == "w" => {
                    let mut text = String::new();
                    for child in &span.children {
                        if let Node::Text(t) = child {
                            text.push_str(t);
                        }
                    }
                    self.plain.push_str(&text);
                    if !self.open.is_empty() {
                        let occurrence = match &span.attributes {
                            Some(a) => number(a.get("x-occurrence")?),
                            None => 1,
                        };
                        self.links.push(WalkLink {
                            sources: self.open.clone(),
                            word: text,
                            occurrence,
                        });
                    }
                }
                // Notes carry no text in usfm-js's verse objects; other character spans
                // are walked for their words.
                Node::Span(span) if !is_note(span.name()) => self.nodes(&span.children)?,
                _ => {}
            }
        }
        Ok(())
    }
}

fn is_note(name: &str) -> bool {
    use crate::markers::{MarkerClass, classify};
    classify(name) == MarkerClass::Note || matches!(name, "fig" | "ca" | "va" | "vp" | "cat" | "rq")
}
