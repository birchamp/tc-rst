//! Evidence for tC4 #628 (fixed-in-4.1 check, 2026-10-29). Goes at
//! `crates/tc-helps/tests/repro_628.rs` (it uses `tests/common/mod.rs`). Run:
//! `tc-heavy cargo test -p tc-helps --test repro_628 -- --nocapture`
//!
//! The issue's named articles from the bundled en_ta v91, through the renderer the article
//! pane uses (`render_article_blocks`, called by `tc_app::understand::blocks`), and a scan of
//! every en_ta and en_tw article for marks left as text.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

mod common;

use std::io::Read;
use std::path::PathBuf;

use tc_helps::blocks::{Block, BlockKind, render_article_blocks};

fn articles(zip: &str) -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en").join(zip);
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let names: Vec<String> = archive
        .file_names()
        .filter(|n| n.ends_with(".md") && !n.ends_with("title.md") && !n.ends_with("sub-title.md"))
        .map(str::to_owned)
        .collect();
    let mut out = Vec::new();
    for name in names {
        let mut text = String::new();
        archive.by_name(&name).unwrap().read_to_string(&mut text).unwrap();
        out.push((name, text));
    }
    out.sort();
    out
}

fn texts(blocks: &[Block]) -> Vec<String> {
    blocks.iter().map(Block::text).collect()
}

#[test]
fn repro_628_named_articles() {
    let ta = articles("en_ta_v91.sb.zip");
    let get = |slug: &str| {
        let key = format!("en_ta/ingredients/translate/{slug}/01.md");
        render_article_blocks(&ta.iter().find(|(n, _)| *n == key).unwrap().1)
    };
    let table = get("grammar-connect-time-background");
    let rows = table.iter().filter(|b| b.kind == BlockKind::Row).count();
    let headers = table.iter().filter(|b| b.kind == BlockKind::Row && b.header).count();
    let pipe_lines = texts(&table).iter().filter(|t| t.starts_with('|')).count();
    println!("TABLE grammar-connect-time-background: rows {rows}, header rows {headers}, cells in first row {}, text lines starting with '|': {pipe_lines}",
        table.iter().find(|b| b.kind == BlockKind::Row).map_or(0, |b| b.cells.len()));

    for (slug, tag) in [("writing-proverbs", "<br>"), ("translate-names", "<sup>")] {
        let t = texts(&get(slug));
        let left = t.iter().filter(|s| s.contains(tag)).count();
        println!("HTML {slug}: blocks with {tag} as text: {left}");
        if let Some(s) = t.iter().find(|s| s.contains(tag)) {
            println!("  e.g. {:?}", s.chars().take(120).collect::<String>());
        }
    }

    let ex = texts(&get("figs-exclusive"));
    let images = ex.iter().filter(|s| s.contains("![") || s.starts_with('!')).count();
    println!("IMAGE figs-exclusive: blocks with image Markdown as text: {images}");
    if let Some(s) = ex.iter().find(|s| s.contains("![") || s.starts_with('!')) {
        println!("  e.g. {:?}", s.chars().take(160).collect::<String>());
    }

    let pr = get("figs-pronouns");
    let depths: Vec<u8> = pr.iter().filter(|b| b.kind == BlockKind::Item).map(|b| b.depth).collect();
    println!("LIST figs-pronouns: item depths {depths:?}");
    let numbered = ta
        .iter()
        .map(|(_, md)| render_article_blocks(md))
        .filter(|b| b.iter().any(|x| x.kind == BlockKind::Numbered))
        .count();
    let link = render_article_blocks("See [Pronouns](../figs-pronouns/01.md) and [God](rc://en/tw/dict/bible/kt/god).");
    println!("LINK: {:?} (spans {})", link[0].text(), link[0].spans.len());
    println!("NUMBERED: en_ta articles with a numbered item block: {numbered}");
}

#[test]
fn repro_628_marks_left_as_text() {
    for zip in ["en_ta_v91.sb.zip", "en_tw_v91.sb.zip"] {
        let all = articles(zip);
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        let mut links = 0;
        for (_, md) in &all {
            let blocks = render_article_blocks(md);
            let t = texts(&blocks);
            let any = |p: &dyn Fn(&str) -> bool| t.iter().any(|s| p(s));
            for (name, hit) in [
                ("'|' row", any(&|s| s.starts_with('|'))),
                ("<br>", any(&|s| s.contains("<br"))),
                ("<sup>", any(&|s| s.contains("<sup"))),
                ("<u>", any(&|s| s.contains("<u>"))),
                ("![image]", any(&|s| s.contains("!["))),
                ("**", any(&|s| s.contains("**"))),
                ("__", any(&|s| s.contains("__"))),
            ] {
                if hit {
                    *counts.entry(name).or_default() += 1;
                }
            }
            if md.contains("](") {
                links += 1;
            }
        }
        println!("{zip}: {} articles; articles with a mark left as text: {counts:?}; articles with a Markdown link (shown as its label, not clickable): {links}", all.len());
    }
}
