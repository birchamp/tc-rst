//! What the helps screens ask of a set of Door43 helps, per passage.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::resource;
use tc_core::{BookId, Reference};
use tc_helps::articles::{academy_article, parse_word_article, word_article_id};
use tc_helps::links::{LinkBase, LinkTarget, find_links, resolve_link};
use tc_helps::metadata::{DcsPin, HelpsSource, Revision, parse_burrito_metadata};
use tc_helps::tsv::{TsvScope, parse_notes, parse_questions, parse_word_links};
use tc_helps::{
    Article, CheckBlocker, CheckReadiness, Error, Helps, HelpsPart, Manual, ObsRef, Slug,
};

fn reference(text: &str) -> Reference {
    text.parse().unwrap()
}

/// The pin of a resource fixture, from its own metadata and checked against it.
fn pin(dir: &str) -> HelpsSource {
    let meta = parse_burrito_metadata(&resource(&format!("{dir}/metadata.json"))).unwrap();
    let Some(Revision::Commit(commit)) = meta.identity.revision.clone() else {
        panic!("{dir}: no commit");
    };
    let (repo, tag) = dir.split_once('@').unwrap();
    let pin = DcsPin {
        owner: "unfoldingWord".into(),
        repo: repo.into(),
        tag: Some(tag.into()),
        commit,
    };
    pin.verify(&meta.identity).unwrap();
    HelpsSource::Dcs(pin)
}

