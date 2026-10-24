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
use tc_journal::alignment::VerseOrder;
use tc_journal::decision::TargetText;
use tc_journal::record::{SourceWord, TargetWord};
use tc_usfm::Book;

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
                        u.chapter() == Some(chapter) && u.verses() == VerseSpan::range(verse, verse)
                    })
                    .map(|u| (at, u))
            });
            let Some((at, unit)) = found else {
                continue;
            };
            let mut text = unit.body().to_owned();
            if let Some(next) = segments.get(at + 1) {
                tc_usfm::source::push_leading(next, &mut text);
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
        let found = tc_usfm::source::source_words(body).map_err(Error::Usfm)?;
        Ok(Self::new(found.iter().map(SourceWord::from_usfm).collect()))
    }

    /// Words given in verse order. Their occurrences are renumbered by text.
    pub fn new(mut words: Vec<SourceWord>) -> Self {
        let counted = number(words.iter().map(|w| w.word.as_str()));
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

/// An original-language book cut into its verses once, so that opening a verse costs that
/// verse: [`SourceVerse::from_book`] looks the verse up in the whole book each time, and a
/// session opens many verses of one book. Each verse holds the text tC4 reads for it: from
/// after its `\v` marker to the next `\v` or `\c` marker, with the headings that lead into
/// the next unit.
#[derive(Debug, Clone, Default)]
pub struct SourceBook {
    units: Vec<SourceUnit>,
    /// `(chapter, verse)` to the unit of that single verse.
    at: HashMap<(u16, u16), usize>,
}

#[derive(Debug, Clone)]
struct SourceUnit {
    text: String,
}

impl SourceBook {
    pub fn parse(usfm: &str) -> Self {
        let book = Book::parse(usfm);
        let segments = book.segments();
        let mut out = Self::default();
        for (at, segment) in segments.iter().enumerate() {
            let Some(unit) = segment.as_verse() else {
                continue;
            };
            let (Some(chapter), Some(verses)) = (unit.chapter(), unit.verses()) else {
                continue;
            };
            if verses.start() != verses.end() {
                continue;
            }
            let mut text = unit.body().to_owned();
            if let Some(next) = segments.get(at + 1) {
                tc_usfm::source::push_leading(next, &mut text);
            }
            // The first unit of a verse is the one `from_book` finds.
            out.at
                .entry((chapter, verses.start()))
                .or_insert(out.units.len());
            out.units.push(SourceUnit { text });
        }
        out
    }

    /// The original-language words of a verse or span: its member verses in order. A member
    /// verse the book does not have adds no words, as in [`SourceVerse::from_book`].
    pub fn verse(&self, chapter: u16, verses: VerseSpan) -> Result<SourceVerse, Error> {
        let mut parts = Vec::new();
        for verse in verses.verses() {
            if let Some(&at) = self.at.get(&(chapter, verse)) {
                parts.push(SourceVerse::from_usfm(&self.units[at].text)?);
            }
        }
        Ok(SourceVerse::span(parts))
    }

    /// The readable text of a verse or span, member verses joined by a space (tC4's
    /// `verseText`).
    pub fn display_text(&self, chapter: u16, verses: VerseSpan) -> String {
        verses
            .verses()
            .filter_map(|verse| self.at.get(&(chapter, verse)))
            .map(|&at| tc_usfm::display_text(&self.units[at].text))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Whether the book has any verse of the span.
    pub fn has(&self, chapter: u16, verses: VerseSpan) -> bool {
        verses
            .verses()
            .any(|verse| self.at.contains_key(&(chapter, verse)))
    }
}

/// The target-language verse an alignment is made against: the text the aligner shows, its
/// words numbered by occurrence, and the hash an alignment records of it.
///
/// It is made from the verse's content (`words + layout` of its verse record, the USFM
/// after `\v N `). The words are `tc_usfm::target_words` of the content, the words the export
/// weaves, so a value that places every word the aligner banks weaves (tC4 #796): a word
/// never joins the next across a footnote or other marker (#795), and every word of a nested
/// alignment, such as Genesis 1:1 "the heavens", a Psalm's superscription and its `\qs` Selah
/// are words to align (#782). The text is the one tC4's aligner shows ([`TargetText::shown`],
/// `tc_usfm::shown_text_v1`), for display only: it decides no word. The hash is the
/// journal's ([`TargetText::md5`], md5 of `plain_text_v1`), the one the fold checks an
/// alignment with (§7.7), so an alignment the aligner saves is valid in the alignment files.
/// (tC4's aligner stamps the md5 of its shown text instead, which its own fold and export
/// reject on every verse where the two texts differ, such as a verse with a poetry line
/// break.)
#[derive(Clone, PartialEq, Eq)]
pub struct TargetVerse {
    text: String,
    md5: Md5,
    words: Vec<TargetWord>,
    /// Each word text's places in the verse, by occurrence, so that a word's place is
    /// found without a scan: the verse-order sort runs after every edit.
    places: HashMap<String, Vec<usize>>,
    aligned_in_text: bool,
}

impl TargetVerse {
    /// The verse whose content is `content`.
    pub fn new(content: &str) -> Self {
        let derived = TargetText::new(content.to_owned());
        let words = tc_journal::alignment::verse_words(content);
        let mut places: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, word) in words.iter().enumerate() {
            places.entry(word.word.clone()).or_default().push(at);
        }
        Self {
            md5: derived.md5(),
            text: derived.shown().to_owned(),
            words,
            places,
            aligned_in_text: holds_zaln(content),
        }
    }

    /// The verse still holds the `\zaln` markup of an aligned file it was added from (tC4
    /// #794): that markup is its alignment, so it does not open in the aligner and no
    /// reflow writes over it.
    pub fn holds_alignment_markup(&self) -> bool {
        self.aligned_in_text
    }

    /// The shown text: what the aligner shows. It decides no word ([`TargetVerse::words`]).
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
        self.places.get(word.word.as_str())?.get(index).copied()
    }

    /// The verse's order of words, for the journal's sort (§5.3, tC4's `inVerseOrder`,
    /// #734): a repeated word by its occurrence, and a word the verse does not have after
    /// the others, in the order it was in. Right-to-left text is in reading order here too:
    /// the order is the text's, not the screen's.
    pub fn order(&self) -> VerseOrder<'_> {
        VerseOrder::new(self.words.iter().map(|w| (w.word.as_str(), w.occurrence)))
    }

    /// Put `words` in the order they stand in this verse ([`TargetVerse::order`]).
    pub fn sort_in_verse_order(&self, words: &mut [TargetWord]) {
        self.order().sort(words);
    }

    /// The verse's words in the order tC4's bootstrap banks them: grouped by occurrence
    /// (every first occurrence, then every second, ...), in verse order within a group.
    ///
    /// word-aligner's `unmerge`, which tC4 calls to start an unaligned verse, files each
    /// unaligned word under an empty alignment keyed by its occurrence, then concatenates
    /// those alignments' words, so "the cat saw the dog" banks as `the cat saw dog the`.
    /// [`bootstrap`](crate::edit::bootstrap) keeps this order so that it equals tC4's
    /// record; the aligner shows the bank in verse order.
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

