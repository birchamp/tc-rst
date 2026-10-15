//! Aquifer release JSON converted into the helps model, against the Titus fixtures.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use common::{aquifer, resource};
use proptest::prelude::*;
use tc_core::{BookId, Reference};
use tc_helps::aquifer::{
    AquiferArticle, AquiferHelpsKind, AquiferOrder, IndexKey, academy_from_aquifer,
    notes_from_aquifer, parse_aquifer_articles, parse_aquifer_metadata, questions_from_aquifer,
    words_from_aquifer,
};
use tc_helps::links::{LinkBase, LinkTarget, find_links, resolve_link};
use tc_helps::markdown::{html_to_markdown, plain_text};
use tc_helps::metadata::{AquiferPin, HelpsSource};
use tc_helps::tsv::{TsvScope, parse_notes};
use tc_helps::{
    AcademyArticleId, AquiferContentId, CheckBlocker, CheckReadiness, CommitSha, Error, HelpRef,
    Helps, JsonFile, NotACheck, Note, Occurrence, PinMismatch,
};

fn articles(rel: &str) -> Vec<AquiferArticle> {
    parse_aquifer_articles(&aquifer(rel)).unwrap()
}

fn id(s: &str) -> AquiferContentId {
    AquiferContentId::parse(s).unwrap()
}

fn tit(chapter: u16, verse: u16) -> Reference {
    format!("TIT {chapter}:{verse}").parse().unwrap()
}

fn pin(code: &str, language: &str, commit: &str) -> AquiferPin {
    AquiferPin {
        resource_code: code.into(),
        language: language.into(),
        tag: "v2026-10-12".into(),
        commit: CommitSha::parse(commit).unwrap(),
        asset_sha256: None,
    }
}

const NOTES_COMMIT: &str = "dbfb46e027ee0807563918f4b95c80879cc32c60";

fn quotes_by_verse(notes: &[Note]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in notes.iter().filter(|n| !n.reference.is_introduction()) {
        out.entry(n.reference.tsv_text())
            .or_default()
            .push(n.quote.as_str().to_owned());
    }
    out
}

#[test]
fn english_notes_carry_the_same_quotes_as_door43_en_tn_v91_in_the_same_order() {
    let notes =
        notes_from_aquifer(&articles("UWTranslationNotes/eng/json/56.content.json")).unwrap();
    assert_eq!(notes.len(), 206);
    assert_eq!(
        notes
            .iter()
            .filter(|n| n.reference.is_introduction())
            .count(),
        4
    );
    assert_eq!(notes.iter().filter(|n| !n.quote.is_empty()).count(), 202);
    assert_eq!(notes.iter().filter(|n| n.gl_quote.is_some()).count(), 202);
    // Aquifer built these from en_tn v91. Sorted by verse and by each title's `(#n)`, the
    // n-th note on a verse is the n-th row on it, quote for quote.
    let door43 = parse_notes(
        &resource("en_tn@v91/TIT.tsv"),
        TsvScope::Book(BookId::Titus),
    )
    .unwrap()
    .notes;
    assert_eq!(quotes_by_verse(&notes), quotes_by_verse(&door43));

    let first = notes
        .iter()
        .find(|n| n.reference.tsv_text() == "1:1")
        .unwrap();
    assert_eq!(
        first.quote.as_str(),
        "κατὰ πίστιν ἐκλεκτῶν Θεοῦ, καὶ ἐπίγνωσιν ἀληθείας"
    );
    assert_eq!(
        first.gl_quote.as_deref(),
        Some("for the faith of the chosen people of God and knowledge of the truth")
    );
    assert!(
        first
            .text
            .starts_with("The words **faith**, **knowledge**, and **truth** are abstract nouns.")
    );
    assert!(
        first
            .text
            .ends_with("See: [Abstract Nouns](aquifer:resource/UWTranslationManual/eng/33257)")
    );
    assert_eq!(
        first.support_reference,
        Some(AcademyArticleId::Aquifer(id("33257")))
    );
    let intro = notes
        .iter()
        .find(|n| n.reference.tsv_text() == "front:intro")
        .unwrap();
    assert!(
        intro
            .text
            .starts_with("# Introduction to Titus\n\n## Part 1: General Introduction")
    );
    // The one note that keeps its Door43 row id.
    let kept: Vec<_> = notes
        .iter()
        .filter_map(|n| n.id.upstream_row_id())
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].as_str(), "r2gj");
}

