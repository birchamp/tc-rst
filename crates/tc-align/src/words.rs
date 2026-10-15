//! The two sides of an alignment: the original-language verse and the target verse.
//!
//! A word's identity on either side is its text plus its occurrence: the 1-based position
//! among words with the same text in the verse (tC4 invariant I-2). Both sides number
//! occurrences the way tC4 does, so a record made by tC4 names the same words here.

use crate::Error;
use md5::{Digest, Md5};
use std::fmt;
use std::str::FromStr;
use tc_usfm::markers::{self, MarkerClass};
use tc_usfm::{Document, Node, SourceWord, TargetWord};

/// The original-language words of one verse, in verse order, numbered by occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceVerse {
    words: Vec<SourceWord>,
}

impl SourceVerse {
    /// The words of an original-language verse body such as UGNT or UHB writes it:
    /// `\w Παῦλος|lemma="Παῦλος" strong="G39720" x-morph="Gr,N,,,,,NMS,"\w*`.
    ///
    /// Every `\w` in verse text counts, at any depth of character markup; notes, figures
    /// and other non-text spans are skipped. `lemma`, `strong` (or `strongs`) and `morph`
    /// are read with or without the `x-` prefix, and the default attribute is the lemma.
    /// Written occurrence attributes are ignored: occurrences are counted by text across
    /// the verse, as word-aligner's `getOrderedVerseObjects` counts them for tC4's
    /// bootstrap. Other attributes are kept in [`SourceWord::extra`] as written.
    pub fn from_usfm(body: &str) -> Result<Self, Error> {
        let doc = Document::parse(body);
        let mut found = Vec::new();
        let mut in_heading = false;
        collect_source_words(doc.nodes(), &mut found, &mut in_heading)?;
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

/// Notes, figures, alternate and published numbers, categories and quotation references:
/// spans whose content is never verse text (the same set `tc-usfm` skips).
fn is_non_text_span(name: &str) -> bool {
    markers::classify(name) == MarkerClass::Note
        || matches!(name, "fig" | "ca" | "va" | "vp" | "cat" | "rq")
}

fn collect_source_words(
    nodes: &[Node<'_>],
    out: &mut Vec<SourceWord>,
    in_heading: &mut bool,
) -> Result<(), Error> {
    for node in nodes {
        let span = match node {
            Node::Marker(marker) if !marker.is_closing() && !marker.is_nested() => {
                // A heading's words are not verse words; its text runs to the next
                // paragraph, verse or chapter marker.
                match marker.class() {
                    MarkerClass::Heading
                    | MarkerClass::Identification
                    | MarkerClass::Introduction
                    | MarkerClass::ChapterLevel => *in_heading = true,
                    MarkerClass::Paragraph | MarkerClass::Verse | MarkerClass::Chapter => {
                        *in_heading = false;
                    }
                    MarkerClass::Note | MarkerClass::NoteContent | MarkerClass::Character => {}
                }
                continue;
            }
            Node::Span(span) => span,
            _ => continue,
        };
        if *in_heading || is_non_text_span(span.name()) {
            continue;
        }
        if span.name() != "w" {
            collect_source_words(&span.children, out, in_heading)?;
            continue;
        }
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
        out.push(word);
    }
    Ok(())
}

/// The target-language verse an alignment is made against: its plain text, the text's
/// hash, and its words numbered by occurrence.
///
/// The text is the verse's plain text as STORAGE.md §7.7 defines it for alignment hashes
/// (`plain_text_v1` of the verse content, tC4's `verseTextMd5` input). Both the words and
/// the hash come from that one text, exactly as tC4's aligner tokenizes and hashes its
/// `targetText`, so the words a record names and the hash it carries always agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetVerse {
    text: String,
    md5: TextMd5,
    words: Vec<TargetWord>,
}

impl TargetVerse {
    pub fn new(text: &str) -> Self {
        let words = tc_usfm::words::words(text)
            .into_iter()
            .map(|w| TargetWord {
                text: w.text.to_owned(),
                occurrence: w.occurrence,
                occurrences: w.occurrences,
            })
            .collect();
        Self {
            text: text.to_owned(),
            md5: TextMd5::of(text),
            words,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn md5(&self) -> TextMd5 {
        self.md5
    }

    /// Every word of the verse in order.
    pub fn words(&self) -> &[TargetWord] {
        &self.words
    }

    /// Where `word` stands in the verse, by text and occurrence.
    pub fn position(&self, word: &TargetWord) -> Option<usize> {
        self.words.iter().position(|w| same_word(w, word))
    }

    /// The verse's words in the order tC4's bootstrap banks them: grouped by occurrence
    /// (every first occurrence, then every second, ...), in verse order within a group.
    ///
    /// word-aligner's `unmerge`, which tC4 calls to start an unaligned verse, files each
    /// unaligned word under an empty alignment keyed by its occurrence, then concatenates
    /// those alignments' words, so "the cat saw the dog" banks as `the cat saw dog the`.
    pub fn bank_order(&self) -> Vec<TargetWord> {
        let mut words = self.words.clone();
        words.sort_by_key(|w| w.occurrence);
        words
    }
}

/// Identity of a target word inside one verse: its text plus its occurrence.
pub(crate) fn same_word(a: &TargetWord, b: &TargetWord) -> bool {
    a.text == b.text && a.occurrence == b.occurrence
}

/// Occurrence numbering by exact text: each word's 1-based index among equal texts, and
/// the count of equal texts.
fn number<'a>(texts: impl Iterator<Item = &'a str> + Clone) -> Vec<(u32, u32)> {
    let mut totals: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for text in texts.clone() {
        *totals.entry(text).or_default() += 1;
    }
    let mut seen: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    texts
        .map(|text| {
            let n = seen.entry(text).or_default();
            *n += 1;
            (*n, totals.get(text).copied().unwrap_or(0))
        })
        .collect()
}

/// The md5 of a verse's plain text, as tC4 stores it in `targetVerseMd5`: 32 lowercase
/// hexadecimal digits. Maps one to one to `tc-journal`'s `Md5`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextMd5([u8; 16]);

impl TextMd5 {
    /// The md5 of the UTF-8 bytes of `text`.
    pub fn of(text: &str) -> Self {
        Self(Md5::digest(text.as_bytes()).into())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Display for TextMd5 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::write_hex(f, &self.0)
    }
}

impl fmt::Debug for TextMd5 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TextMd5({self})")
    }
}

impl FromStr for TextMd5 {
    type Err = Error;

    /// Exactly 32 lowercase hexadecimal digits, the only form tC4 writes.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        crate::parse_hex(s)
            .map(Self)
            .ok_or_else(|| Error::InvalidMd5(s.to_owned()))
    }
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
        let md5 = TextMd5::of("Pablo, siervo de Dios");
        let text = md5.to_string();
        assert_eq!(text.len(), 32);
        assert_eq!(text.parse::<TextMd5>().unwrap(), md5);
        assert!(text.to_uppercase().parse::<TextMd5>().is_err());
        assert!("abc".parse::<TextMd5>().is_err());
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
