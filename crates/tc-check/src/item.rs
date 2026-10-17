//! A check item: one tN note or tW word link that the checker decides on.

use std::fmt;

use serde_json::{Map, Value};
use tc_helps::highlight::{AlignedVerse, gateway_quote};
use tc_helps::{HelpItemId, HelpRef, Occurrence, Quote, WordCategory};
use tc_journal::json::{Json, JsonMap};

use crate::js;
use crate::key::{CheckKey, ToolId};

/// The tC3 thematic category of a tN check, from its Translation Academy slug
/// (tC4 `TN_CATEGORY_MAP`). A slug the map does not name is `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoteCategory {
    Discourse,
    Numbers,
    Figures,
    Culture,
    Grammar,
    Other,
}

const NOTE_CATEGORIES: [(NoteCategory, &[&str]); 6] = [
    (
        NoteCategory::Discourse,
        &[
            "figs-declarative",
            "figs-events",
            "figs-exclamations",
            "figs-exmetaphor",
            "figs-imperative",
            "figs-parables",
            "figs-pastforfuture",
            "figs-quotations",
            "figs-quotesinquotes",
            "figs-sentences",
            "translate-versebridge",
            "writing-background",
            "writing-connectingwords",
            "writing-endofstory",
            "writing-intro",
            "writing-newevent",
            "writing-participants",
            "writing-poetry",
            "writing-proverbs",
            "writing-quotations",
        ],
    ),
    (
        NoteCategory::Numbers,
        &[
            "translate-fraction",
            "translate-numbers",
            "translate-ordinal",
        ],
    ),
    (
        NoteCategory::Figures,
        &[
            "figs-apostrophe",
            "figs-doublenegatives",
            "figs-doublet",
            "figs-ellipsis",
            "figs-euphemism",
            "figs-hendiadys",
            "figs-hyperbole",
            "figs-idiom",
            "figs-irony",
            "figs-litotes",
            "figs-merism",
            "figs-metaphor",
            "figs-metonymy",
            "figs-parallelism",
            "figs-personification",
            "figs-rquestion",
            "figs-simile",
            "figs-synecdoche",
            "figs-quotemarks",
        ],
    ),
    (
        NoteCategory::Culture,
        &[
            "figs-explicit",
            "figs-go",
            "translate-bdistance",
            "translate-bmoney",
            "translate-bvolume",
            "translate-hebrewmonths",
            "translate-names",
            "translate-bweight",
            "translate-symaction",
            "translate-unknown",
            "writing-symlanguage",
        ],
    ),
    (
        NoteCategory::Grammar,
        &[
            "figs-abstractnouns",
            "figs-activepassive",
            "figs-distinguish",
            "figs-exclusive",
            "figs-123person",
            "figs-they",
            "figs-you",
            "figs-we",
            "figs-genericnoun",
            "figs-hypo",
            "figs-inclusive",
            "figs-nominaladj",
            "figs-possession",
            "figs-pronouns",
            "figs-rpronouns",
            "figs-gendernotations",
            "grammar-connect-logic-goal",
            "grammar-connect-exceptions",
            "grammar-connect-logic-contrast",
            "grammar-connect-logic-result",
        ],
    ),
    (
        NoteCategory::Other,
        &[
            "guidelines-sonofgodprinciples",
            "translate-manuscripts",
            "translate-textvariants",
            "translate-transliterate",
        ],
    ),
];

impl NoteCategory {
    /// The category of a Translation Academy slug.
    pub fn of(group_id: &str) -> Self {
        NOTE_CATEGORIES
            .iter()
            .find(|(_, slugs)| slugs.contains(&group_id))
            .map_or(Self::Other, |(category, _)| *category)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Discourse => "discourse",
            Self::Numbers => "numbers",
            Self::Figures => "figures",
            Self::Culture => "culture",
            Self::Grammar => "grammar",
            Self::Other => "other",
        }
    }
}

/// The category a check is grouped under: a tN thematic category, or the folder of the
/// tW article (`kt`, `names`, `other`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Note(NoteCategory),
    Word(WordCategory),
}

impl Category {
    /// The category as tC4 files write it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Note(c) => c.name(),
            Self::Word(c) => c.folder(),
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One word of a tN quote and its number among equal words of the quote (tC4
/// `tnQuoteWords`). The number is within the quote, not the verse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteWord {
    pub word: String,
    pub occurrence: u32,
}

/// tC4 `tnQuoteWords`: the quote split on whitespace, `&` dropped, repeated words
/// numbered.
pub fn quote_words(quote: &str) -> Vec<QuoteWord> {
    let mut seen: Vec<(&str, u32)> = Vec::new();
    quote
        .split(js::is_whitespace)
        .filter(|w| !w.is_empty() && *w != "&")
        .map(|word| {
            let occurrence = match seen.iter_mut().find(|(w, _)| *w == word) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    seen.push((word, 1));
                    1
                }
            };
            QuoteWord {
                word: word.to_owned(),
                occurrence,
            }
        })
        .collect()
}