#[test]
fn no_aquifer_note_can_drive_a_check_until_its_occurrence_is_known() {
    let notes =
        notes_from_aquifer(&articles("UWTranslationNotes/eng/json/56.content.json")).unwrap();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for n in &notes {
        *tally
            .entry(format!("{:?}", n.check_readiness()))
            .or_default() += 1;
    }
    let blocked = format!(
        "{:?}",
        CheckReadiness::Blocked(vec![CheckBlocker::OccurrenceUnrecorded])
    );
    assert_eq!(tally.get(&blocked), Some(&157), "{tally:?}");
    assert_eq!(
        tally.get(&format!(
            "{:?}",
            CheckReadiness::NotACheck(NotACheck::PlainNote)
        )),
        Some(&45)
    );
    assert_eq!(tally.get(&format!("{:?}", CheckReadiness::Ready)), None);
    assert!(
        notes
            .iter()
            .filter(|n| !n.reference.is_introduction())
            .all(|n| n.occurrence == Occurrence::Unrecorded)
    );
}

#[test]
fn portuguese_notes_keep_the_original_quotes_and_link_across_languages() {
    let eng = notes_from_aquifer(&articles("UWTranslationNotes/eng/json/56.content.json")).unwrap();
    let por = notes_from_aquifer(&articles("UWTranslationNotes/por/json/56.content.json")).unwrap();
    assert_eq!(por.len(), 206);
    assert_eq!(por.iter().filter(|n| !n.quote.is_empty()).count(), 201);
    // Same order, same original-language quote, wherever Portuguese has one.
    for (p, e) in por.iter().zip(&eng) {
        assert_eq!(p.reference, e.reference);
        if !p.quote.is_empty() {
            assert_eq!(p.quote, e.quote);
        }
    }
    // por TIT 1:2 #4 has no original-language quote, only the Portuguese one.
    let no_quote = por
        .iter()
        .find(|n| !n.reference.is_introduction() && n.quote.is_empty())
        .unwrap();
    assert_eq!(
        no_quote.gl_quote.as_deref(),
        Some("antes do início dos tempos")
    );
    assert_eq!(
        no_quote.text,
        "Tradução alternativa: “antes de todos os períodos do tempo”"
    );

    let eng_meta =
        parse_aquifer_metadata(&aquifer("UWTranslationNotes/eng/metadata.json")).unwrap();
    let por_id = &eng_meta.localizations[&id("140738")]["por"];
    assert_eq!(por_id, &id("191432"));
    let find = |notes: &[Note], wanted: &AquiferContentId| {
        notes
            .iter()
            .find(|n| n.id == tc_helps::HelpItemId::Aquifer(wanted.clone()))
            .cloned()
            .unwrap()
    };
    assert_eq!(find(&por, por_id).quote, find(&eng, &id("140738")).quote);
}

#[test]
fn every_link_in_converted_text_resolves() {
    let mut count = 0;
    for rel in [
        "UWTranslationNotes/eng/json/56.content.json",
        "UWTranslationNotes/por/json/56.content.json",
    ] {
        for n in notes_from_aquifer(&articles(rel)).unwrap() {
            for link in find_links(&n.text) {
                resolve_link(&link.href, &LinkBase::Absolute)
                    .unwrap_or_else(|e| panic!("{rel} {:?}: {e}", n.id));
                count += 1;
            }
        }
    }
    for rel in [
        "UWTranslationWords/eng/json/tit-1-1.content.json",
        "UWTranslationWords/por/json/tit-1-1.content.json",
        "UWTranslationManual/eng/json/subset.content.json",
        "UWTranslationManual/por/json/subset.content.json",
    ] {
        for a in articles(rel) {
            for link in find_links(&html_to_markdown(&a.html)) {
                resolve_link(&link.href, &LinkBase::Absolute)
                    .unwrap_or_else(|e| panic!("{rel} {}: {e}", a.content_id));
                count += 1;
            }
        }
    }
    // Every `<data class="resource-ref|bible-ref">` and `<a href>` tag of the fixtures.
    assert_eq!(count, 439);
    let jude = resolve_link("aquifer:bible/65001017-65001019", &LinkBase::Absolute).unwrap();
    assert!(matches!(jude, LinkTarget::Bible(r) if r.to_string() == "JUD 1:17-19"));
}

#[test]
fn questions_split_into_question_and_answer() {
    for lang in ["eng", "por"] {
        let qs = questions_from_aquifer(&articles(&format!(
            "UWTranslationQuestions/{lang}/json/56.content.json"
        )))
        .unwrap();
        assert_eq!(qs.len(), 36);
        assert!(
            qs.iter()
                .all(|q| q.question.ends_with('?') && !q.response.is_empty()),
            "{lang}"
        );
    }
    let qs = questions_from_aquifer(&articles("UWTranslationQuestions/eng/json/56.content.json"))
        .unwrap();
    assert_eq!(qs[0].reference.tsv_text(), "1:1");
    assert_eq!(
        qs[0].question,
        "What was Paul’s purpose in his service to God?"
    );
    assert_eq!(
        qs[0].response,
        "His purpose was to establish the faith of God’s chosen people and to establish the knowledge of the truth."
    );
}

