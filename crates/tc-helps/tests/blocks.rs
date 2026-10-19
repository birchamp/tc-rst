//! The block renderer against tC4's own (`fixtures/helps/article-block-vectors.json`, made by
//! `tools/gen-article-block-vectors.mjs` from tC4's `renderArticleBlocks` and `takeSpans`):
//! every Titus note of en_tn v91 and es-419_tn v66, two articles and the marks of #619.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use tc_helps::blocks::{Block, BlockKind, Span, js_len, render_article_blocks, take_spans};

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/helps/article-block-vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn spans_of(value: &Value) -> Vec<Span> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Span {
            text: s["text"].as_str().unwrap().to_owned(),
            bold: s.get("bold").is_some(),
            italic: s.get("italic").is_some(),
        })
        .collect()
}

fn block_of(value: &Value) -> Block {
    Block {
        kind: match value["kind"].as_str().unwrap() {
            "h" => BlockKind::Heading,
            "li" => BlockKind::Item,
            "p" => BlockKind::Paragraph,
            other => panic!("kind {other}"),
        },
        level: value["level"].as_u64().map(|l| u8::try_from(l).unwrap()),
        quote: u8::try_from(value["quote"].as_u64().unwrap()).unwrap(),
        spans: spans_of(&value["spans"]),
    }
}

#[test]
fn every_block_is_the_block_tc4_renders() {
    let vectors = vectors();
    let cases = vectors["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 430, "every case of the file is read");
    let mut blocks = 0;
    for case in cases {
        let input = case["input"].as_str().unwrap();
        let want: Vec<Block> = case["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(block_of)
            .collect();
        assert_eq!(render_article_blocks(input), want, "input: {input:?}");
        blocks += want.len();
    }
    assert_eq!(blocks, 608, "every block of the file is compared");
}

#[test]
fn every_cut_is_the_cut_tc4_makes() {
    let vectors = vectors();
    for case in vectors["vectors"].as_array().unwrap() {
        for block in case["blocks"].as_array().unwrap() {
            let spans = spans_of(&block["spans"]);
            let len = js_len(&block_of(block).text());
            assert_eq!(
                take_spans(&spans, len / 2),
                spans_of(&block["half"]),
                "{spans:?}"
            );
            assert_eq!(
                take_spans(&spans, len),
                spans,
                "the whole length keeps every span"
            );
        }
    }
}

#[test]
fn article_shows_no_raw_quote_or_bold_marks() {
    // tC4 #619: a quote's `>` and a bold run's `__` are drawn, never shown as text.
    let blocks = render_article_blocks("> __Paul__ wrote this\n> > to **Titus**");
    assert_eq!(blocks.len(), 2);
    assert_eq!((blocks[0].quote, blocks[1].quote), (1, 2));
    for block in &blocks {
        let text = block.text();
        assert!(
            !text.contains('>') && !text.contains("__") && !text.contains("**"),
            "{text}"
        );
    }
    assert!(blocks[0].spans.iter().any(|s| s.bold && s.text == "Paul"));
    assert!(blocks[1].spans.iter().any(|s| s.bold && s.text == "Titus"));
}

#[test]
fn a_cut_never_splits_a_character_in_two() {
    // JavaScript's slice would keep half of the pair; the cut leaves the character out whole.
    let spans = vec![Span::plain("a𝔸b")];
    assert_eq!(take_spans(&spans, 2), vec![Span::plain("a")]);
    assert_eq!(take_spans(&spans, 3), vec![Span::plain("a𝔸")]);
}