/// A check on the checker's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckItem {
    /// The identity a decision on this check is stored under, with the target in the
    /// project's versification.
    pub key: CheckKey,
    /// The Translation Academy slug of a tN check, or the tW article slug of a tW check.
    pub group_id: String,
    pub category: Category,
    /// The original-language quote exactly as the resource writes it (tC4 `quoteString`).
    pub quote: Quote,
    /// A tN check's note as the TSV cell holds it (tC4 `occurrenceNote`). `None` for tW.
    pub note: Option<String>,
    /// Where the resource row places the check, in the resource's own numbering.
    pub source: HelpRef,
    /// The help row the check comes from, as the helps panel knows it.
    pub help_id: HelpItemId,
}

impl CheckItem {
    /// tC4's `contextId` for this check, with tC4's field order, as a first decision on
    /// it records verbatim (STORAGE.md §5.4).
    pub fn context(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("checkId".into(), self.key.check_id.as_str().into());
        if let Some(note) = &self.note {
            map.insert("occurrenceNote".into(), note.as_str().into());
        }
        map.insert(
            "reference".into(),
            crate::key::reference_json(&self.key.target),
        );
        map.insert("tool".into(), self.key.tool.name().into());
        map.insert("groupId".into(), self.group_id.as_str().into());
        let quote = match self.key.tool {
            ToolId::TranslationWords => Value::from(self.quote.as_str()),
            ToolId::TranslationNotes | ToolId::TranslationQuestions => Value::Array(
                quote_words(self.quote.as_str())
                    .into_iter()
                    .map(|w| {
                        let mut word = Map::new();
                        word.insert("word".into(), w.word.into());
                        word.insert("occurrence".into(), w.occurrence.into());
                        Value::Object(word)
                    })
                    .collect(),
            ),
        };
        map.insert("quote".into(), quote);
        map.insert("quoteString".into(), self.quote.as_str().into());
        map.insert("occurrence".into(), self.key.occurrence.into());
        map
    }

    /// [`CheckItem::context`] as the journal's decision record holds it.
    pub fn journal_context(&self) -> JsonMap {
        self.context()
            .iter()
            // The context holds strings, whole numbers and lists of objects of them, all of
            // which the journal's JSON holds, so no member is left out.
            .filter_map(|(k, v)| Some((k.clone(), Json::try_from(v).ok()?)))
            .collect()
    }

    /// The original-language quote the screens fall back to: a tN quote's words joined
    /// by spaces (so `&` goes), else the quote, else the group id (tC4 `quoteOf`).
    pub fn original_title(&self) -> String {
        let words = quote_words(self.quote.as_str());
        let joined = match self.key.tool {
            ToolId::TranslationWords => self.quote.as_str().to_owned(),
            _ if words.is_empty() => self.quote.as_str().to_owned(),
            _ => words
                .iter()
                .map(|w| w.word.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        };
        if joined.is_empty() {
            self.group_id.clone()
        } else {
            joined
        }
    }

    /// The title of the check on the rail and in the detail header: the gateway words
    /// that render the quote in `gateway`, the aligned gateway verse, else
    /// [`CheckItem::original_title`] (tC4 `gatewayTitleFor`).
    pub fn title(&self, gateway: Option<&AlignedVerse>) -> String {
        let occurrence = Occurrence::nth(self.key.occurrence).unwrap_or(Occurrence::NotTied);
        gateway
            .and_then(|verse| gateway_quote(verse, &self.quote, occurrence))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.original_title())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_words_number_repeats_within_the_quote() {
        let words: Vec<(String, u32)> = quote_words("τοῦ Θεοῦ & τοῦ Σωτῆρος")
            .into_iter()
            .map(|w| (w.word, w.occurrence))
            .collect();
        assert_eq!(
            words,
            vec![
                ("τοῦ".to_owned(), 1),
                ("Θεοῦ".to_owned(), 1),
                ("τοῦ".to_owned(), 2),
                ("Σωτῆρος".to_owned(), 1)
            ]
        );
        assert!(quote_words("  ").is_empty());
    }

    #[test]
    fn categories_follow_the_tc3_map() {
        assert_eq!(NoteCategory::of("figs-metaphor"), NoteCategory::Figures);
        assert_eq!(
            NoteCategory::of("figs-abstractnouns"),
            NoteCategory::Grammar
        );
        assert_eq!(NoteCategory::of("translate-blessing"), NoteCategory::Other);
        assert_eq!(Category::Word(WordCategory::KeyTerm).name(), "kt");
    }
}
