//! What the helps screens ask of a set of Door43 helps, per passage.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::resource;
use tc_core::{BookId, Reference};
use tc_helps::articles::{academy_article, parse_word_article, word_article_id};
use tc_helps::links::{LinkBase, LinkTarget, find_links, resolve_link};
use tc_helps::metadata::{DcsPin, HelpsSource, parse_burrito_metadata};
use tc_helps::tsv::{TsvScope, parse_notes, parse_questions, parse_word_links};
use tc_helps::{Article, CheckReadiness, CommitSha, Helps, Manual, ObsRef, Slug};

fn reference(text: &str) -> Reference {
    text.parse().unwrap()
}

fn titus_helps() -> Helps {
    let meta = parse_burrito_metadata(&resource("en_tn@v86/metadata.json")).unwrap();
    let pin = DcsPin {
        owner: "unfoldingWord".into(),
        repo: "en_tn".into(),
        tag: Some("v86".into()),
        commit: CommitSha::parse("c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5").unwrap(),
    };
    pin.verify(&meta.identity).unwrap();
    let mut helps = Helps::new(HelpsSource::Dcs(pin));
    let tit = TsvScope::Book(BookId::Titus);
    helps.add_notes(
        parse_notes(&resource("en_tn@v86/TIT.tsv"), tit)
            .unwrap()
            .notes,
    );
    helps.add_word_links(parse_word_links(&resource("en_twl@v86/TIT.tsv"), tit).unwrap());
    let god = word_article_id("payload/kt/god.md").unwrap();
    helps.add_word_articles([parse_word_article(
        god,
        &resource("en_tw@v87/payload-kt-god.md"),
    )]);
    helps.add_academy_articles([academy_article(
        Manual::Translate,
        Slug::parse("figs-abstractnouns").unwrap(),
        &resource("en_ta@v86/translate-figs-abstractnouns-title.md"),
        None,
        &resource("en_ta@v86/translate-figs-abstractnouns-01.md"),
    )]);
    helps
}

#[test]
fn a_verse_gets_its_notes_words_and_linked_articles() {
    let helps = titus_helps();
    let notes: Vec<_> = helps.notes_for(reference("TIT 1:1")).collect();
    assert_eq!(notes.len(), 4);
    assert_eq!(
        notes[0].quote.as_str(),
        "κατὰ πίστιν ἐκλεκτῶν Θεοῦ καὶ ἐπίγνωσιν ἀληθείας"
    );
    let article = helps
        .academy_article(notes[0].support_reference.as_ref().unwrap())
        .unwrap();
    assert_eq!(article.title, "Abstract Nouns");

    let words = helps.words_for(reference("TIT 1:1"));
    let names: Vec<String> = words
        .iter()
        .map(|(id, _)| match id {
            tc_helps::WordArticleId::Dcs { category, slug } => {
                format!("{}/{}", category.folder(), slug)
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        names,
        [
            "names/paul",
            "other/servant",
            "kt/god",
            "kt/apostle",
            "kt/jesus",
            "kt/christ",
            "kt/faith",
            "kt/elect",
            "other/know",
            "kt/godly"
        ]
    );
    let god = words.iter().find(|(_, a)| a.is_some()).unwrap().1.unwrap();
    assert_eq!(god.title, "God");
    // Θεοῦ is linked twice in the verse (occurrences 1 and 2), one article.
    assert_eq!(helps.word_links_for(reference("TIT 1:1")).count(), 11);
    let god_link = helps
        .word_links_for(reference("TIT 1:1"))
        .find(|l| l.orig_words.as_str() == "Θεοῦ")
        .unwrap();
    assert_eq!(helps.word_link_readiness(god_link), CheckReadiness::Ready);

    // A range covers every verse in it.
    assert_eq!(helps.notes_for(reference("TIT 1:1-2")).count(), 4 + 4);
    assert_eq!(helps.notes_for(reference("JON 1:1")).count(), 0);
}

#[test]
fn introductions_are_asked_for_separately() {
    let helps = titus_helps();
    let book: Vec<_> = helps.introductions(BookId::Titus, None).collect();
    assert_eq!(book.len(), 1);
    assert!(book[0].text.starts_with("# Introduction to Titus"));
    assert_eq!(helps.introductions(BookId::Titus, Some(2)).count(), 1);
    assert_eq!(helps.introductions(BookId::Titus, Some(4)).count(), 0);
    // A link in the book introduction opens a verse.
    let link = &find_links(&book[0].text)[0];
    let target = resolve_link(
        &link.href,
        &LinkBase::Note {
            book: BookId::Titus,
            chapter: None,
        },
    )
    .unwrap();
    assert!(matches!(&target, LinkTarget::Bible(r) if r.to_string() == "TIT 1:1"));
    assert_eq!(helps.article(&target), None);
    let nouns = resolve_link(
        "rc://*/ta/man/translate/figs-abstractnouns",
        &LinkBase::Absolute,
    )
    .unwrap();
    assert!(
        matches!(helps.article(&nouns), Some(Article::Academy(a)) if a.title == "Abstract Nouns")
    );
}

#[test]
fn obs_helps_are_asked_for_by_frame() {
    let mut helps = Helps::new(HelpsSource::Dcs(DcsPin {
        owner: "unfoldingWord".into(),
        repo: "en_obs-tn".into(),
        tag: Some("v13".into()),
        commit: CommitSha::parse("e86138ea13f619f09f7a6dcaa60592716d407fe4").unwrap(),
    }));
    helps.add_notes(
        parse_notes(&resource("en_obs-tn@v13/OBS.tsv"), TsvScope::Obs)
            .unwrap()
            .notes,
    );
    helps
        .add_questions(parse_questions(&resource("en_obs-tq@v10/OBS.tsv"), TsvScope::Obs).unwrap());
    let title: Vec<_> = helps.obs_notes_for(ObsRef::new(1, 0).unwrap()).collect();
    assert_eq!(title.len(), 1);
    assert_eq!(title[0].quote.as_str(), "The Creation");
    let frame: Vec<&str> = helps
        .obs_notes_for(ObsRef::new(1, 1).unwrap())
        .map(|n| n.quote.as_str())
        .collect();
    assert_eq!(frame[..3], ["the beginning", "created", "the universe"]);
    // Bible passages never match story frames.
    assert_eq!(helps.notes_for(reference("GEN 1:1")).count(), 0);
    assert_eq!(helps.questions().len(), 672);
}
