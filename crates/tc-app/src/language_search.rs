//! The language suggestions of New Bible and New Open Bible Stories (AD27, tC4 #492): a port
//! of `tC4:src/data/languageSearch.ts`, line by line, over the list tC4 ships
//! (`assets/langnames.json`, Door43's list as tC4 `d310bf3` vendors it). The list stays on
//! the server, off the WebAssembly heap, and nothing here uses the network.
//!
//! The order (tC4's owner decisions of 2026-09-30): the code typed whole, then a name typed
//! whole, then a name or a code that starts with the query, then a word or the acronym inside
//! a name, then a name that contains the query. A language whose code or own name matches
//! comes before every language that only an alternate name reaches; those follow in the same
//! order of ranks. Capital letters and accents do not count. Languages that are still equal go
//! by name, then by code.
//!
//! A Door43 code a Scripture Burrito cannot hold (about 470 of them, such as
//! `acm-x-lebarabic`) is offered in a form it can hold ([`offered_code`], owner's answer to
//! Q35), never stored as "Undetermined" (tC4 #582).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use serde::Deserialize;
use tc_api::shown::Direction;
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

use crate::js;

/// The list tC4 ships, byte for byte (`assets/README.md`).
const SHIPPED: &[u8] = include_bytes!("../assets/langnames.json");

/// How many suggestions a query gets at most (`suggestLanguages`'s default).
pub const SUGGESTION_LIMIT: usize = 100;

/// One language of the list: the fields the app reads from Door43's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageRow {
    /// The language code, as Door43 writes it.
    pub lc: String,
    /// The anglicized name; empty for some languages.
    pub ang: String,
    /// The name in the language itself.
    pub ln: String,
    pub ld: Direction,
    /// Alternate names.
    pub alt: Vec<String>,
}

/// Why the shipped list did not read. The list is compiled in, so this is a bug a test finds.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the language list does not read: {0}")]
pub struct LanguageListError(String);

/// The name a suggestion shows and a choice stores: the anglicized name, or the language's own
/// name where Door43 has no anglicized one.
pub fn language_name(row: &LanguageRow) -> &str {
    if row.ang.is_empty() {
        &row.ln
    } else {
        &row.ang
    }
}

const CODE: u8 = 0;
const WHOLE: u8 = 1;
const STARTS: u8 = 2;
const WORD: u8 = 3;
const CONTAINS: u8 = 4;
const NONE: u8 = 5;

/// `fold`: decomposed, without marks, in lower case, trimmed as JavaScript trims, in that
/// order (a letter whose lower case adds a mark keeps it, as in tC4).
fn fold(text: &str) -> String {
    let unmarked: String = text.nfd().filter(|c| !is_mark(*c)).collect();
    js::trim(&unmarked.to_lowercase()).to_owned()
}

/// `\p{M}`.
fn is_mark(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}

/// `[\p{L}\p{N}]`, by General Category (Rust's `is_alphanumeric` is the Alphabetic property).
fn is_letter_or_number(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
            | GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber
    )
}

/// `words`: the runs of letters and numbers.
fn words(folded: &str) -> Vec<&str> {
    folded
        .split(|c: char| !is_letter_or_number(c))
        .filter(|word| !word.is_empty())
        .collect()
}

#[derive(Debug, Clone)]
struct Name {
    folded: String,
    /// The words of the name, joined by one space.
    words: String,
    /// The first UTF-16 code unit of each word, as `part[0]` takes it in JavaScript: half of
    /// a character above U+FFFF.
    acronym: Vec<u16>,
}

fn to_name(text: &str) -> Name {
    let folded = fold(text);
    let parts = words(&folded);
    let acronym = parts
        .iter()
        .filter_map(|part| part.encode_utf16().next())
        .collect();
    let words = parts.join(" ");
    Name {
        folded,
        words,
        acronym,
    }
}

#[derive(Debug, Clone)]
struct Indexed {
    row: LanguageRow,
    code: String,
    /// The folded display name: the sort key among equal ranks.
    name: String,
    own: Vec<Name>,
    alternate: Vec<Name>,
}

/// The list, folded once (`indexLanguages`); [`LanguageIndex::suggest`] reads it at every
/// keystroke.
#[derive(Debug, Clone)]
pub struct LanguageIndex {
    rows: Vec<Indexed>,
}

/// How many times the shipped index was built in this process: once, whatever reads it.
static BUILDS: AtomicUsize = AtomicUsize::new(0);
static SHARED: OnceLock<Result<LanguageIndex, LanguageListError>> = OnceLock::new();

/// Whether `needle` is a run of `haystack`, by code units (`String.prototype.includes`).
fn includes(haystack: &[u16], needle: &[u16]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// The query as `rankNames` reads it.
struct Query<'a> {
    folded: &'a str,
    words: String,
    utf16: Vec<u16>,
}

fn rank_names(names: &[Name], query: &Query<'_>) -> u8 {
    let mut best = NONE;
    for name in names {
        let rank = if name.folded == query.folded {
            WHOLE
        } else if name.folded.starts_with(query.folded) {
            STARTS
        } else if (!query.words.is_empty()
            && format!(" {}", name.words).contains(&format!(" {}", query.words)))
            // The acronym, for a query of more than one code unit.
            || (query.utf16.len() > 1 && includes(&name.acronym, &query.utf16))
        {
            WORD
        } else if name.folded.contains(query.folded) {
            CONTAINS
        } else {
            NONE
        };
        best = best.min(rank);
    }
    best
}

