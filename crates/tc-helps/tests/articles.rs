//! Translation words and Translation Academy articles, and the links inside them.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{fixture, resource};
use proptest::prelude::*;
use tc_core::BookId;
use tc_helps::articles::{
    academy_article, parse_academy_config, parse_toc, parse_word_article, word_article_id,
};
use tc_helps::links::{LinkBase, LinkTarget, find_links, parse_rc_link, resolve_link};
use tc_helps::{
    AcademyArticleId, BibleLocus, Error, Manual, ObsRef, Slug, WordArticleId, WordCategory,
    YamlFile,
};

fn slug(s: &str) -> Slug {
    Slug::parse(s).unwrap()
}

fn kt(s: &str) -> WordArticleId {
    WordArticleId::Dcs {
        category: WordCategory::KeyTerm,
        slug: slug(s),
    }
}

#[test]
fn a_words_article_reads_title_body_and_every_link() {
    let id = word_article_id("ingredients/payload/kt/god.md").unwrap();
    assert_eq!(id, kt("god"));
    let article = parse_word_article(id, &resource("en_tw@v87/payload-kt-god.md"));
    assert_eq!(article.title, "God");
    assert!(article.body.starts_with("## Definition:\n"));

    let base = LinkBase::WordArticle(WordCategory::KeyTerm);
    let links = find_links(&article.body);
    let targets: Vec<LinkTarget> = links
        .iter()
        .map(|l| resolve_link(&l.href, &base).unwrap_or_else(|e| panic!("{}: {e}", l.href)))
        .collect();
    assert!(targets.contains(&LinkTarget::Word(WordArticleId::Dcs {
        category: WordCategory::Other,
        slug: slug("creation"),
    })));
    assert!(targets.contains(&LinkTarget::Word(kt("falsegod"))));
    assert!(
        targets.contains(&LinkTarget::Academy(AcademyArticleId::Dcs {
            manual: Manual::Translate,
            slug: slug("translate-names"),
        }))
    );
    assert!(targets.contains(&LinkTarget::Obs(ObsRef::new(1, 1).unwrap())));
    let first_bible = targets
        .iter()
        .find_map(|t| match t {
            LinkTarget::Bible(r) => Some(r.to_string()),
            _ => None,
        })
        .unwrap();
    assert_eq!(first_bible, "1JN 1:7");
    // god.md lists 18 Bible references and 12 story frames.
    let bible = targets
        .iter()
        .filter(|t| matches!(t, LinkTarget::Bible(_)))
        .count();
    let frames = targets
        .iter()
        .filter(|t| matches!(t, LinkTarget::Obs(_)))
        .count();
    assert_eq!((bible, frames), (18, 12));
    assert_eq!(
        parse_rc_link("rc://en/tn/help/1jn/01/07")
            .unwrap()
            .language
            .as_deref(),
        Some("en")
    );
}

#[test]
fn an_academy_article_assembles_title_and_body() {
    let title = resource("en_ta@v86/translate-figs-abstractnouns-title.md");
    assert!(!title.ends_with('\n'));
    let body = resource("en_ta@v86/translate-figs-abstractnouns-01.md");
    let article = academy_article(
        Manual::Translate,
        slug("figs-abstractnouns"),
        &title,
        Some("What are abstract nouns?\n"),
        &body,
    );
    assert_eq!(article.title, "Abstract Nouns");
    assert!(article.body.starts_with("### Description"));
    assert!(
        article
            .page()
            .starts_with("# Abstract Nouns\n## What are abstract nouns?\n\n### Description\n")
    );
    let bare = academy_article(Manual::Translate, slug("x"), "X", None, "body");
    assert_eq!(bare.page(), "# X\n\nbody");
}

/// Files saved by Windows editors start with a byte order mark.
fn bom(text: &str) -> String {
    format!("\u{feff}{text}")
}

#[test]
fn a_byte_order_mark_is_not_part_of_any_article_or_yaml_file() {
    let god = resource("en_tw@v87/payload-kt-god.md");
    assert_eq!(
        parse_word_article(kt("god"), &bom(&god)),
        parse_word_article(kt("god"), &god)
    );
    assert_eq!(parse_word_article(kt("god"), &bom(&god)).title, "God");
    let title = resource("en_ta@v86/translate-figs-abstractnouns-title.md");
    let body = resource("en_ta@v86/translate-figs-abstractnouns-01.md");
    let plain = academy_article(Manual::Translate, slug("x"), &title, Some("Q?"), &body);
    let marked = academy_article(
        Manual::Translate,
        slug("x"),
        &bom(&title),
        Some(&bom("Q?")),
        &bom(&body),
    );
    assert_eq!(marked, plain);
    let toc = resource("en_ta@v86/translate-toc.yaml");
    assert_eq!(parse_toc(&bom(&toc)).unwrap(), parse_toc(&toc).unwrap());
    let config = fixture("helps/ta/translate-config.yaml");
    assert_eq!(
        parse_academy_config(&bom(&config)).unwrap(),
        parse_academy_config(&config).unwrap()
    );
}

