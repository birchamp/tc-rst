//! Quote highlighting against the real aligned en_ult Titus, with tC4's test vectors.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use common::{fixture, repo_text, resource};
use proptest::prelude::*;
use tc_core::BookId;
use tc_helps::highlight::{
    AlignedVerse, MatchQuality, SourceWord, TokenKind, gateway_quote, highlight_plain,
    highlight_quote, normalize_word, tokenize_plain,
};
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{BibleLocus, Error, HelpItemId, HelpRef, Note, Occurrence, Quote};

fn attr<'a>(attrs: &'a str, name: &str) -> &'a str {
    let key = format!("{name}=\"");
    let start = attrs.find(&key).unwrap() + key.len();
    let len = attrs[start..].find('"').unwrap();
    &attrs[start..start + len]
}

/// Reads one verse of aligned USFM into the highlight input: each `\zaln-s` opens an
/// alignment whose sources are its own word plus those of the milestones around it.
fn aligned(text: &str) -> AlignedVerse {
    let mut verse = AlignedVerse::new();
    let mut stack = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("\\zaln-s |") {
            let end = after.find("\\*").unwrap();
            let attrs = &after[..end];
            let mut sources: Vec<SourceWord> = stack
                .last()
                .map(|(_, s): &(_, Vec<SourceWord>)| s.clone())
                .unwrap_or_default();
            sources.push(SourceWord::new(
                attr(attrs, "x-content"),
                attr(attrs, "x-occurrence").parse().unwrap(),
            ));
            let id = verse.add_alignment(sources.clone());
            stack.push((id, sources));
            rest = &after[end + 2..];
        } else if let Some(after) = rest.strip_prefix("\\zaln-e\\*") {
            stack.pop();
            rest = after;
        } else if let Some(after) = rest.strip_prefix("\\w ") {
            let end = after.find("\\w*").unwrap();
            let word = after[..end].split('|').next().unwrap();
            verse
                .push_word(word, stack.last().map(|(id, _)| *id))
                .unwrap();
            rest = &after[end + 3..];
        } else if rest.starts_with('\\') {
            let n = rest[1..]
                .find(|c: char| c.is_whitespace() || c == '\\')
                .map_or(rest.len(), |i| i + 1);
            rest = rest[n..].strip_prefix("\\*").unwrap_or(&rest[n..]);
            verse.push_text(" ");
        } else {
            let n = rest.find('\\').unwrap_or(rest.len());
            verse.push_text(&rest[..n]);
            rest = &rest[n..];
        }
    }
    verse
}

fn ult(chapter: u16, verse: u16) -> AlignedVerse {
    let usfm = fixture("tc4/test-fixtures/en_ult/TIT.usfm");
    let marker = format!("\\c {chapter}\n");
    let body = &usfm[usfm.find(&marker).unwrap() + marker.len()..];
    let body = &body[..body.find("\\c ").unwrap_or(body.len())];
    let marker = format!("\\v {verse} ");
    let text = &body[body.find(&marker).unwrap() + marker.len()..];
    aligned(&text[..text.find("\\v ").unwrap_or(text.len())])
}

/// The verses of a USFM book by chapter and verse, each as [`ult`] cuts one: the text after
/// `\\v N ` up to the next `\\v` or `\\c`.
fn verse_bodies(usfm: &str) -> HashMap<(u16, u16), &str> {
    let mut out = HashMap::new();
    for chapter in usfm.split("\\c ").skip(1) {
        let Some(c) = chapter
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
        else {
            continue;
        };
        for verse in chapter.split("\\v ").skip(1) {
            let Some((n, body)) = verse.split_once(char::is_whitespace) else {
                continue;
            };
            if let Ok(v) = n.parse() {
                out.insert((c, v), body);
            }
        }
    }
    out
}

