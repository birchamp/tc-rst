//! Story Markdown and the print document, through `tc_export::produce`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use tc_api::model::print::PageSetup;
use tc_export::book::{print_items, printed_items};
use tc_export::{Direction, ExportError, ExportInput, PrintBookInput, Produced, Producer, produce};

use common::{chapters_of, markup, repo_path, vectors};

#[test]
fn story_markdown_is_the_content_folder_without_bak_or_ds_store() {
    fn walk(root: &std::path::Path, dir: &str, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = format!("{dir}/{}", entry.file_name().into_string().unwrap());
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else {
                out.push((path, std::fs::read(entry.path()).unwrap()));
            }
        }
    }
    let root = repo_path("fixtures/tc4/sample-burrito-obs");
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    walk(&root, "ingredients/content", &mut files);
    let stories = files.clone();
    files.push(("ingredients/content/01.md.bak".into(), b"old".to_vec()));
    files.push(("ingredients/content/.DS_Store".into(), b"mac".to_vec()));
    files.push(("ingredients/LICENSE.md".into(), b"license".to_vec()));
    files.push(("metadata.json".into(), b"{}".to_vec()));

    let (out, _) = produce(
        Producer::StoryMarkdown,
        &ExportInput::StoryFiles(files.clone()),
    )
    .unwrap();
    let Produced::Files(out) = out else {
        panic!("Story Markdown is a folder of files")
    };
    let expected: Vec<(String, Vec<u8>)> = files[..stories.len()]
        .iter()
        .map(|(path, bytes)| (path["ingredients/".len()..].to_owned(), bytes.clone()))
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(out, expected);
}

#[test]
fn the_print_document_holds_tc4s_print_dom_and_page() {
    let case = &vectors()["books"][0];
    let items = print_items(&printed_items(&chapters_of(case))).unwrap();
    let title = case["title"].as_str().unwrap().to_owned();
    let setup = PageSetup::default();
    let print = |items| {
        produce(
            Producer::PrintHtml,
            &ExportInput::PrintBook(PrintBookInput {
                title: title.clone(),
                document_title: "TIT-2026-10-24".to_owned(),
                items,
                setup,
                dir: Direction::Ltr,
                stylesheet: ".print-book{}",
                words: &markup::word,
            }),
        )
    };
    let (out, _) = print(items.clone()).unwrap();
    let Produced::File(bytes) = out else {
        panic!("the print is one file")
    };
    let html = String::from_utf8(bytes).unwrap();
    let view = tc_export::book::print_book(&title, &items, &setup, Direction::Ltr);
    assert!(
        html.starts_with(
            "<!doctype html>\n<html dir=\"ltr\"><head><meta charset=\"utf-8\"><title>TIT-2026-10-24</title>"
        )
    );
    assert!(
        html.contains("@page { size: A4; margin: 18mm 16mm 20mm;"),
        "{html}"
    );
    assert!(html.contains(":root { --print-columns: "));
    assert!(html.contains("\n.print-book{}</style></head>\n<body>"));
    assert!(html.ends_with(&format!("<body>{}</body></html>", markup::book(&view))));

    assert!(matches!(
        print(Vec::new()),
        Err(ExportError::NothingDrafted)
    ));
}
