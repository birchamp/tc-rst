//! The two sides of an alignment: the original-language verse and the target verse.
//!
//! A word's identity on either side is its text plus its occurrence: the 1-based position
//! among words with the same text in the verse (tC4 invariant I-2). Both sides number
//! occurrences the way tC4 does, so a record made by tC4 names the same words here.

use crate::Error;
use std::collections::HashMap;
use std::fmt;
use tc_core::Md5;
use tc_core::VerseSpan;
use tc_usfm::{Book, Document, Node, Segment, SegmentKind, SourceWord, Span, TargetWord};

/// The original-language words of one verse, in verse order, numbered by occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceVerse {
    words: Vec<SourceWord>,
}

impl SourceVerse {
    /// The original-language words of a verse or span of `book` (UGNT, UHB), read from
    /// the text tC4 reads for a verse: from after its `\v` marker to the next `\v` or
    /// `\c` marker (usfm-js's verse objects). That is the unit's body and what the next
    /// segment holds before its own `\v` or `\c` marker: `tc-usfm` gives a heading that
    /// stands before the next `\v`, with its words, to the next verse (an acrostic's
    /// `\qa`, a mid-verse `\s5`), and one that stands before the next `\c` to that
    /// chapter, and tC4 counts those words in this verse.
    ///
    /// A span is its member verses in order, numbered across the span
    /// ([`SourceVerse::span`]). A member verse the book does not have adds no words, as
    /// tC4's `verseObjectsFor` finds none for it.
    pub fn from_book(book: &Book<'_>, chapter: u16, verses: VerseSpan) -> Result<Self, Error> {
        let segments = book.segments();
        let mut parts = Vec::new();
        for verse in verses.verses() {
            let found = segments.iter().enumerate().find_map(|(at, s)| {
                s.as_verse()
                    .filter(|u| {
                        u.chapter() == Some(chapter) && u.verses() == Some(VerseSpan::single(verse))
                    })
                    .map(|u| (at, u))
            });
            let Some((at, unit)) = found else {
                continue;
            };
            let mut text = unit.body().to_owned();
            if let Some(next) = segments.get(at + 1) {
                push_leading(next, &mut text);
            }
            parts.push(Self::from_usfm(&text)?);
        }
        Ok(Self::span(parts))
    }

    /// The words of an original-language verse text such as UGNT or UHB writes it:
    /// `\w Παῦλος|lemma="Παῦλος" strong="G39720" x-morph="Gr,N,,,,,NMS,"\w*`.
    ///
    /// The words are the `\w` words that usfm-js, which tC4 reads the original with, makes
    /// verse objects of, so the bootstrap gives a card to each word tC4 gives one. They
    /// count at any depth of character markup and after any heading, title or other
    /// paragraph-level marker inside the verse (a mid-verse `\s5`, a Psalm's `\d`, an
    /// acrostic's `\qa`). The words of a note, figure, sidebar or other span usfm-js keeps
    /// as text do not count: such a span runs to its own end marker, across paragraphs, or
    /// to the end of the verse. Opened inside a span usfm-js shows, such as `\add` or
    /// `\k-s`, it hides nothing, and tC4 counts its words. `lemma`, `strong` (or `strongs`)
    /// and `morph` are read with or without the `x-` prefix, and the default attribute is
    /// the lemma.
    /// Written occurrence attributes are ignored: occurrences are counted by text across
    /// the verse, as word-aligner's `getOrderedVerseObjects` counts them for tC4's
    /// bootstrap. Other attributes are kept in [`SourceWord::extra`] as written.
    pub fn from_usfm(body: &str) -> Result<Self, Error> {
        let doc = Document::parse(body);
        let mut found = Vec::new();
        collect_source_words(doc.nodes(), &mut Phrases::default(), &mut found)?;
        Ok(Self::new(found))
    }

    /// Words given in verse order. Their occurrences are renumbered by text.
    pub fn new(mut words: Vec<SourceWord>) -> Self {
        let counted = number(words.iter().map(|w| w.content.as_str()));
        for (word, (occurrence, occurrences)) in words.iter_mut().zip(counted) {
            word.occurrence = occurrence;
            word.occurrences = occurrences;
        }
        Self { words }
    }