fn titus_helps() -> Helps {
    let mut helps = Helps::new();
    let tit = TsvScope::Book(BookId::Titus);
    helps
        .add_notes(
            &pin("en_tn@v86"),
            parse_notes(&resource("en_tn@v86/TIT.tsv"), tit)
                .unwrap()
                .notes,
        )
        .unwrap();
    helps
        .add_word_links(
            &pin("en_twl@v86"),
            parse_word_links(&resource("en_twl@v86/TIT.tsv"), tit).unwrap(),
        )
        .unwrap();
    let god = word_article_id("payload/kt/god.md").unwrap();
    helps
        .add_word_articles(
            &pin("en_tw@v87"),
            [parse_word_article(
                god,
                &resource("en_tw@v87/payload-kt-god.md"),
            )],
        )
        .unwrap();
    helps
        .add_academy_articles(
            &pin("en_ta@v86"),
            [academy_article(
                Manual::Translate,
                Slug::parse("figs-abstractnouns").unwrap(),
                &resource("en_ta@v86/translate-figs-abstractnouns-title.md"),
                None,
                &resource("en_ta@v86/translate-figs-abstractnouns-01.md"),
            )],
        )
        .unwrap();
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
    // Each answer names the repository and commit it came from.
    assert_eq!(notes[0].source, &pin("en_tn@v86"));
    assert_eq!(article.source, &pin("en_ta@v86"));

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
    assert_eq!(god.source, &pin("en_tw@v87"));
    // Θεοῦ is linked twice in the verse (occurrences 1 and 2), one article.
    assert_eq!(helps.word_links_for(reference("TIT 1:1")).count(), 11);
    let god_link = helps
        .word_links_for(reference("TIT 1:1"))
        .find(|l| l.orig_words.as_str() == "Θεοῦ")
        .unwrap();
    assert_eq!(god_link.source, &pin("en_twl@v86"));
    assert_eq!(helps.word_link_readiness(&god_link), CheckReadiness::Ready);
    // The readiness is judged against the en_tw pin, which has only the god article.
    let servant = helps
        .word_links_for(reference("TIT 1:1"))
        .find(|l| l.orig_words.as_str() == "δοῦλος")
        .unwrap();
    assert_eq!(
        helps.word_link_readiness(&servant),
        CheckReadiness::Blocked(vec![CheckBlocker::ArticleMissing])
    );
    assert_eq!(
        helps.source(HelpsPart::WordArticles),
        Some(&pin("en_tw@v87"))
    );

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
fn each_part_keeps_one_pin() {
    let mut helps = titus_helps();
    for (part, dir) in [
        (HelpsPart::Notes, "en_tn@v86"),
        (HelpsPart::WordLinks, "en_twl@v86"),
        (HelpsPart::WordArticles, "en_tw@v87"),
        (HelpsPart::AcademyArticles, "en_ta@v86"),
    ] {
        assert_eq!(helps.source(part), Some(&pin(dir)), "{part}");
    }
    assert_eq!(helps.source(HelpsPart::Questions), None);
    // Notes from another release of en_tn would be answered under the wrong pin.
    let v91 = parse_notes(
        &resource("en_tn@v91/TIT.tsv"),
        TsvScope::Book(BookId::Titus),
    )
    .unwrap()
    .notes;
    let before = helps.notes().len();
    match helps.add_notes(&pin("en_tn@v91"), v91) {
        Err(Error::PinConflict {
            part,
            held,
            offered,
        }) => {
            assert_eq!(part, HelpsPart::Notes);
            assert_eq!(*held, pin("en_tn@v86"));
            assert_eq!(*offered, pin("en_tn@v91"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(helps.notes().len(), before);
    // The same pin adds more of its own books.
    let jonah = parse_notes(
        &resource("en_tn@v86/JON.tsv"),
        TsvScope::Book(BookId::Jonah),
    )
    .unwrap()
    .notes;
    helps.add_notes(&pin("en_tn@v86"), jonah).unwrap();
    assert_eq!(helps.notes_for(reference("JON 1:1")).count(), 3);
}

#[test]
fn obs_helps_are_asked_for_by_frame() {
    let mut helps = Helps::new();
    helps
        .add_notes(
            &pin("en_obs-tn@v13"),
            parse_notes(&resource("en_obs-tn@v13/OBS.tsv"), TsvScope::Obs)
                .unwrap()
                .notes,
        )
        .unwrap();
    helps
        .add_questions(
            &pin("en_obs-tq@v10"),
            parse_questions(&resource("en_obs-tq@v10/OBS.tsv"), TsvScope::Obs).unwrap(),
        )
        .unwrap();
    helps
        .add_word_links(
            &pin("en_obs-twl@v3"),
            parse_word_links(&resource("en_obs-twl@v3/OBS.tsv"), TsvScope::Obs).unwrap(),
        )
        .unwrap();
    let frame = |story, frame| ObsRef::new(story, frame).unwrap();
    let title: Vec<_> = helps.obs_notes_for(frame(1, 0)).collect();
    assert_eq!(title.len(), 1);
    assert_eq!(title[0].quote.as_str(), "The Creation");
    assert_eq!(title[0].source, &pin("en_obs-tn@v13"));
    let quotes: Vec<&str> = helps
        .obs_notes_for(frame(1, 1))
        .map(|n| n.item.quote.as_str())
        .collect();
    assert_eq!(quotes[..3], ["the beginning", "created", "the universe"]);

    let questions: Vec<&str> = helps
        .obs_questions_for(frame(1, 1))
        .map(|q| q.item.question.as_str())
        .collect();
    assert_eq!(
        questions,
        [
            "Where did everything in the universe come from?",
            "How long did it take God to create everything?"
        ]
    );
    assert_eq!(
        helps.obs_questions_for(frame(1, 2)).next().unwrap().source,
        &pin("en_obs-tq@v10")
    );
    assert_eq!(helps.obs_questions_for(frame(1, 3)).count(), 0);

    let words: Vec<&str> = helps
        .obs_word_links_for(frame(1, 1))
        .map(|l| l.item.orig_words.as_str())
        .collect();
    assert_eq!(words, ["God", "Spirit"]);
    let articles = helps.obs_words_for(frame(1, 2));
    assert_eq!(articles.len(), 2);
    // No words articles are loaded, so the links cannot yet be checks.
    let god = helps.obs_word_links_for(frame(1, 1)).next().unwrap();
    assert_eq!(god.source, &pin("en_obs-twl@v3"));
    assert_eq!(
        helps.word_link_readiness(&god),
        CheckReadiness::Blocked(vec![CheckBlocker::WordsNotLoaded])
    );
    assert_eq!(
        helps.source(HelpsPart::ObsNotes),
        Some(&pin("en_obs-tn@v13"))
    );
    assert_eq!(helps.source(HelpsPart::Notes), None);

    // Bible passages never match story frames.
    assert_eq!(helps.notes_for(reference("GEN 1:1")).count(), 0);
    assert_eq!(helps.questions_for(reference("GEN 1:1")).count(), 0);
    assert_eq!(helps.questions().len(), 672);
}