fn lit(verse: &AlignedVerse, quote: &str, occurrence: u32) -> Vec<String> {
    let occ = Occurrence::nth(occurrence).unwrap();
    highlight_quote(verse, &Quote::new(quote), occ)
        .texts(verse)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_alignment_carries_the_verse_level_occurrence() {
    let v = ult(1, 1);
    let god: Vec<String> = v
        .tokens()
        .iter()
        .filter_map(|t| match t.kind {
            TokenKind::Word(Some(id)) => v
                .sources(id)?
                .iter()
                .find(|s| s.content == "Θεοῦ")
                .map(|s| format!("{}#{}", t.text, s.occurrence)),
            _ => None,
        })
        .collect();
    assert_eq!(god, ["of#1", "God#1", "of#2", "God#2"]);
}

#[test]
fn a_repeated_original_word_resolves_to_the_occurrence_the_quote_names() {
    let v = ult(1, 1);
    // The real en_tn TIT 1:1 quote, with its comma. The second "of God" follows ἐκλεκτῶν.
    let h = highlight_quote(
        &v,
        &Quote::new("κατὰ πίστιν ἐκλεκτῶν Θεοῦ, καὶ ἐπίγνωσιν ἀληθείας"),
        Occurrence::nth(1).unwrap(),
    );
    assert_eq!(h.quality, MatchQuality::Exact);
    let words = h.texts(&v);
    for want in ["faith", "chosen", "people", "knowledge"] {
        assert!(words.contains(&want), "{want} in {words:?}");
    }
    assert!(!words.contains(&"servant") && !words.contains(&"Paul"));
    assert_eq!(words.iter().filter(|w| **w == "God").count(), 1);
}

#[test]
fn a_single_word_resolves_through_its_occurrence() {
    let v = ult(1, 1);
    assert_eq!(lit(&v, "Θεοῦ", 1), ["of", "God"]);
    let second = lit(&v, "Θεοῦ", 2);
    assert_eq!(second, ["of", "God"]);
    let first = highlight_quote(&v, &Quote::new("Θεοῦ"), Occurrence::nth(1).unwrap());
    let other = highlight_quote(&v, &Quote::new("Θεοῦ"), Occurrence::nth(2).unwrap());
    assert!(first.tokens.iter().all(|t| !other.tokens.contains(t)));
    assert!(!second.contains(&"servant".to_owned()));
}

#[test]
fn a_repeated_first_word_anchors_at_the_tightest_rendering() {
    let words = lit(&ult(2, 2), "τῇ ἀγάπῃ", 1);
    assert!(words.contains(&"love".to_owned()), "{words:?}");
    assert!(!words.contains(&"faith".to_owned()) && !words.contains(&"patience".to_owned()));
}

#[test]
fn a_long_quote_never_borrows_words_from_another_clause() {
    let words = lit(
        &ult(1, 15),
        "τοῖς δὲ μεμιαμμένοις καὶ ἀπίστοις, οὐδὲν καθαρόν",
        1,
    );
    for want in ["corrupted", "unbelieving", "pure"] {
        assert!(words.contains(&want.to_owned()), "{want} in {words:?}");
    }
    assert!(!words.contains(&"all".to_owned()));
}

#[test]
fn an_occurrence_the_verse_lacks_lights_nothing() {
    let v = ult(1, 1);
    // ἐκλεκτῶν is rendered by two groups ("of", "the chosen people"), one occurrence.
    let h = highlight_quote(&v, &Quote::new("ἐκλεκτῶν"), Occurrence::nth(2).unwrap());
    assert!(h.tokens.is_empty());
    assert_eq!(h.quality, MatchQuality::None);
    assert!(lit(&v, "ἐκλεκτῶν", 1).contains(&"people".to_owned()));
    assert!(lit(&v, "", 1).is_empty());
    assert!(
        highlight_quote(&v, &Quote::default(), Occurrence::NotTied)
            .tokens
            .is_empty()
    );
    assert!(lit(&v, "λόγος", 1).is_empty());
}

#[test]
fn the_gateway_quote_reads_the_lit_words_in_order() {
    let v = ult(1, 1);
    let gl = |q: &str, n: u32| gateway_quote(&v, &Quote::new(q), Occurrence::nth(n).unwrap());
    assert_eq!(
        gl("ἐκλεκτῶν Θεοῦ", 1).as_deref(),
        Some("of the chosen people of God")
    );
    assert_eq!(gl("Θεοῦ", 1).as_deref(), Some("of God"));
    assert_eq!(
        gl("πίστιν & ἀληθείας", 1).as_deref(),
        Some("the faith … of the truth")
    );
    assert_eq!(gl("λόγος", 1), None);
    assert_eq!(gl("", 1), None);
}

#[test]
fn unrecorded_or_every_occurrence_lights_each_rendering() {
    let v = ult(1, 1);
    let god = Quote::new("Θεοῦ");
    let all = highlight_quote(&v, &god, Occurrence::All);
    assert_eq!(all.quality, MatchQuality::AllOccurrences);
    assert_eq!(all.texts(&v), ["of", "God", "of", "God"]);
    let unknown = highlight_quote(&v, &god, Occurrence::Unrecorded);
    assert_eq!(unknown.tokens, all.tokens);
    // A quote that occurs once needs no occurrence: an Aquifer note still lights exactly.
    let once = highlight_quote(&v, &Quote::new("ἐκλεκτῶν Θεοῦ"), Occurrence::Unrecorded);
    assert_eq!(once.quality, MatchQuality::Exact);
    assert_eq!(once.texts(&v).join(" "), "of the chosen people of God");
}

#[test]
fn real_notes_on_titus_light_their_rendering_in_the_ult() {
    let notes = parse_notes(
        &resource("en_tn@v86/TIT.tsv"),
        TsvScope::Book(BookId::Titus),
    )
    .unwrap()
    .notes;
    let mut tally: std::collections::BTreeMap<String, usize> = Default::default();
    let mut approximate = Vec::new();
    for n in &notes {
        let HelpRef::Bible(r) = &n.reference else {
            continue;
        };
        let BibleLocus::Verses(list) = &r.locus else {
            continue;
        };
        let at = list.ranges()[0].start();
        let h = highlight_quote(&ult(at.chapter, at.verse), &n.quote, n.occurrence);
        *tally.entry(format!("{:?}", h.quality)).or_default() += 1;
        if h.quality == MatchQuality::Approximate {
            approximate.push(n.quote.as_str().to_owned());
        }
    }
    // Every one of the 202 quoted notes lights its rendering. tC4's ordered match finds
    // 149; 46 more render the Greek in another order but use each of their words exactly
    // once in the verse, so they are certain too. The rest repeat a word (καὶ) the verse
    // also uses elsewhere.
    let want: std::collections::BTreeMap<String, usize> =
        [("Exact".to_owned(), 195), ("Approximate".to_owned(), 7)].into();
    assert_eq!(tally, want);
    assert!(approximate.contains(&"ἐπιθυμίαις καὶ ἡδοναῖς ποικίλαις".to_owned()));
    let husband = lit(&ult(1, 6), "μιᾶς γυναικὸς ἀνήρ", 1);
    assert_eq!(husband, ["a", "husband", "of", "one", "wife"]);
}

#[test]
fn a_word_cannot_point_at_an_alignment_the_verse_lacks() {
    let mut other = AlignedVerse::new();
    let id = other.add_alignment(vec![SourceWord::new("x", 1)]);
    let mut verse = AlignedVerse::new();
    assert!(matches!(
        verse.push_word("y", Some(id)),
        Err(Error::UnknownAlignment(0))
    ));
}

/// The words of a UHB verse in order, as its `\\w` markers write them.
fn uhb_words(chapter: u16, verse: u16) -> Vec<String> {
    let usfm = fixture("tc4/test-fixtures/hbo_uhb/JON.usfm");
    let marker = format!("\\c {chapter}\n");
    let body = &usfm[usfm.find(&marker).unwrap() + marker.len()..];
    let body = &body[..body.find("\\c ").unwrap_or(body.len())];
    let marker = format!("\\v {verse}\n");
    let text = &body[body.find(&marker).unwrap() + marker.len()..];
    let text = &text[..text.find("\\v ").unwrap_or(text.len())];
    text.split("\\w ")
        .skip(1)
        .map(|w| w.split('|').next().unwrap().to_owned())
        .collect()
}

/// Each original word as its own alignment, in verse order, with its occurrence counted as
/// a `\\zaln-s` milestone counts it.
fn sources(words: &[String]) -> Vec<SourceWord> {
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            SourceWord::new(
                w.clone(),
                1 + words[..i].iter().filter(|x| *x == w).count() as u32,
            )
        })
        .collect()
}