#[test]
fn words_show_per_verse_but_cannot_drive_a_check() {
    let words = words_from_aquifer(&articles(
        "UWTranslationWords/eng/json/tit-1-1.content.json",
    ));
    assert_eq!(words.articles.len(), 5);
    assert_eq!(words.links.len(), 84 + 125 + 227 + 52 + 176);
    let apostle = &words.articles[0];
    assert_eq!(apostle.title, "apostle, apostleship");
    assert!(
        apostle
            .body
            .starts_with("## Definition:\n\nThe “apostles” were men sent by Jesus")
    );
    assert!(
        apostle
            .body
            .contains("* [Jude 1:17–19](aquifer:bible/65001017-65001019)")
    );

    let mut helps = Helps::new(HelpsSource::Aquifer(pin(
        "UWTranslationWords",
        "eng",
        "f8ebe522a769f0f5311a5915d1d61f9e28e6b617",
    )));
    helps.add_word_articles(words.articles.clone());
    helps.add_word_links(words.links.clone());
    let here = helps.words_for(tit(1, 1));
    assert_eq!(here.len(), 5);
    assert!(here.iter().all(|(_, article)| article.is_some()));
    let link = helps.word_links_for(tit(1, 1)).next().unwrap();
    assert_eq!(
        helps.word_link_readiness(link),
        CheckReadiness::Blocked(vec![
            CheckBlocker::NoOriginalWords,
            CheckBlocker::OccurrenceUnrecorded
        ])
    );
    let titles = |r| -> Vec<String> {
        helps
            .words_for(r)
            .iter()
            .map(|(_, a)| a.unwrap().title.clone())
            .collect()
    };
    assert_eq!(titles(tit(3, 15)), ["faith"]);
    assert_eq!(
        titles(tit(2, 12)),
        ["godly, godliness, ungodly, godless, ungodliness, godlessness"]
    );
    assert!(titles(tit(3, 13)).is_empty());

    let por = words_from_aquifer(&articles(
        "UWTranslationWords/por/json/tit-1-1.content.json",
    ));
    let titles: Vec<&str> = por.articles.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, ["Apóstolo", "Eleito", "Fé", "Piedoso", "Paulo"]);
    assert!(por.articles[1].body.starts_with(
        "# escolhido, escolher, povo escolhido, o Escolhido, eleito\n\n## Definição:"
    ));
}

#[test]
fn academy_articles_convert_every_element_the_manual_uses() {
    let eng = academy_from_aquifer(&articles(
        "UWTranslationManual/eng/json/subset.content.json",
    ));
    assert_eq!(eng.len(), 8);
    let body = |title: &str| {
        eng.iter()
            .find(|a| a.title == title)
            .map(|a| a.body.clone())
            .unwrap_or_else(|| panic!("{title}"))
    };
    assert!(body("Abstract Nouns").starts_with("### Description\n\nAbstract nouns are nouns"));
    assert!(body("Abstract Nouns").contains("\n\n> From **childhood** you have known"));
    assert!(body("Biblical Money").contains("| Unit in OT | Metal | Weight |"));
    assert!(body("File Formats").contains("```\n\\c 1\n\\p\n\\v 1 In the beginning was the Word"));
    assert!(body("The Translation Process").contains("!["));
    for a in &eng {
        assert!(a.subtitle.is_none());
        for tag in [
            "<p", "<data", "<strong", "<table", "<br", "<sup", "&quot;", "&amp;",
        ] {
            assert!(!a.body.contains(tag), "{} keeps {tag}", a.title);
        }
    }
    let por = academy_from_aquifer(&articles(
        "UWTranslationManual/por/json/subset.content.json",
    ));
    assert_eq!(por.len(), 8);

    // Portuguese notes link the Portuguese academy articles by id.
    let mut helps = Helps::new(HelpsSource::Aquifer(pin(
        "UWTranslationNotes",
        "por",
        NOTES_COMMIT,
    )));
    helps.add_academy_articles(por);
    helps.add_notes(
        notes_from_aquifer(&articles("UWTranslationNotes/por/json/56.content.json")).unwrap(),
    );
    let first = helps.notes_for(tit(1, 1)).next().unwrap();
    let article = helps
        .academy_article(first.support_reference.as_ref().unwrap())
        .unwrap();
    assert_eq!(article.title, "Substantivos abstratos");
    let target = resolve_link(&find_links(&first.text)[0].href, &LinkBase::Absolute).unwrap();
    assert!(
        matches!(helps.article(&target), Some(tc_helps::Article::Academy(a)) if a.title == "Substantivos abstratos")
    );
}

