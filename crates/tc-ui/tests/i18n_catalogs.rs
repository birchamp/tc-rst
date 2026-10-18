//! tC4's catalog checks (`test/i18n-catalogs.test.ts` at d310bf3) over the catalogs vendored in
//! `assets/i18n`. A target catalog that lacks an English key shows English for it; one with an
//! extra key holds text no screen shows; an empty entry is a blank label; a lost or renamed
//! `{placeholder}` shows "{n}" on screen or drops a number; an English sentence in a target
//! catalog is translated by name only. That file's three `t()` cases, and
//! `test/i18n-keys.test.ts`, are not here: the rebuild has no `t()` and no screen that names a
//! key yet (docs/TC4-DELTA-2026-10-16.md §3.5, "Still open").
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::{Map, Value};
use tc_ui::i18n::{DEFAULT_LOCALE, LOCALES, TextDir, locale, placeholders};
use unicode_general_category::{GeneralCategory as G, get_general_category};

fn catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/i18n")
}

fn catalog(id: &str) -> Map<String, Value> {
    let path = catalog_dir().join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    match serde_json::from_str(&text) {
        Ok(Value::Object(m)) => m,
        other => panic!("{id}.json is not a JSON object: {other:?}"),
    }
}

fn english() -> BTreeMap<String, String> {
    catalog(DEFAULT_LOCALE)
        .into_iter()
        .map(|(k, v)| match v {
            Value::String(s) => (k, s),
            other => panic!("en.json {k} is not a string: {other}"),
        })
        .collect()
}

fn targets() -> impl Iterator<Item = &'static str> {
    LOCALES
        .iter()
        .map(|l| l.id)
        .filter(|id| *id != DEFAULT_LOCALE)
}

/// A character of a word as tC4's `/[\p{L}\p{N}]+/gu` counts it: a letter or a number by its
/// general category. `char::is_alphabetic` would also take the vowel signs and other combining
/// marks (Other_Alphabetic), which split a word in tC4.
fn letter_or_number(c: char) -> bool {
    matches!(
        get_general_category(c),
        G::UppercaseLetter
            | G::LowercaseLetter
            | G::TitlecaseLetter
            | G::ModifierLetter
            | G::OtherLetter
            | G::DecimalNumber
            | G::LetterNumber
            | G::OtherNumber
    )
}

/// English messages a translation may equal: product names, codes, symbols, numbers, and a
/// single word (a cognate such as "Format", with its placeholders). Never a sentence of two
/// or more words. tC4's `SHARED` rule.
fn may_equal_english(message: &str) -> bool {
    let mut text = message.to_string();
    for p in placeholders(message) {
        text = text.replace(p, " ");
    }
    let words = text
        .split(|c: char| !letter_or_number(c))
        .filter(|w| !w.is_empty())
        .count();
    words <= 1
        || matches!(
            message.trim(),
            "translationCore"
                | "translationCore4"
                | "translationCore 4"
                | "Scripture Burrito"
                | "Scripture Burrito (.zip)"
                | "Open Bible Stories"
                | "unfoldingWord Literal Text"
                | "unfoldingWord Simplified Text"
        )
}

