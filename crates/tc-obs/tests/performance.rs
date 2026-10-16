//! Reading Open Bible Stories costs what it reads (docs/PERFORMANCE.md rules 1 and 2).
//!
//! The count tests read, decode, render and write the fifty en_obs v9 stories, and the same
//! stories made ten times larger (each with its frames ten times over), and assert that
//! each line is touched once per read and once per write: the cost follows the lines read
//! and written and nothing else.
//! The benchmark times the same actions in a release build:
//! `cargo test --release -p tc-obs --test performance -- --nocapture`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::stories;
use tc_obs::story::{decode_story, lines_touched, parse_story, write_frame};
use tc_obs::{FrameNo, StoryFile};

fn lines(text: &str) -> u64 {
    text.split('\n').count() as u64
}

/// The story with its frames `times` times over, each copy's image line made distinct, in
/// the writers' layout.
fn enlarged(text: &str, times: usize) -> String {
    let story = parse_story(text).unwrap();
    let mut out = vec![
        format!("# {}. {}", story.number, story.title),
        String::new(),
    ];
    for copy in 0..times {
        for frame in &story.frames {
            out.push(format!(
                "![{}]({}?copy={copy})",
                frame.image.alt, frame.image.url
            ));
            out.push(String::new());
            out.extend(frame.text.split('\n').map(str::to_owned));
            out.push(String::new());
        }
    }
    if let Some(r) = &story.reference {
        out.push(format!("_{r}_"));
        out.push(String::new());
    }
    out.join("\n")
}

/// Lines touched by `action` over every story.
fn touched(corpus: &[String], action: impl Fn(&str)) -> u64 {
    let before = lines_touched();
    for text in corpus {
        action(text);
    }
    lines_touched() - before
}

fn corpora() -> (Vec<String>, Vec<String>) {
    let real = stories("en_obs");
    let large = real.iter().map(|t| enlarged(t, 10)).collect();
    (real, large)
}

#[test]
fn reading_every_story_examines_each_line_once() {
    let (real, large) = corpora();
    for corpus in [&real, &large] {
        let read = touched(corpus, |t| {
            StoryFile::parse(t).unwrap().story();
        });
        assert_eq!(read, corpus.iter().map(|t| lines(t)).sum::<u64>());
    }
    // The larger corpus holds about ten times the lines, and costs exactly its lines.
    let (small, big): (u64, u64) = (
        real.iter().map(|t| lines(t)).sum(),
        large.iter().map(|t| lines(t)).sum(),
    );
    assert!(big > 9 * small && big < 11 * small, "{small} {big}");
}

#[test]
fn opening_one_story_reads_that_story_alone() {
    let (real, large) = corpora();
    for corpus in [&real, &large] {
        let one = &corpus[0];
        assert_eq!(
            touched(std::slice::from_ref(one), |t| drop(
                StoryFile::parse(t).unwrap()
            )),
            lines(one)
        );
    }
}

#[test]
fn decoding_and_rendering_a_story_reads_and_writes_it_and_its_base_once() {
    let (real, large) = corpora();
    for corpus in [&real, &large] {
        for text in corpus.iter() {
            let before = lines_touched();
            let records = decode_story(text).unwrap().records;
            let decoded = lines_touched() - before;
            let rendered = records.render().unwrap();
            // Decoding reads the file and writes the base; rendering reads the base and
            // writes the file.
            assert_eq!(decoded, lines(text) + lines(&records.base));
            assert_eq!(
                lines_touched() - before,
                2 * (lines(text) + lines(&records.base))
            );
            assert_eq!(&rendered, text);
        }
    }
}

/// One story of `frames` frames of two-line paragraphs: the shape that made each frame's
/// reduction move every line after it.
fn long_story(frames: usize) -> String {
    let mut text = String::from("# 1. Title\n\n");
    for f in 1..=frames {
        text.push_str(&format!("![a](b{f})\n\nline one of {f}\nline two\n\n"));
    }
    text.push_str("_Reference_\n");
    text
}

#[test]
fn decoding_one_long_story_costs_its_lines() {
    for frames in [400, 4_000] {
        let text = long_story(frames);
        let before = lines_touched();
        let decoded = decode_story(&text).unwrap();
        let base = &decoded.records.base;
        assert_eq!(lines_touched() - before, lines(&text) + lines(base));
        // The base holds each frame's image line and three empty lines, and the title's.
        assert_eq!(lines(base), 2 + 4 * frames as u64 + 3);
        assert_eq!(decoded.records.render().unwrap(), text);
    }
}

