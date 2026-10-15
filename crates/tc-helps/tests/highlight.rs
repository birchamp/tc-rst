//! Quote highlighting against the real aligned en_ult Titus, with tC4's test vectors.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::{fixture, resource};
use proptest::prelude::*;
use tc_core::BookId;
use tc_helps::highlight::{
    AlignedVerse, MatchQuality, SourceWord, TokenKind, gateway_quote, highlight_plain,
    highlight_quote, tokenize_plain,
};
use tc_helps::tsv::{TsvScope, parse_notes};
use tc_helps::{BibleLocus, Error, HelpRef, Occurrence, Quote};

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
    fn plain_tokens_give_the_text_back(text in "\\PC{0,200}", quote in "\\PC{0,30}") {
        let tokens = tokenize_plain(&text);
        prop_assert_eq!(tokens.iter().map(|t| t.text.as_str()).collect::<String>(), text);
        let lit = highlight_plain(&tokens, &Quote::new(quote));
        prop_assert!(lit.iter().all(|&i| tokens[i].word));
    }
}