    /// The source of a verse span such as JON 2:9-10: its member verses' words in order,
    /// numbered by occurrence across the whole span, as tC4 aligns a span as one text.
    pub fn span(parts: impl IntoIterator<Item = SourceVerse>) -> Self {
        Self::new(parts.into_iter().flat_map(|p| p.words).collect())
    }

    pub fn words(&self) -> &[SourceWord] {
        &self.words
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

/// Append what `segment` holds before its `\v` or `\c` marker: the headings, paragraph
/// markers and milestones `tc-usfm` gives to the verse or chapter they lead into.
fn push_leading(segment: &Segment<'_>, out: &mut String) {
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
        for attribute in attributes.parse().map_err(Error::Usfm)? {
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

/// The target-language verse an alignment is made against: its plain text, the text's
/// hash, and its words numbered by occurrence.
///
/// The text is the verse's plain text as STORAGE.md §7.7 defines it for alignment hashes
/// (`plain_text_v1` of the verse content, tC4's `verseTextMd5` input). Both the words and
/// the hash come from that one text, exactly as tC4's aligner tokenizes and hashes its
/// `targetText`, so the words a record names and the hash it carries always agree.
#[derive(Clone, PartialEq, Eq)]
pub struct TargetVerse {
    text: String,
    md5: Md5,
    words: Vec<TargetWord>,
    /// Each word text's places in the verse, by occurrence, so that a word's place is
    /// found without a scan: the verse-order sort runs after every edit.
    places: HashMap<String, Vec<usize>>,
}

impl TargetVerse {
    pub fn new(text: &str) -> Self {
        let words: Vec<TargetWord> = tc_usfm::words::words(text)
            .into_iter()
            .map(|w| TargetWord {
                text: w.text.to_owned(),
                occurrence: w.occurrence,
                occurrences: w.occurrences,
            })
            .collect();
        let mut places: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, word) in words.iter().enumerate() {
            places.entry(word.text.clone()).or_default().push(at);
        }
        Self {
            text: text.to_owned(),
            md5: Md5::of(text.as_bytes()),
            words,
            places,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn md5(&self) -> Md5 {
        self.md5
    }

    /// Every word of the verse in order.
    pub fn words(&self) -> &[TargetWord] {
        &self.words
    }

    /// Where `word` stands in the verse, by text and occurrence.
    pub fn position(&self, word: &TargetWord) -> Option<usize> {
        let index = usize::try_from(word.occurrence.checked_sub(1)?).ok()?;
        self.places.get(word.text.as_str())?.get(index).copied()
    }

    /// Put `words` in the order they stand in this verse, by text and occurrence, as
    /// tC4's `inVerseOrder` does (#734): a repeated word by its occurrence, and a word the
    /// verse does not have after the others, in the order it was in. Right-to-left text
    /// is in reading order here too: the order is the text's, not the screen's.
    pub fn sort_in_verse_order(&self, words: &mut [TargetWord]) {
        words.sort_by_cached_key(|w| self.position(w).unwrap_or(usize::MAX));
    }

    /// The verse's words in the order tC4's bootstrap banks them: grouped by occurrence
    /// (every first occurrence, then every second, ...), in verse order within a group.
    ///
    /// word-aligner's `unmerge`, which tC4 calls to start an unaligned verse, files each
    /// unaligned word under an empty alignment keyed by its occurrence, then concatenates
    /// those alignments' words, so "the cat saw the dog" banks as `the cat saw dog the`.
    /// [`AlignmentValue::bootstrap`](crate::AlignmentValue::bootstrap) keeps this order so
    /// that it equals tC4's record; the aligner shows the bank in verse order.
    pub fn bank_order(&self) -> Vec<TargetWord> {
        let mut words = self.words.clone();
        words.sort_by_key(|w| w.occurrence);
        words
    }
}

// `places` is left out: it is only an index of `words`, and a hash map prints its entries
// in a different order in each value, so two equal verses would print differently.
impl fmt::Debug for TargetVerse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TargetVerse")
            .field("text", &self.text)
            .field("md5", &self.md5)
            .field("words", &self.words)
            .finish_non_exhaustive()
    }
}

/// Identity of a target word inside one verse: its text plus its occurrence.
pub(crate) fn same_word(a: &TargetWord, b: &TargetWord) -> bool {
    a.text == b.text && a.occurrence == b.occurrence
}

/// How the Align workspace tells a repeated word apart (tC4 #735, as tC3's
/// `WordOccurrence` did, `src/views/Align.jsx:37-48` at d310bf3): a word that occurs more
/// than once in its verse shows its occurrence number after it, raised, and is named
/// "word n"; a word that occurs once shows no number.
pub trait Occurrence {
    /// The number shown after the word, if the verse has the word more than once.
    fn shown_occurrence(&self) -> Option<u32>;

    /// The word's accessible name with its number, such as "de 3", if the verse has the
    /// word more than once. A word that occurs once is named by its text alone.
    fn occurrence_name(&self) -> Option<String>;
}

impl Occurrence for TargetWord {
    fn shown_occurrence(&self) -> Option<u32> {
        (self.occurrences > 1).then_some(self.occurrence)
    }

    fn occurrence_name(&self) -> Option<String> {
        let n = self.shown_occurrence()?;
        Some(format!("{} {n}", self.text))
    }
}

impl Occurrence for SourceWord {
    fn shown_occurrence(&self) -> Option<u32> {
        (self.occurrences > 1).then_some(self.occurrence)
    }

    fn occurrence_name(&self) -> Option<String> {
        let n = self.shown_occurrence()?;
        Some(format!("{} {n}", self.content))
    }
}

/// A target word's identity inside one verse, as a key: its text and its occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WordId {
    pub text: String,
    pub occurrence: u32,
}

impl WordId {
    pub fn of(word: &TargetWord) -> Self {
        Self {
            text: word.text.clone(),
            occurrence: word.occurrence,
        }
    }
}

/// Occurrence numbering by exact text: each word's 1-based index among equal texts, and
/// the count of equal texts.
fn number<'a>(texts: impl Iterator<Item = &'a str> + Clone) -> Vec<(u32, u32)> {
    let mut totals: HashMap<&str, u32> = HashMap::new();
    for text in texts.clone() {
        *totals.entry(text).or_default() += 1;
    }
    let mut seen: HashMap<&str, u32> = HashMap::new();
    texts
        .map(|text| {
            let n = seen.entry(text).or_default();
            *n += 1;
            (*n, totals.get(text).copied().unwrap_or(0))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_words_are_numbered_by_text_across_the_verse() {
        let body = "\\w καὶ|lemma=\"καί\" strong=\"G25320\" x-morph=\"Gr,CC,,,,,,,,\"\\w* \\f + \\ft \\w x|lemma=\"x\"\\w*\\f* \\add \\w καὶ|lemma=\"καί\" x-tw=\"rc://x\"\\w*\\add*";
        let verse = SourceVerse::from_usfm(body).unwrap();
        let words: Vec<_> = verse
            .words()
            .iter()
            .map(|w| (w.content.as_str(), w.occurrence, w.occurrences))
            .collect();
        assert_eq!(words, vec![("καὶ", 1, 2), ("καὶ", 2, 2)]);
        assert_eq!(verse.words()[0].morph.as_deref(), Some("Gr,CC,,,,,,,,"));
        assert_eq!(verse.words()[0].strong.as_deref(), Some("G25320"));
        assert_eq!(
            verse.words()[1].extra,
            vec![("x-tw".to_owned(), "rc://x".to_owned())]
        );
    }

    #[test]
    fn md5_reads_and_writes_lowercase_hex_only() {
        let md5 = Md5::of("Pablo, siervo de Dios".as_bytes());
        let text = md5.to_string();
        assert_eq!(text.len(), 32);
        assert_eq!(text.parse::<Md5>().unwrap(), md5);
        assert!(text.to_uppercase().parse::<Md5>().is_err());
        assert!("abc".parse::<Md5>().is_err());
    }

    #[test]
    fn bank_order_groups_by_occurrence() {
        let verse = TargetVerse::new("the cat saw the dog");
        let order: Vec<_> = verse
            .bank_order()
            .into_iter()
            .map(|w| format!("{}{}", w.text, w.occurrence))
            .collect();
        assert_eq!(order, ["the1", "cat1", "saw1", "dog1", "the2"]);
    }
}
