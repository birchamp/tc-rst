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
        depth: 0,
        number: None,
        cells: Vec::new(),
        header: false,
    }
}

/// tC4's block, with #628's one difference these inputs hold: a paragraph that starts with
/// `1. ` is the numbered item 1.
fn rebuild_block(value: &Value) -> Block {
    let mut block = block_of(value);
    if block.kind != BlockKind::Paragraph {
        return block;
    }
    let text = block.text();
    let Some((number, _)) = text.split_once(". ") else {
        return block;
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return block;
    }
    let cut = number.len() + 2;
    let first = &mut block.spans[0];
    assert!(first.text.len() >= cut, "{text}");
    first.text = first.text[cut..].to_owned();
    block.kind = BlockKind::Numbered;
    block.number = Some(number.to_owned());
    block
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
            .map(rebuild_block)
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

#[test]
fn ta_article_with_a_table_renders_a_table() {
    // tC4 #628: a table, a numbered list and a nested list are drawn as such, where tC4 shows
    // their pipes, dashes and indents as plain lines.
    let markdown = "Intro\n\n| Word | **Meaning** |\n|------|:-------:|\n| hesed | loyal love |\n| emet | truth |\nAfter\n\n1. one\n2) two\n- top\n  - under\n    - deeper\n- back";
    let blocks = render_article_blocks(markdown);
    let kinds: Vec<BlockKind> = blocks.iter().map(|b| b.kind).collect();
    use BlockKind::{Item, Numbered, Paragraph, Row};
    assert_eq!(
        kinds,
        [
            Paragraph, Row, Row, Row, Paragraph, Numbered, Numbered, Item, Item, Item, Item
        ]
    );
    let row = |b: &Block| -> Vec<String> {
        b.cells
            .iter()
            .map(|c| c.iter().map(|s| s.text.as_str()).collect())
            .collect()
    };
    assert_eq!(row(&blocks[1]), ["Word", "Meaning"]);
    assert!(blocks[1].header && !blocks[2].header);
    assert!(blocks[1].cells[1][0].bold, "a cell keeps its marks");
    assert_eq!(row(&blocks[3]), ["emet", "truth"]);
    assert_eq!(blocks[3].text(), "emet truth");
    assert_eq!(
        (blocks[5].number.as_deref(), blocks[5].text().as_str()),
        (Some("1"), "one")
    );
    assert_eq!(blocks[6].number.as_deref(), Some("2"));
    let depths: Vec<u8> = blocks[7..].iter().map(|b| b.depth).collect();
    assert_eq!(depths, [0, 1, 2, 0]);
    // A line of pipes with no rule under it stays a paragraph, word for word.
    let lone = render_article_blocks("| not a table |");
    assert_eq!(lone[0].kind, Paragraph);
    assert_eq!(lone[0].text(), "| not a table |");
}