#[test]
fn the_registry_names_the_ten_decided_locales_each_with_a_catalog() {
    let ids: Vec<_> = LOCALES.iter().map(|l| l.id).collect();
    assert_eq!(
        ids,
        [
            "en", "es-419", "fr", "hi", "pt-BR", "id", "uk", "ru", "vi", "ar"
        ]
    );
    let labels: Vec<_> = LOCALES.iter().map(|l| l.label).collect();
    assert_eq!(
        labels,
        [
            "English",
            "Español (Latinoamérica)",
            "Français",
            "हिन्दी",
            "Português (Brasil)",
            "Bahasa Indonesia",
            "Українська",
            "Русский",
            "Tiếng Việt",
            "العربية",
        ]
    );
    // #657: Arabic is the one interface that reads right to left.
    let rtl: Vec<_> = LOCALES
        .iter()
        .filter(|l| l.dir != TextDir::Ltr)
        .map(|l| l.id)
        .collect();
    assert_eq!(rtl, ["ar"]);
    for id in ids.iter().copied() {
        assert_eq!(locale(id).map(|l| l.id), Some(id));
    }
    let files: BTreeSet<String> = std::fs::read_dir(catalog_dir())
        .expect("assets/i18n is readable")
        .map(|e| {
            e.expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let want: BTreeSet<String> = ids.iter().map(|id| format!("{id}.json")).collect();
    assert_eq!(
        files, want,
        "assets/i18n holds exactly one catalog per locale"
    );
}

#[test]
fn every_target_covers_every_english_key_and_no_other() {
    let en = english();
    for id in targets() {
        let target = catalog(id);
        let missing: Vec<_> = en.keys().filter(|k| !target.contains_key(*k)).collect();
        let extra: Vec<_> = target.keys().filter(|k| !en.contains_key(*k)).collect();
        assert!(missing.is_empty(), "{id} lacks: {missing:?}");
        assert!(extra.is_empty(), "{id} has keys English lacks: {extra:?}");
    }
}

#[test]
fn every_entry_is_nonempty_text() {
    for l in &LOCALES {
        let bad: Vec<_> = catalog(l.id)
            .into_iter()
            .filter(|(_, v)| v.as_str().is_none_or(|s| s.trim().is_empty()))
            .map(|(k, _)| k)
            .collect();
        assert!(
            bad.is_empty(),
            "{} has empty or non-string entries: {bad:?}",
            l.id
        );
    }
}

#[test]
fn every_entry_keeps_its_english_placeholders_by_name_and_count() {
    let en = english();
    for id in targets() {
        let target = catalog(id);
        let bad: Vec<_> = en
            .iter()
            .filter(|(k, source)| {
                target
                    .get(*k)
                    .and_then(Value::as_str)
                    .is_some_and(|t| placeholders(t) != placeholders(source))
            })
            .map(|(k, _)| k)
            .collect();
        assert!(bad.is_empty(), "{id} placeholder mismatch in: {bad:?}");
    }
}

#[test]
fn no_target_passes_an_english_sentence_off_as_translated() {
    let en = english();
    for id in targets() {
        let target = catalog(id);
        let copied: Vec<_> = en
            .iter()
            .filter(|(k, source)| {
                target.get(*k).and_then(Value::as_str) == Some(source.as_str())
                    && !may_equal_english(source)
            })
            .map(|(k, _)| k)
            .collect();
        assert!(copied.is_empty(), "{id} equals English in: {copied:?}");
    }
}

#[test]
fn the_exemption_from_translation_is_for_names_and_single_words_only() {
    assert!(may_equal_english("Format"));
    assert!(may_equal_english("{n} USFM"));
    assert!(may_equal_english("Scripture Burrito (.zip)"));
    assert!(may_equal_english("translationCore 4"));
    assert!(!may_equal_english("Sync with Door43"));
    assert!(!may_equal_english("Open Bible Stories now"));
}

#[test]
fn the_exemption_counts_words_as_tc4_does_where_combining_marks_split_them() {
    // tC4 counts runs of letters and numbers (general categories L and N), so a vowel sign
    // (Mc) or a combining mark (Mn) ends a word: "भाषा" is two words there, as are these.
    assert!(!may_equal_english("भाषा"));
    assert!(!may_equal_english("{n}% मसौदा"));
    assert!(!may_equal_english("كِتَاب"));
    assert!(!may_equal_english("nai\u{308}ve"));
    // A precomposed letter is one letter; a letter number and an other number count as numbers.
    assert!(may_equal_english("Tiếng"));
    assert!(may_equal_english("Ⅻ²"));
    // A circled letter is a symbol (So) with the Alphabetic property: no word in tC4.
    assert!(may_equal_english("Ⓐ Ⓑ"));
}