#[test]
fn the_translate_toc_lists_177_articles_in_reading_order() {
    let toc = parse_toc(&resource("en_ta@v86/translate-toc.yaml")).unwrap();
    assert_eq!(toc.title, "Table of Contents");
    let links = toc.links();
    assert_eq!(links.len(), 177);
    assert_eq!(links[0].as_str(), "translate-manual");
    assert!(links.iter().any(|l| l.as_str() == "figs-abstractnouns"));
    assert_eq!(toc.sections[0].title, "Introduction");
    assert_eq!(toc.sections[0].link, None);
}

#[test]
fn academy_config_lists_dependencies_and_recommendations() {
    let config = parse_academy_config(&fixture("helps/ta/translate-config.yaml")).unwrap();
    assert_eq!(config.len(), 4);
    let nouns = &config[&slug("figs-abstractnouns")];
    assert_eq!(nouns.dependencies, vec![slug("figs-intro")]);
    assert_eq!(
        nouns.recommended,
        vec![slug("figs-activepassive"), slug("figs-metonymy")]
    );
    assert!(config[&slug("figs-activepassive")].dependencies.is_empty());
    assert!(config[&slug("translate-names")].dependencies.is_empty());
}

#[test]
fn broken_yaml_is_refused_with_the_file_named() {
    for (text, file) in [
        ("sections: [", YamlFile::AcademyToc),
        ("sections:\n  - link: x\n", YamlFile::AcademyToc),
        (
            "sections:\n  - title: A\n    link: has space\n",
            YamlFile::AcademyToc,
        ),
    ] {
        match parse_toc(text) {
            Err(Error::Yaml { file: f, .. }) => assert_eq!(f, file),
            other => panic!("{text:?}: {other:?}"),
        }
    }
    assert!(matches!(
        parse_academy_config("- a\n- b\n"),
        Err(Error::Yaml {
            file: YamlFile::AcademyConfig,
            ..
        })
    ));
    assert!(matches!(
        parse_academy_config("figs-x:\n  recommended: 3\n"),
        Err(Error::Yaml {
            file: YamlFile::AcademyConfig,
            ..
        })
    ));
}

#[test]
fn article_paths_and_links_refuse_what_they_cannot_name() {
    for bad in [
        "god.md",
        "payload/animals/dog.md",
        "payload/kt/god.txt",
        "payload/kt/.md",
    ] {
        assert!(word_article_id(bad).is_err(), "{bad}");
    }
    for bad in [
        "rc://*/ta/man/nomanual/x",
        "rc://*/tw/dict/bible/animals/dog",
        "rc://*/tn/help/xyz/01/01",
        "rc://*/tn/help/gen/aa/01",
        "rc://*/ult/chapter/gen/01",
        "ftp://example.org/x",
        "../../../../outside.md",
    ] {
        let base = LinkBase::Note {
            book: BookId::Titus,
            chapter: Some(1),
        };
        assert!(resolve_link(bad, &base).is_err(), "{bad}");
    }
    // The Portuguese manual links its editors' addresses.
    assert_eq!(
        resolve_link("mailto:x@example.org", &LinkBase::Absolute).unwrap(),
        LinkTarget::Mail("x@example.org".into())
    );
    let note = resolve_link(
        "../front/intro.md",
        &LinkBase::Note {
            book: BookId::Titus,
            chapter: Some(2),
        },
    )
    .unwrap();
    assert!(matches!(note, LinkTarget::Bible(r) if r.locus == BibleLocus::BookIntro));
    assert_eq!(
        resolve_link("01/11", &LinkBase::ObsNote).unwrap(),
        LinkTarget::Obs(ObsRef::new(1, 11).unwrap())
    );
    assert_eq!(
        resolve_link(
            "../figs-metaphor/01.md",
            &LinkBase::AcademyArticle {
                manual: Manual::Translate,
                slug: slug("figs-simile")
            }
        )
        .unwrap(),
        LinkTarget::Academy(AcademyArticleId::Dcs {
            manual: Manual::Translate,
            slug: slug("figs-metaphor")
        })
    );
}

/// A toc tree as the generator below writes it: (title, link, children).
#[derive(Debug, Clone)]
struct Node(String, Option<String>, Vec<Node>);

fn toc_tree() -> impl Strategy<Value = Vec<Node>> {
    let leaf = ("[A-Za-z][A-Za-z ,’-]{0,20}", prop::option::of(yaml_slug()))
        .prop_map(|(t, l)| Node(t, l, Vec::new()));
    let tree = leaf.prop_recursive(3, 30, 4, |inner| {
        (
            "[A-Za-z][A-Za-z ]{0,20}",
            prop::option::of(yaml_slug()),
            prop::collection::vec(inner, 1..4),
        )
            .prop_map(|(t, l, kids)| Node(t, l, kids))
    });
    prop::collection::vec(tree, 0..5)
}