/// tC4's `holdsInlineAlignment` (PR #799): `\zaln-s` followed by anything but a letter,
/// digit or `_`.
fn holds_zaln(content: &str) -> bool {
    content.match_indices("\\zaln-s").any(|(at, m)| {
        content[at + m.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
    })
}

/// Identity of a target word inside one verse: its text plus its occurrence.
pub(crate) fn same_word(a: &TargetWord, b: &TargetWord) -> bool {
    a.word == b.word && a.occurrence == b.occurrence
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
        Some(format!("{} {n}", self.word))
    }
}

impl Occurrence for SourceWord {
    fn shown_occurrence(&self) -> Option<u32> {
        (self.occurrences > 1).then_some(self.occurrence)
    }

    fn occurrence_name(&self) -> Option<String> {
        let n = self.shown_occurrence()?;
        Some(format!("{} {n}", self.word))
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
            text: word.word.clone(),
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
    use tc_journal::json::Json;

    #[test]
    fn source_words_are_numbered_by_text_across_the_verse() {
        let body = "\\w καὶ|lemma=\"καί\" strong=\"G25320\" x-morph=\"Gr,CC,,,,,,,,\"\\w* \\f + \\ft \\w x|lemma=\"x\"\\w*\\f* \\add \\w καὶ|lemma=\"καί\" x-tw=\"rc://x\"\\w*\\add*";
        let verse = SourceVerse::from_usfm(body).unwrap();
        let words: Vec<_> = verse
            .words()
            .iter()
            .map(|w| (w.word.as_str(), w.occurrence, w.occurrences))
            .collect();
        assert_eq!(words, vec![("καὶ", 1, 2), ("καὶ", 2, 2)]);
        assert_eq!(verse.words()[0].morph.as_deref(), Some("Gr,CC,,,,,,,,"));
        assert_eq!(verse.words()[0].strong.as_deref(), Some("G25320"));
        assert_eq!(
            verse.words()[1].extra,
            tc_journal::json::JsonMap::from([("x-tw".to_owned(), Json::from("rc://x"))])
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
            .map(|w| format!("{}{}", w.word, w.occurrence))
            .collect();
        assert_eq!(order, ["the1", "cat1", "saw1", "dog1", "the2"]);
    }
}
