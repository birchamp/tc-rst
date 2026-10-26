//! SHA-256 in bulk, as a first open hashes (docs/STORAGE.md §4.4): `ring` against `sha2`.
//!
//! `cargo run --release -p tc-corpus --example shabench -- DIR [rounds]` reads the packs and
//! pending files of a corpus saved by `foldbench save` and hashes them on one thread the two
//! ways a read can: each change line's JSON alone (P4: one hash a line, about 1 KB), and
//! whole files in 1 MiB pieces (a pass over every byte). In each round the two libraries run
//! one right after the other, so a change in the machine's load hits both. It prints MB/s for
//! each round, then the medians, and checks that both give the same digests.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use sha2::Digest;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["packs", "pending"]
        .iter()
        .flat_map(|sub| std::fs::read_dir(dir.join(sub)).expect("a saved corpus"))
        .flatten()
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// The JSON of every line of `bytes`: the part before its last tab.
fn jsons(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split(|b| *b == b'\n').filter_map(|line| {
        let tab = line.iter().rposition(|b| *b == b'\t')?;
        Some(&line[..tab])
    })
}

fn ring_bytes(digest: ring::digest::Digest) -> [u8; 32] {
    digest.as_ref().try_into().expect("32 bytes")
}

fn sha2_whole(bytes: &[u8]) -> [u8; 32] {
    sha2::Sha256::digest(bytes).into()
}

fn ring_whole(bytes: &[u8]) -> [u8; 32] {
    ring_bytes(ring::digest::digest(&ring::digest::SHA256, bytes))
}

fn sha2_pieces(bytes: &[u8]) -> [u8; 32] {
    let mut hash = sha2::Sha256::new();
    for piece in bytes.chunks(1 << 20) {
        hash.update(piece);
    }
    hash.finalize().into()
}

fn ring_pieces(bytes: &[u8]) -> [u8; 32] {
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    for piece in bytes.chunks(1 << 20) {
        hash.update(piece);
    }
    ring_bytes(hash.finish())
}

/// MB/s of `hash` over `inputs`, and a digest of all the digests, to check the two agree.
fn rate(inputs: &[&[u8]], hash: fn(&[u8]) -> [u8; 32]) -> (f64, [u8; 32]) {
    let bytes: usize = inputs.iter().map(|i| i.len()).sum();
    let mut all = sha2::Sha256::new();
    let started = Instant::now();
    for input in inputs {
        all.update(hash(std::hint::black_box(input)));
    }
    let seconds = started.elapsed().as_secs_f64();
    (bytes as f64 / seconds / 1e6, all.finalize().into())
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn load() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .unwrap_or("unavailable")
        .to_owned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(args.first().expect("shabench DIR [rounds]"));
    let rounds: usize = args.get(1).and_then(|r| r.parse().ok()).unwrap_or(5);
    let contents: Vec<Vec<u8>> = files(&dir)
        .iter()
        .map(|p| std::fs::read(p).expect("read"))
        .collect();
    let whole: Vec<&[u8]> = contents.iter().map(Vec::as_slice).collect();
    let lines: Vec<&[u8]> = contents.iter().flat_map(|f| jsons(f)).collect();
    let line_bytes: usize = lines.iter().map(|l| l.len()).sum();
    #[cfg(target_arch = "x86_64")]
    println!(
        "cpu: sha {}, avx2 {}, avx512f {}, avx512bw {}, avx512vl {}",
        std::is_x86_feature_detected!("sha"),
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("avx512f"),
        std::is_x86_feature_detected!("avx512bw"),
        std::is_x86_feature_detected!("avx512vl"),
    );
    #[cfg(not(target_arch = "x86_64"))]
    println!("cpu: not x86, so no SHA or AVX-512 bits to read");
    println!(
        "{} files, {} bytes; {} lines, {line_bytes} bytes of JSON ({} B a line)",
        whole.len(),
        whole.iter().map(|f| f.len()).sum::<usize>(),
        lines.len(),
        line_bytes / lines.len().max(1),
    );
    let mut results: [Vec<f64>; 4] = Default::default();
    for round in 0..rounds {
        let before = load();
        let (a, da) = rate(&lines, sha2_whole);
        let (b, db) = rate(&lines, ring_whole);
        let (c, dc) = rate(&whole, sha2_pieces);
        let (d, dd) = rate(&whole, ring_pieces);
        assert_eq!(da, db, "line digests differ");
        assert_eq!(dc, dd, "file digests differ");
        println!(
            "round {round}: lines sha2 {a:.0} MB/s, ring {b:.0} MB/s; files sha2 {c:.0} MB/s, \
             ring {d:.0} MB/s; load {before}..{}",
            load()
        );
        for (slot, value) in results.iter_mut().zip([a, b, c, d]) {
            slot.push(value);
        }
    }
    let [a, b, c, d] = results.map(median);
    println!(
        "median of {rounds}: lines sha2 {a:.0} MB/s, ring {b:.0} MB/s ({:.2}x); files sha2 \
         {c:.0} MB/s, ring {d:.0} MB/s ({:.2}x)",
        b / a,
        d / c
    );
}