#[test]
fn hebrew_quotes_split_maqaf_compounds_into_the_aligned_words() {
    let words = uhb_words(1, 1);
    assert_eq!(words.len(), 8, "{words:?}");
    let src = sources(&words);
    // en_ult JON 1:1: "Now the word of Yahweh came to Jonah son of Amittai, saying,"
    let rendering: [&[&str]; 8] = [
        &["Now"],
        &["the", "word"],
        &["of", "Yahweh"],
        &["came", "to"],
        &["Jonah"],
        &["son", "of"],
        &["Amittai"],
        &["saying"],
    ];
    let mut verse = AlignedVerse::new();
    for (source, english) in src.iter().zip(rendering) {
        let id = verse.add_alignment(vec![source.clone()]);
        for word in english {
            verse.push_word(*word, Some(id)).unwrap();
            verse.push_text(" ");
        }
    }
    let notes = parse_notes(
        &resource("en_tn@v86/JON.tsv"),
        TsvScope::Book(BookId::Jonah),
    )
    .unwrap()
    .notes;
    let jdr1 = notes
        .iter()
        .find(|n| n.id.upstream_row_id().unwrap().as_str() == "jdr1")
        .unwrap();
    assert!(jdr1.quote.as_str().contains('\u{05BE}'));
    let h = highlight_quote(&verse, &jdr1.quote, jdr1.occurrence);
    assert_eq!(h.quality, MatchQuality::Exact);
    assert_eq!(h.texts(&verse).join(" "), "Now the word of Yahweh");
    assert_eq!(
        gateway_quote(&verse, &jdr1.quote, jdr1.occurrence).as_deref(),
        Some("Now the word of Yahweh")
    );
    let compound = Quote::new("דְּבַר־יְהוָ֔ה");
    let h = highlight_quote(&verse, &compound, Occurrence::nth(1).unwrap());
    assert_eq!(h.quality, MatchQuality::Exact);
    assert_eq!(h.texts(&verse).join(" "), "the word of Yahweh");
    assert_eq!(lit(&verse, "אֶל־יוֹנָ֥ה", 1), ["came", "to", "Jonah"]);
}

