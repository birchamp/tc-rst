//! What the print model costs on the largest book and on a whole OBS project
//! (docs/PERFORMANCE.md rule 5), and that it grows with the book and nothing else. Run in a
//! release build for the figures:
//! `cargo test --release -p tc-export --test performance -- --nocapture`.
//!
//! The rows come from tc-usfm's `shown_text_v1` of each verse, the text the editor's rows
//! show.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::time::{Duration, Instant};

use common::repo_path;
use tc_api::model::print::{FlowBlock, PageSetup};
use tc_export::Direction;
use tc_export::book::{
    BookChapter, BookVerse, has_undrafted, print_book, print_items, printed_items,
};
use tc_export::stories::{StoryBlock, print_stories};
use tc_helps::obs::ObsStory;
use tc_obs::print::printed;

fn psalms() -> Vec<BookChapter> {
    let usfm = std::fs::read_to_string(repo_path("fixtures/scale/psa/en_ult/19-PSA.usfm")).unwrap();
    let book = tc_usfm::Book::parse(&usfm);
    let mut chapters: BTreeMap<u16, Vec<BookVerse>> = BTreeMap::new();
    for unit in book.verses() {
        let body = unit.body().trim();
        let drafted = !body.is_empty() && body != "___";
        chapters
            .entry(unit.chapter().unwrap())
            .or_default()
            .push(BookVerse {
                label: unit.label().to_owned(),
                drafted,
                text: if drafted {
                    tc_usfm::shown_text_v1(unit.body())
                } else {
                    String::new()
                },
            });
    }
    chapters
        .into_iter()
        .map(|(number, verses)| BookChapter { number, verses })
        .collect()
}

/// The book `times` times over, its chapters numbered on.
fn enlarged(book: &[BookChapter], times: u16) -> Vec<BookChapter> {
    let count = u16::try_from(book.len()).unwrap();
    (0..times)
        .flat_map(|copy| {
            book.iter().map(move |c| BookChapter {
                number: c.number + copy * count,
                verses: c.verses.clone(),
            })
        })
        .collect()
}

/// The book with every other chapter, and every third verse, undrafted.
fn half_drafted(book: &[BookChapter]) -> Vec<BookChapter> {
    book.iter()
        .map(|c| BookChapter {
            number: c.number,
            verses: c
                .verses
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let undrafted = c.number % 2 == 0 || i % 3 == 0;
                    BookVerse {
                        label: v.label.clone(),
                        drafted: !undrafted,
                        text: if undrafted {
                            String::new()
                        } else {
                            v.text.clone()
                        },
                    }
                })
                .collect(),
        })
        .collect()
}

fn verse_lines(book: &[BookChapter]) -> usize {
    let items = print_items(&printed_items(book)).unwrap();
    print_book("Psalms", &items, &PageSetup::default(), Direction::Ltr)
        .flow
        .iter()
        .map(|block| match block {
            FlowBlock::Chapter(c) => c.verses.len(),
            FlowBlock::Gap(_) => 0,
        })
        .sum()
}

fn time(runs: usize, mut action: impl FnMut()) -> (Duration, Duration) {
    let mut samples: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            action();
            start.elapsed()
        })
        .collect();
    samples.sort();
    (samples[runs / 2], samples[(runs * 95).div_ceil(100) - 1])
}

fn en_obs() -> Vec<ObsStory> {
    let path = repo_path("bundle/en/en_obs_v9.sb.zip");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (1..=50)
        .map(|n| {
            let mut text = String::new();
            archive
                .by_name(&format!("en_obs/ingredients/content/{n:02}.md"))
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            tc_obs::story::parse_story(&text).unwrap()
        })
        .collect()
}

#[test]
fn the_print_model_costs_what_the_book_holds() {
    let psalms = psalms();
    let verses: usize = psalms.iter().map(|c| c.verses.len()).sum();
    assert_eq!((psalms.len(), verses), (150, 2461));
    let ten = enlarged(&psalms, 10);
    let half = half_drafted(&psalms);
    // Each verse of a printed chapter prints once: the output grows with the book only.
    assert_eq!(verse_lines(&psalms), verses);
    assert_eq!(verse_lines(&ten), 10 * verses);
    let stories = en_obs();
    let mut sources = |story: &ObsStory| -> BTreeMap<usize, String> {
        (1..=story.frames.len())
            .map(|n| (n, format!("/media/obs/{}-{n}.jpg", story.number)))
            .collect()
    };
    let obs = printed(&stories, Some(&mut sources));
    let frames: usize = stories.iter().map(|s| s.frames.len()).sum();
    let printed_frames: usize = print_stories(
        &obs.items,
        &obs.pictures,
        &PageSetup::default(),
        Direction::Ltr,
    )
    .blocks
    .iter()
    .map(|b| match b {
        StoryBlock::Story(s) => s.frames.len(),
        StoryBlock::Gap(_) => 0,
    })
    .sum();
    assert_eq!(printed_frames, frames);

    let runs = 21;
    let model = |book: &[BookChapter]| {
        let items = print_items(&printed_items(book)).unwrap();
        let view = print_book("Psalms", &items, &PageSetup::default(), Direction::Ltr);
        std::hint::black_box((view, has_undrafted(book)));
    };
    let rows = [
        (
            "Psalms: items, parts and view",
            time(runs, || model(&psalms)),
        ),
        (
            "Psalms, every other chapter undrafted",
            time(runs, || model(&half)),
        ),
        (
            "Psalms ten times over (1,500 chapters)",
            time(runs, || model(&ten)),
        ),
        (
            "en_obs v9, 50 stories: printed and view, pictures on",
            time(runs, || {
                let obs = printed(&stories, Some(&mut sources));
                std::hint::black_box(print_stories(
                    &obs.items,
                    &obs.pictures,
                    &PageSetup::default(),
                    Direction::Ltr,
                ));
            }),
        ),
    ];
    println!("{verses} Psalms verses, {frames} OBS frames; {runs} runs");
    for (name, (median, p95)) in &rows {
        println!("{name}: median {median:?}, p95 {p95:?}");
    }
}