#[test]
fn saving_a_frame_reads_and_writes_its_story_once() {
    let (real, large) = corpora();
    for corpus in [&real, &large] {
        for text in corpus.iter() {
            let frames = StoryFile::parse(text.as_str()).unwrap().frame_count() as u16;
            let before = lines_touched();
            let written = write_frame(text, FrameNo(frames), "Texto nuevo.").unwrap();
            assert_eq!(lines_touched() - before, lines(text) + lines(&written));
        }
    }
}

/// Median and p95 of `runs` timings of `action`, after one untimed warm-up.
fn time(runs: usize, mut action: impl FnMut()) -> (Duration, Duration) {
    action();
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            action();
            start.elapsed()
        })
        .collect();
    times.sort();
    (times[runs / 2], times[(runs * 95).div_ceil(100) - 1])
}

#[test]
fn the_obs_actions_are_quick() {
    let (real, large) = corpora();
    let runs = if cfg!(debug_assertions) { 3 } else { 41 };
    let bytes: usize = real.iter().map(String::len).sum();
    let largest = real.iter().max_by_key(|t| t.len()).unwrap();
    let records: Vec<_> = real
        .iter()
        .map(|t| decode_story(t).unwrap().records)
        .collect();
    let largest_records = decode_story(largest).unwrap().records;
    let long = [long_story(2_000), long_story(32_000)];
    let frames = StoryFile::parse(largest.as_str()).unwrap().frame_count() as u16;
    let rows = [
        (
            "read all 50 stories (Home tiles, progress)",
            time(runs, || {
                for t in &real {
                    std::hint::black_box(StoryFile::parse(t.as_str()).unwrap().story());
                }
            }),
        ),
        (
            "read all 50 stories, ten times larger",
            time(runs, || {
                for t in &large {
                    std::hint::black_box(StoryFile::parse(t.as_str()).unwrap().story());
                }
            }),
        ),
        (
            "open the largest story",
            time(runs, || {
                std::hint::black_box(StoryFile::parse(largest.as_str()).unwrap().story());
            }),
        ),
        (
            "save a frame of the largest story",
            time(runs, || {
                std::hint::black_box(
                    write_frame(largest, FrameNo(frames), "Texto nuevo.").unwrap(),
                );
            }),
        ),
        (
            "project the largest story from its records",
            time(runs, || {
                std::hint::black_box(largest_records.render().unwrap());
            }),
        ),
        (
            "project all 50 stories from their records",
            time(runs, || {
                for r in &records {
                    std::hint::black_box(r.render().unwrap());
                }
            }),
        ),
        (
            "decode all 50 stories into records",
            time(runs, || {
                for t in &real {
                    std::hint::black_box(decode_story(t).unwrap());
                }
            }),
        ),
        (
            "decode one story of 2,000 two-line frames",
            time(runs, || {
                std::hint::black_box(decode_story(&long[0]).unwrap());
            }),
        ),
        (
            "decode one story of 32,000 two-line frames",
            time(runs, || {
                std::hint::black_box(decode_story(&long[1]).unwrap());
            }),
        ),
    ];
    println!(
        "{} stories, {bytes} bytes; largest {} bytes, {frames} frames; {runs} runs",
        real.len(),
        largest.len()
    );
    for (name, (median, p95)) in &rows {
        println!("{name}: median {median:?}, p95 {p95:?}");
    }
    if cfg!(debug_assertions) {
        return;
    }
    // Three times the nearest budgets of docs/PERFORMANCE.md (rule 5): the first chapter of
    // an opened book ready in 50 ms, a chapter's projection after a save in 2 ms.
    let ceiling = |name: &str| rows.iter().find(|(n, _)| *n == name).unwrap().1.1;
    assert!(ceiling("read all 50 stories (Home tiles, progress)") < Duration::from_millis(150));
    assert!(ceiling("open the largest story") < Duration::from_millis(6));
    assert!(ceiling("save a frame of the largest story") < Duration::from_millis(6));
    assert!(ceiling("project the largest story from its records") < Duration::from_millis(6));
}