#[test]
fn every_jonah_note_quote_finds_its_words_in_the_uhb_verse() {
    let notes = parse_notes(
        &resource("en_tn@v86/JON.tsv"),
        TsvScope::Book(BookId::Jonah),
    )
    .unwrap()
    .notes;
    let mut tally: std::collections::BTreeMap<String, usize> = Default::default();
    let mut maqaf = 0;
    for n in &notes {
        let HelpRef::Bible(r) = &n.reference else {
            continue;
        };
        let BibleLocus::Verses(list) = &r.locus else {
            continue;
        };
        if n.quote.is_empty() {
            continue;
        }
        let at = list.ranges()[0].start();
        // Each Hebrew word aligned to itself: the quote is in the verse's own order.
        let mut verse = AlignedVerse::new();
        let words = uhb_words(at.chapter, at.verse);
        for source in sources(&words) {
            let id = verse.add_alignment(vec![source.clone()]);
            verse.push_word(source.content, Some(id)).unwrap();
            verse.push_text(" ");
        }
        let h = highlight_quote(&verse, &n.quote, n.occurrence);
        *tally.entry(format!("{:?}", h.quality)).or_default() += 1;
        if n.quote.as_str().contains('\u{05BE}') {
            maqaf += 1;
            assert_eq!(h.quality, MatchQuality::Exact, "{}", n.quote);
        }
    }
    assert_eq!(maqaf, 65);
    let want: std::collections::BTreeMap<String, usize> = [("Exact".to_owned(), 191)].into();
    assert_eq!(tally, want);
}

/// The single verse a help row is on, or `None` for any other place.
fn single_verse(reference: &HelpRef) -> Option<(u16, u16)> {
    let HelpRef::Bible(r) = reference else {
        return None;
    };
    let BibleLocus::Verses(list) = &r.locus else {
        return None;
    };
    let [range] = list.ranges() else {
        return None;
    };
    (range.start() == range.end()).then_some((range.start().chapter, range.start().verse))
}