fn write_toc(nodes: &[Node], indent: usize, out: &mut String) {
    for Node(title, link, kids) in nodes {
        let pad = " ".repeat(indent);
        out.push_str(&format!("{pad}- title: \"{title}\"\n"));
        if let Some(link) = link {
            out.push_str(&format!("{pad}  link: {link}\n"));
        }
        if !kids.is_empty() {
            out.push_str(&format!("{pad}  sections:\n"));
            write_toc(kids, indent + 4, out);
        }
    }
}

fn same_tree(nodes: &[Node], entries: &[tc_helps::articles::TocEntry]) -> bool {
    nodes.len() == entries.len()
        && nodes.iter().zip(entries).all(|(Node(t, l, kids), e)| {
            *t == e.title
                && l.as_deref() == e.link.as_ref().map(Slug::as_str)
                && same_tree(kids, &e.sections)
        })
}

/// A slug YAML reads as a string: `null` would be a null.
fn yaml_slug() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,12}".prop_filter("YAML null", |s| s != "null")
}

proptest! {
    #[test]
    fn tocs_read_back_as_written(tree in toc_tree()) {
        let mut text = "title: \"Table of Contents\"\nsections:\n".to_owned();
        write_toc(&tree, 2, &mut text);
        let toc = parse_toc(&text).unwrap();
        prop_assert!(same_tree(&tree, &toc.sections), "{text}");
    }

    #[test]
    fn academy_configs_read_back_as_written(
        entries in prop::collection::btree_map(yaml_slug(),
            (prop::collection::vec(yaml_slug(), 0..4), prop::collection::vec(yaml_slug(), 0..4)), 0..8),
    ) {
        let mut text = String::new();
        for (slug, (deps, recs)) in &entries {
            text.push_str(&format!("{slug}:\n"));
            for (field, list) in [("dependencies", deps), ("recommended", recs)] {
                if list.is_empty() {
                    text.push_str(&format!("  {field}: []\n"));
                } else {
                    text.push_str(&format!("  {field}:\n"));
                    for item in list {
                        text.push_str(&format!("    - {item}\n"));
                    }
                }
            }
        }
        if entries.is_empty() {
            text.push_str("{}\n");
        }
        let config = parse_academy_config(&text).unwrap();
        prop_assert_eq!(config.len(), entries.len());
        for (slug, (deps, recs)) in &entries {
            let got = &config[&Slug::parse(slug).unwrap()];
            let names = |v: &Vec<Slug>| v.iter().map(|s| s.as_str().to_owned()).collect::<Vec<_>>();
            prop_assert_eq!(&names(&got.dependencies), deps);
            prop_assert_eq!(&names(&got.recommended), recs);
        }
    }

    #[test]
    fn word_articles_read_back_from_their_page(title in "[^\n#]{1,40}", body in "[^#]{0,200}") {
        let title = title.trim().to_owned();
        prop_assume!(!title.is_empty());
        let page = format!("# {title}\n\n{body}");
        let article = parse_word_article(kt("x"), &page);
        prop_assert_eq!(&article.title, &title);
        prop_assert_eq!(article.body, body.trim_start_matches(['\n', '\r']));
    }

    #[test]
    fn rc_links_resolve_to_what_they_name(cat in prop_oneof![Just("kt"), Just("names"), Just("other")],
                                          name in "[a-z0-9-]{1,20}",
                                          chapter in 1u16..150, verse in 1u16..176) {
        let word = resolve_link(&format!("rc://*/tw/dict/bible/{cat}/{name}"), &LinkBase::Absolute).unwrap();
        let LinkTarget::Word(WordArticleId::Dcs { category, slug }) = word else { panic!() };
        prop_assert_eq!(category.folder(), cat);
        prop_assert_eq!(slug.as_str(), name.as_str());
        let bible = resolve_link(&format!("rc://en/tn/help/gen/{chapter:02}/{verse:02}"), &LinkBase::Absolute).unwrap();
        let LinkTarget::Bible(r) = bible else { panic!() };
        prop_assert_eq!(r.to_string(), format!("GEN {chapter}:{verse}"));
    }

    #[test]
    fn link_scanning_never_panics(text in "\\PC{0,200}") {
        for link in find_links(&text) {
            prop_assert!(text.is_char_boundary(link.span.start) && text.is_char_boundary(link.span.end));
            let _ = resolve_link(&link.href, &LinkBase::ExportRoot);
        }
        let _ = tc_helps::markdown::plain_text(&text);
    }
}