impl LanguageIndex {
    /// The list tC4 ships, read strictly. Use [`LanguageIndex::shared`] to read it once.
    pub fn shipped() -> Result<Self, LanguageListError> {
        let rows: Vec<LanguageRow> =
            serde_json::from_slice(SHIPPED).map_err(|e| LanguageListError(e.to_string()))?;
        Ok(Self::from_rows(rows))
    }

    /// The shipped list, built the first time any App reads it and kept for the process.
    pub fn shared() -> Result<&'static LanguageIndex, &'static LanguageListError> {
        SHARED
            .get_or_init(|| {
                BUILDS.fetch_add(1, AtomicOrdering::SeqCst);
                Self::shipped()
            })
            .as_ref()
    }

    /// How many times [`LanguageIndex::shared`] built the index in this process.
    pub fn shared_builds() -> usize {
        BUILDS.load(AtomicOrdering::SeqCst)
    }

    pub fn from_rows(rows: Vec<LanguageRow>) -> Self {
        let rows = rows
            .into_iter()
            .map(|row| {
                let code = fold(&row.lc);
                let name = fold(language_name(&row));
                let own = [row.ang.as_str(), row.ln.as_str()]
                    .into_iter()
                    .filter(|name| !name.is_empty())
                    .map(to_name)
                    .collect();
                let alternate = row.alt.iter().map(|name| to_name(name)).collect();
                Indexed {
                    row,
                    code,
                    name,
                    own,
                    alternate,
                }
            })
            .collect();
        Self { rows }
    }

    /// Every row, in the list's order.
    pub fn rows(&self) -> impl Iterator<Item = &LanguageRow> {
        self.rows.iter().map(|indexed| &indexed.row)
    }

    /// The languages that match `query`, best first, `limit` at most (`suggestLanguages`).
    pub fn suggest(&self, query: &str, limit: usize) -> Vec<&LanguageRow> {
        let folded = fold(query);
        if folded.is_empty() {
            return Vec::new();
        }
        let query = Query {
            folded: &folded,
            words: words(&folded).join(" "),
            utf16: folded.encode_utf16().collect(),
        };
        struct Hit<'a> {
            language: &'a Indexed,
            rank: u8,
            alternate: u8,
        }
        let mut hits: Vec<Hit<'_>> = Vec::new();
        for language in &self.rows {
            let mut own = rank_names(&language.own, &query);
            if language.code == folded {
                own = CODE;
            } else if language.code.starts_with(&folded) {
                own = own.min(STARTS);
            }
            let alternate = rank_names(&language.alternate, &query);
            if own == NONE && alternate == NONE {
                continue;
            }
            hits.push(if own == NONE {
                Hit {
                    language,
                    rank: alternate,
                    alternate: 1,
                }
            } else {
                Hit {
                    language,
                    rank: own,
                    alternate: 0,
                }
            });
        }
        // Stable, as JavaScript's sort is.
        hits.sort_by(|a, b| {
            a.alternate
                .cmp(&b.alternate)
                .then(a.rank.cmp(&b.rank))
                .then_with(|| js::utf16_cmp(&a.language.name, &b.language.name))
                .then_with(|| js::utf16_cmp(&a.language.code, &b.language.code))
        });
        hits.into_iter()
            .take(limit)
            .map(|hit| &hit.language.row)
            .collect()
    }
}

/// What the Code box gets for a Door43 code (owner's answer to Q35, 2026-10-19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// The code is a language tag a Scripture Burrito can hold.
    Same,
    /// A tag made from the code that a Scripture Burrito can hold; the project keeps the
    /// Door43 code beside it.
    Adjusted(String),
    /// No tag can be made from the code: the person types one.
    Unusable,
}

/// The longest private subtag a language tag holds.
const PRIVATE_PIECE: usize = 8;

/// The code the Code box gets for `door43`, a code of Door43's list. A code the schema takes
/// is offered as it is. Otherwise, when its first subtag is two or three letters, the rest
/// becomes a private part: every letter and digit of the other subtags (but `x`), in lower
/// case, in pieces of at most eight, so no letter is lost: `acm-x-lebarabic` becomes
/// `acm-x-lebarabi-c`. Cutting the rest to eight letters instead would lose letters and give
/// two languages one code, or another language's code. Anything else is unusable.
pub fn offered_code(door43: &str) -> Offer {
    if tc_burrito::is_language_tag(door43) {
        return Offer::Same;
    }
    let mut subtags = door43.split('-');
    let first = subtags.next().unwrap_or_default();
    if !(2..=3).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Offer::Unusable;
    }
    let rest: String = subtags
        .filter(|subtag| !subtag.eq_ignore_ascii_case("x"))
        .flat_map(str::chars)
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let pieces: Vec<&str> = rest
        .as_bytes()
        .chunks(PRIVATE_PIECE)
        // ASCII only, so every chunk is text.
        .filter_map(|chunk| std::str::from_utf8(chunk).ok())
        .collect();
    let offered = format!("{first}-x-{}", pieces.join("-"));
    if tc_burrito::is_language_tag(&offered) {
        Offer::Adjusted(offered)
    } else {
        Offer::Unusable
    }
}

/// The longest query the field sends a list for: a longer one is no language's name.
pub const MAX_QUERY: usize = 200;
