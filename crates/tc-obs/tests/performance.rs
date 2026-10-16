//! Reading Open Bible Stories costs what it reads (docs/PERFORMANCE.md rules 1 and 2).
//!
//! The count tests read, decode, render and write the fifty en_obs v9 stories, and the same
//! stories made ten times larger (each with its frames ten times over), and assert that
//! each line is examined once per read: the cost follows the lines read and nothing else.
//! The benchmark times the same actions in a release build:
//! `cargo test --release -p tc-obs --test performance -- --nocapture`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::stories;
use tc_obs::story::{decode_story, lines_examined, parse_story, write_frame};
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

/// Lines examined by `action` over every story.
fn examined(corpus: &[String], action: impl Fn(&str)) -> u64 {
    let before = lines_examined();
    for text in corpus {
        action(text);
    }
    lines_examined() - before
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
        let read = examined(corpus, |t| {
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
            examined(std::slice::from_ref(one), |t| drop(
                StoryFile::parse(t).unwrap()
            )),
            lines(one)
        );
    }
}

#[test]
fn decoding_and_rendering_a_story_reads_it_and_its_base_once() {
    let (real, large) = corpora();
    for corpus in [&real, &large] {
        for text in corpus.iter() {
            let before = lines_examined();
            let records = decode_story(text).unwrap();
            let rendered = records.render().unwrap();
            assert_eq!(
                lines_examined() - before,
                lines(text) + lines(&records.base)
            );
            assert_eq!(&rendered, text);
        }
    }
}

#[test]
fn saving_a_frame_reads_its_story_once() {
    let (real, large) = corpora();
    for corpus in [&real, &large] {
        for text in corpus.iter() {
            let frames = StoryFile::parse(text.as_str()).unwrap().frame_count() as u16;
            let before = lines_examined();
            write_frame(text, FrameNo(frames), "Texto nuevo.").unwrap();
            assert_eq!(lines_examined() - before, lines(text));
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
    let records: Vec<_> = real.iter().map(|t| decode_story(t).unwrap()).collect();
    let largest_records = decode_story(largest).unwrap();
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