/// One verse of a USFM fixture.
fn fixture_verse(path: &str, chapter: u16, verse: u16) -> AlignedVerse {
    let usfm = fixture(path);
    aligned(verse_bodies(&usfm)[&(chapter, verse)])
}

/// The notes of a book in en_tn v91, as the bundled export holds them.
fn v91_notes(book: BookId) -> Vec<Note> {
    let entry = format!(
        "bundle/en/en_tn_v91.sb.zip:en_tn/ingredients/{}.tsv",
        book.code()
    );
    parse_notes(&repo_text(&entry), TsvScope::Book(book))
        .unwrap()
        .notes
}

fn row<'a, T>(rows: &'a [T], id: &str, id_of: impl Fn(&T) -> &HelpItemId) -> &'a T {
    rows.iter()
        .find(|r| id_of(r).upstream_row_id().is_some_and(|r| r.as_str() == id))
        .unwrap_or_else(|| panic!("no row {id}"))
}

#[test]
fn a_reordered_quote_lights_only_its_own_phrase() {
    // tC4 9ad4d5c's #773 vectors (test/sourceHighlight.test.ts): real en_tn v91 rows against
    // its extract of en_ult v91 Genesis. The quotes are read from the table, so no Hebrew is
    // retyped; tC4's test passes occurrence 1, as each row says.
    let notes = v91_notes(BookId::Genesis);
    let cases = [
        // The shortest run, not every group that names a quote word: GEN 33:5 repeats אֶת
        // ("his eyes … the women … whom God …" before the fix).
        (
            "mix3",
            (33, 5),
            "whom God has graciously given to your servant",
        ),
        ("gyfv", (24, 48), "the daughter of my master s brother"),
        (
            "r2u9",
            (9, 11),
            "that never again will all flesh be cut off",
        ),
        // A word the quote names twice (בֶּן) is needed twice.
        ("zczu", (11, 31), "and Lot Haran s son his son s son"),
        // The run is measured over every group its instances are rendered by: תָּבֹא is
        // "Do … let … enter", so a run from "enter" to the second אַל only looks short.
        ("h9hm", (49, 6), "Do not let … enter into their council"),
    ];
    let mut wrong = Vec::new();
    for (id, (chapter, verse), want) in cases {
        let note = row(&notes, id, |n| &n.id);
        assert_eq!(
            single_verse(&note.reference),
            Some((chapter, verse)),
            "{id}"
        );
        assert_eq!(note.occurrence, Occurrence::nth(1).unwrap(), "{id}");
        let v = fixture_verse("tc4/test-fixtures/en_ult/GEN-773.usfm", chapter, verse);
        let got = gateway_quote(&v, &note.quote, note.occurrence);
        if got.as_deref() != Some(want) {
            wrong.push(format!("{id} {chapter}:{verse}: {got:?}, tC4 {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));

    // mix3 took the fallback: the verse holds אֶת more often than the quote does.
    let mix3 = row(&notes, "mix3", |n| &n.id);
    let v = fixture_verse("tc4/test-fixtures/en_ult/GEN-773.usfm", 33, 5);
    let h = highlight_quote(&v, &mix3.quote, mix3.occurrence);
    assert_eq!(h.quality, MatchQuality::Approximate);

    // The strict match still runs first: en_tn TIT 2:2's τῇ ἀγάπῃ is "in love", whose
    // anchor is the second τῇ (the tightest candidate of its class), never "in faith".
    assert_eq!(
        gateway_quote(
            &ult(2, 2),
            &Quote::new("τῇ ἀγάπῃ"),
            Occurrence::nth(1).unwrap()
        )
        .as_deref(),
        Some("in love")
    );
}

#[test]
fn genesis_maqaf_quotes_title_in_the_ult() {
    // tC4 9ad4d5c's #765 vectors: real en_tn and en_twl v91 rows against en_ult v91 Genesis 1.
    let notes = v91_notes(BookId::Genesis);
    let links = parse_word_links(
        &repo_text("bundle/en/en_twl_v91.sb.zip:en_twl/ingredients/GEN.tsv"),
        TsvScope::Book(BookId::Genesis),
    )
    .unwrap();
    let title = |verse: u16, quote: &Quote| {
        let v = fixture_verse("tc4/test-fixtures/en_ult/GEN-1.usfm", 1, verse);
        gateway_quote(&v, quote, Occurrence::nth(1).unwrap())
    };
    let note = |id: &str, verse: u16| {
        let n = row(&notes, id, |n| &n.id);
        assert_eq!(single_verse(&n.reference), Some((1, verse)), "{id}");
        &n.quote
    };
    let vzz2 = row(&links, "vzz2", |l| &l.id);
    assert_eq!(single_verse(&vzz2.reference), Some((1, 28)));
    let maqaf = [
        (
            note("xxe6", 5),
            5,
            "Then there was evening and there was morning",
        ),
        (note("xfx7", 7), 7, "And it was so"),
        (&vzz2.orig_words, 28, "the earth"),
    ];
    for (quote, verse, want) in maqaf {
        assert!(quote.as_str().contains('\u{05BE}'), "{quote}");
        assert_eq!(title(verse, quote).as_deref(), Some(want), "{quote}");
    }
    // Quotes without a maqaf stay as they were.
    assert_eq!(title(5, note("c9yz", 5)).as_deref(), Some("And God called"));
    assert_eq!(title(5, note("xmx5", 5)).as_deref(), Some("one day"));
}

/// One row of a vector table: the row id, `chapter:verse`, and tC4's title.
type TitleRow = (String, String, Option<String>);

/// The occurrence `CheckItem::title` passes: tC4 reads 0, -1 and an empty cell as the first
/// occurrence (`Math.max(1, Number(occurrence) || 1)`).
fn title_occurrence(occurrence: Occurrence) -> Occurrence {
    match occurrence {
        Occurrence::Nth(_) => occurrence,
        Occurrence::NotTied | Occurrence::All | Occurrence::Unrecorded => Occurrence::NotTied,
    }
}

#[test]
fn every_gateway_title_of_genesis_psalms_and_titus_is_tc4s() {
    let vectors: serde_json::Value =
        serde_json::from_str(&fixture("helps/gateway-title-vectors.json")).unwrap();
    assert_eq!(
        vectors["tc4"]["commit"],
        "9ad4d5c506466819e5b10f255bbb4a36d394e498"
    );
    let tables = vectors["tables"].as_array().unwrap();
    let sources: BTreeSet<&str> = tables.iter().map(|t| t["ult"].as_str().unwrap()).collect();
    let ults: HashMap<&str, String> = sources.into_iter().map(|s| (s, repo_text(s))).collect();
    let index: HashMap<&str, HashMap<(u16, u16), &str>> = ults
        .iter()
        .map(|(s, usfm)| (*s, verse_bodies(usfm)))
        .collect();
    let mut converted: HashMap<&str, HashMap<(u16, u16), AlignedVerse>> = HashMap::new();
    let mut genesis_rows = 0;
    let mut failures = Vec::new();
    for table in tables {
        let (book, helps) = (
            table["book"].as_str().unwrap(),
            table["helps"].as_str().unwrap(),
        );
        let ult = table["ult"].as_str().unwrap();
        let bodies = &index[ult];
        let scope = TsvScope::Book(book.parse::<BookId>().unwrap());
        let tsv = repo_text(table["tsv"].as_str().unwrap());
        let quoted: Vec<(String, (u16, u16), Quote, Occurrence)> = match helps {
            "tn" => parse_notes(&tsv, scope)
                .unwrap()
                .notes
                .into_iter()
                .map(|n| (n.id, n.reference, n.quote, n.occurrence))
                .collect::<Vec<_>>(),
            "twl" => parse_word_links(&tsv, scope)
                .unwrap()
                .into_iter()
                .map(|l| (l.id, l.reference, l.orig_words, l.occurrence))
                .collect(),
            other => panic!("helps {other}"),
        }
        .into_iter()
        .filter_map(|(id, reference, quote, occurrence)| {
            let at = single_verse(&reference)?;
            (!quote.is_empty() && bodies.contains_key(&at)).then(|| {
                let id = id.upstream_row_id().unwrap().as_str().to_owned();
                (id, at, quote, title_occurrence(occurrence))
            })
        })
        .collect();
        let verses = converted.entry(ult).or_default();
        for (_, at, _, _) in &quoted {
            verses.entry(*at).or_insert_with(|| aligned(bodies[at]));
        }

        let started = Instant::now();
        let got: Vec<TitleRow> = quoted
            .iter()
            .map(|(id, at, quote, occurrence)| {
                let title = gateway_quote(&verses[at], quote, *occurrence);
                (id.clone(), format!("{}:{}", at.0, at.1), title)
            })
            .collect();
        let took = started.elapsed();
        eprintln!("{book} {helps}: {} titles in {took:?}", got.len());
        if !cfg!(debug_assertions) && (book, helps) == ("PSA", "tn") {
            // Three times the Check screen's 250 ms open budget (PERFORMANCE rule 5).
            assert!(
                took < Duration::from_millis(750),
                "{book} {helps}: {took:?}"
            );
        }

        let want: Vec<TitleRow> = serde_json::from_value(table["rows"].clone()).unwrap();
        if book == "GEN" {
            genesis_rows += want.len();
            // C34: since #765 no Genesis row falls back to its Hebrew quote.
            assert!(
                want.iter().all(|(_, _, title)| title.is_some()),
                "{book} {helps}"
            );
        }
        let differ: Vec<String> = want
            .iter()
            .zip(&got)
            .filter(|(w, g)| w != g)
            .map(|(w, g)| {
                format!(
                    "  {} {}: tC4 {:?}, here {} {} {:?}",
                    w.0, w.1, w.2, g.0, g.1, g.2
                )
            })
            .collect();
        if want.len() != got.len() || !differ.is_empty() {
            failures.push(format!(
                "{book} {helps}: {} rows, tC4 has {}; {} differ, first:\n{}",
                got.len(),
                want.len(),
                differ.len(),
                differ[..differ.len().min(10)].join("\n")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(genesis_rows, 11_398);
}

// Story 1 of en_obs v9, quoted as tC4's test quotes it, with the en_obs-tn v13 quotes.
const OBS_1_1: &str = "This is how God made everything in the beginning. He created the universe and everything in it in six days. After God created the earth it was dark and empty because he had not yet formed anything in it. But God’s Spirit was there over the water.";
const OBS_1_2: &str = "Then God said, “Let there be light!” And there was light. God saw that the light was good and called it “day.”";

fn plain_lit(text: &str, quote: &str) -> String {
    let tokens = tokenize_plain(text);
    highlight_plain(&tokens, &Quote::new(quote))
        .iter()
        .map(|&i| tokens[i].text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn obs_quotes_are_found_in_the_story_text() {
    let tokens = tokenize_plain(OBS_1_1);
    assert_eq!(
        tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
        OBS_1_1
    );
    assert_eq!(plain_lit(OBS_1_1, "the beginning"), "the beginning");
    assert_eq!(
        plain_lit(OBS_1_1, "he had not yet formed anything in it"),
        "he had not yet formed anything in it"
    );
    assert_eq!(plain_lit(OBS_1_1, "God’s Spirit"), "God’s Spirit");
    let god = highlight_plain(&tokens, &Quote::new("God"));
    assert_eq!(god, [tokens.iter().position(|t| t.text == "God").unwrap()]);
    assert_eq!(
        plain_lit(OBS_1_2, "God said & there was light"),
        "God said there was light"
    );
    assert_eq!(plain_lit(OBS_1_1, "the beginning of nothing"), "");
    assert_eq!(plain_lit(OBS_1_1, ""), "");
}

proptest! {
    #[test]
    fn highlights_are_sorted_word_tokens(
        words in prop::collection::vec(("[a-c]{1,2}", prop::option::of(0usize..6)), 0..30),
        groups in prop::collection::vec(prop::collection::vec(("[a-c]{1,2}", 1u32..3), 1..3), 6),
        quote in "[a-c &…,]{0,12}",
        occurrence in prop_oneof![
            Just(Occurrence::NotTied), Just(Occurrence::All), Just(Occurrence::Unrecorded),
            (1u32..4).prop_map(|n| Occurrence::nth(n).unwrap())
        ],
    ) {
        let mut verse = AlignedVerse::new();
        let ids: Vec<_> = groups
            .iter()
            .map(|g| verse.add_alignment(g.iter().map(|(c, o)| SourceWord::new(c.clone(), *o)).collect()))
            .collect();
        for (text, group) in &words {
            verse.push_word(text.clone(), group.map(|g| ids[g])).unwrap();
            verse.push_text(" ");
        }
        let h = highlight_quote(&verse, &Quote::new(quote.clone()), occurrence);
        prop_assert!(h.tokens.windows(2).all(|w| w[0] < w[1]));
        prop_assert!(h.tokens.iter().all(|&i| matches!(verse.tokens()[i].kind, TokenKind::Word(Some(_)))));
        prop_assert_eq!(h.tokens.is_empty(), h.quality == MatchQuality::None);
        let gl = gateway_quote(&verse, &Quote::new(quote), occurrence);
        prop_assert_eq!(gl.is_none(), h.tokens.is_empty());
    }

    #[test]
    fn a_highlight_lights_only_quote_words_and_each_one_the_verse_renders(
        words in prop::collection::vec(("[a-c]{1,2}", prop::option::of(0usize..6)), 0..30),
        groups in prop::collection::vec(prop::collection::vec(("[a-c]{1,2}", 1u32..3), 1..3), 6),
        quote in "[a-c &…,]{0,12}",
        occurrence in prop_oneof![
            Just(Occurrence::NotTied), Just(Occurrence::All), Just(Occurrence::Unrecorded),
            (1u32..4).prop_map(|n| Occurrence::nth(n).unwrap())
        ],
    ) {
        let mut verse = AlignedVerse::new();
        let ids: Vec<_> = groups
            .iter()
            .map(|g| verse.add_alignment(g.iter().map(|(c, o)| SourceWord::new(c.clone(), *o)).collect()))
            .collect();
        for (text, group) in &words {
            verse.push_word(text.clone(), group.map(|g| ids[g])).unwrap();
            verse.push_text(" ");
        }
        // The quote's words as highlight_quote reads them.
        let qw: BTreeSet<String> = quote
            .split(|c: char| c.is_whitespace() || c == '\u{05BE}')
            .filter(|w| *w != "&" && *w != "…")
            .map(normalize_word)
            .filter(|w| !w.is_empty())
            .collect();
        let held = |i: usize| -> BTreeSet<String> {
            match verse.tokens()[i].kind {
                TokenKind::Word(Some(id)) => verse
                    .sources(id)
                    .unwrap()
                    .iter()
                    .map(|s| normalize_word(&s.content))
                    .collect(),
                _ => BTreeSet::new(),
            }
        };
        let h = highlight_quote(&verse, &Quote::new(quote.clone()), occurrence);
        // Nothing invented: every lit word renders a quote word.
        for &i in &h.tokens {
            prop_assert!(!held(i).is_disjoint(&qw), "token {i} lit for {quote:?}");
        }
        // Nothing lost: unless a one-word quote resolved through its own occurrence number,
        // every quote word the verse renders is lit (the shortest run included).
        if !matches!(h.quality, MatchQuality::None | MatchQuality::WordOccurrence) {
            let lit: BTreeSet<String> = h.tokens.iter().flat_map(|&i| held(i)).collect();
            let rendered: BTreeSet<String> =
                (0..verse.tokens().len()).flat_map(held).filter(|w| qw.contains(w)).collect();
            prop_assert!(rendered.is_subset(&lit), "{quote:?}: {rendered:?} not all in {lit:?}");
        }
    }

    #[test]
    fn plain_tokens_give_the_text_back(text in "\\PC{0,200}", quote in "\\PC{0,30}") {
        let tokens = tokenize_plain(&text);
        prop_assert_eq!(tokens.iter().map(|t| t.text.as_str()).collect::<String>(), text);
        let lit = highlight_plain(&tokens, &Quote::new(quote));
        prop_assert!(lit.iter().all(|&i| tokens[i].word));
    }
}