/// The visible text of HTML: tags dropped, the entities Aquifer uses decoded.
fn html_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let mut decoded = out
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", "\u{a0}")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    while let Some(at) = decoded.find("&#") {
        let end = at + decoded[at..].find(';').unwrap();
        let c = char::from_u32(decoded[at + 2..end].parse().unwrap()).unwrap();
        decoded.replace_range(at..=end, &c.to_string());
    }
    decoded.replace("&amp;", "&")
}

fn non_space(text: &str) -> Vec<char> {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn is_subsequence(needle: &[char], hay: &[char]) -> bool {
    let mut it = hay.iter();
    needle.iter().all(|c| it.any(|h| h == c))
}

#[test]
fn conversion_keeps_every_character_of_text() {
    for rel in [
        "UWTranslationNotes/eng/json/56.content.json",
        "UWTranslationNotes/por/json/56.content.json",
        "UWTranslationQuestions/eng/json/56.content.json",
        "UWTranslationQuestions/por/json/56.content.json",
        "UWTranslationWords/eng/json/tit-1-1.content.json",
        "UWTranslationWords/por/json/tit-1-1.content.json",
        "UWTranslationManual/eng/json/subset.content.json",
        "UWTranslationManual/por/json/subset.content.json",
    ] {
        for a in articles(rel) {
            let source = non_space(&html_text(&a.html));
            let converted = non_space(&plain_text(&html_to_markdown(&a.html)));
            assert!(
                is_subsequence(&source, &converted),
                "{rel} {} lost text",
                a.content_id
            );
            // What is added is only table pipes, fences, list numbers and image text.
            assert!(
                converted.len() <= source.len() + source.len() / 10 + 60,
                "{rel} {}",
                a.content_id
            );
        }
    }
    // The notes model keeps the quotes outside the body; together they hold every character.
    let raw = articles("UWTranslationNotes/eng/json/56.content.json");
    let by_id: BTreeMap<_, _> = raw.iter().map(|a| (a.content_id.clone(), a)).collect();
    for n in notes_from_aquifer(&raw).unwrap() {
        let tc_helps::HelpItemId::Aquifer(cid) = &n.id else {
            panic!()
        };
        let source = non_space(&html_text(&by_id[cid].html));
        let gl = n
            .gl_quote
            .as_ref()
            .map(|g| format!("\"{g}\""))
            .unwrap_or_default();
        let whole = format!("{} {gl} {}", n.quote, plain_text(&n.text));
        assert!(is_subsequence(&source, &non_space(&whole)), "{cid}");
    }
}

#[test]
fn metadata_names_the_resource_and_its_attribution() {
    let eng = parse_aquifer_metadata(&aquifer("UWTranslationNotes/eng/metadata.json")).unwrap();
    assert_eq!(eng.kind(), Some(AquiferHelpsKind::Notes));
    assert_eq!(eng.order, AquiferOrder::Canonical);
    assert_eq!(eng.language, "eng");
    assert_eq!(eng.license.copyright, "© 2022 unfoldingWord");
    assert_eq!(
        eng.license.licenses,
        vec![(
            "CC BY-SA 4.0 license".to_owned(),
            "https://creativecommons.org/licenses/by-sa/4.0/legalcode.en".to_owned()
        )]
    );
    assert!(!eng.shows_adaptation_notice());
    assert!(!eng.attribution(None).contains("Mission Mutual"));

    let por = parse_aquifer_metadata(&aquifer("UWTranslationNotes/por/metadata.json")).unwrap();
    assert!(por.shows_adaptation_notice());
    let p = pin("UWTranslationNotes", "por", NOTES_COMMIT);
    let text = por.attribution(Some(&p));
    assert!(text.contains("**unfoldingWord® Translation Notes**"));
    assert!(text.contains("Released under CC BY-SA 4.0 license by Mission Mutual"));
    assert!(text.contains(NOTES_COMMIT));
    p.verify(&por).unwrap();
    assert!(matches!(
        p.verify(&eng),
        Err(Error::PinMismatch(PinMismatch::Language { .. }))
    ));
    assert!(matches!(
        pin("UWTranslationWords", "por", NOTES_COMMIT).verify(&por),
        Err(Error::PinMismatch(PinMismatch::Repository { .. }))
    ));

    for (rel, kind, order) in [
        (
            "UWTranslationWords/eng/metadata.json",
            AquiferHelpsKind::Words,
            AquiferOrder::Alphabetical,
        ),
        (
            "UWTranslationManual/por/metadata.json",
            AquiferHelpsKind::Academy,
            AquiferOrder::Monograph,
        ),
        (
            "UWTranslationQuestions/por/metadata.json",
            AquiferHelpsKind::Questions,
            AquiferOrder::Canonical,
        ),
    ] {
        let meta = parse_aquifer_metadata(&aquifer(rel)).unwrap();
        assert_eq!((meta.kind(), meta.order), (Some(kind), order), "{rel}");
    }
}

#[test]
fn malformed_aquifer_json_is_refused_naming_the_article_and_field() {
    let good = r#"{"content_id": 7, "title": "Titus 1:1 (#1)", "index_reference": "56001001", "language": "eng", "content": "<p>x</p>"}"#;
    let parsed = parse_aquifer_articles(&format!("[{good}]")).unwrap();
    assert_eq!(parsed[0].content_id, id("7"));
    assert_eq!(parsed[0].ordinal(), Some(1));
    assert!(matches!(&parsed[0].index, IndexKey::Passage(r) if r.to_string() == "TIT 1:1"));

    let cases = [
        (good.replace("56001001", "67001001"), "index_reference"),
        (good.replace("56001001", "56001001-57001001"), "index_reference"),
        (
            good.replace(
                "\"content\"",
                r#""associations": {"passage": [{"start_ref": "56001001", "end_ref": "57001001"}]}, "content""#,
            ),
            "associations.passage",
        ),
        (good.replace("7,", "\"has space\","), "content_id"),
    ];
    for (json, field) in cases {
        match parse_aquifer_articles(&format!("[{json}]")) {
            Err(Error::AquiferArticle {
                field: f,
                content_id,
                ..
            }) => {
                assert_eq!(f, field);
                assert!(!content_id.is_empty());
            }
            other => panic!("{json}: {other:?}"),
        }
    }
    assert!(matches!(
        parse_aquifer_articles("{\"not\": \"a list\"}"),
        Err(Error::Json {
            file: JsonFile::AquiferArticles,
            ..
        })
    ));
    assert!(matches!(
        parse_aquifer_articles(&format!(
            "[{}]",
            good.replace("\"title\": \"Titus 1:1 (#1)\",", "")
        )),
        Err(Error::Json { .. })
    ));
    assert!(matches!(
        parse_aquifer_metadata("{\"resource_metadata\": {}}"),
        Err(Error::Json {
            file: JsonFile::AquiferMetadata,
            ..
        })
    ));
    // A note filed under an alphabetical key cannot be placed in a book.
    let sorted =
        parse_aquifer_articles(&format!("[{}]", good.replace("56001001", "apostle"))).unwrap();
    assert!(notes_from_aquifer(&sorted).is_err());
}

#[test]
fn notes_and_questions_land_on_their_verses() {
    let mut helps = Helps::new(HelpsSource::Aquifer(pin(
        "UWTranslationNotes",
        "eng",
        NOTES_COMMIT,
    )));
    helps.add_notes(
        notes_from_aquifer(&articles("UWTranslationNotes/eng/json/56.content.json")).unwrap(),
    );
    helps.add_questions(
        questions_from_aquifer(&articles("UWTranslationQuestions/eng/json/56.content.json"))
            .unwrap(),
    );
    assert_eq!(helps.notes_for(tit(1, 1)).count(), 4);
    assert_eq!(helps.introductions(BookId::Titus, None).count(), 1);
    assert_eq!(helps.introductions(BookId::Titus, Some(3)).count(), 1);
    assert_eq!(helps.questions_for(tit(1, 1)).count(), 1);
    let span: Reference = "TIT 1:1-4".parse().unwrap();
    assert!(helps.notes_for(span).count() > 4);
    assert!(
        helps
            .notes_for(tit(1, 1))
            .all(|n| matches!(&n.reference, HelpRef::Bible(r) if r.to_string() == "TIT 1:1"))
    );
}

proptest! {
    #[test]
    fn html_conversion_never_panics_and_keeps_paragraph_text(text in "[^<>&]{0,200}", html in "\\PC{0,200}") {
        let _ = html_to_markdown(&html);
        let md = html_to_markdown(&format!("<p>{text}</p>"));
        let plain = plain_text(&md);
        let want: Vec<&str> = text.split_whitespace().collect();
        let got: Vec<&str> = plain.split_whitespace().collect();
        prop_assert_eq!(got, want);
    }
}
