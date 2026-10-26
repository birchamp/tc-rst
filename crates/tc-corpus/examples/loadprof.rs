//! Where a load's CPU time goes, one thread: `loadprof <psalms|bible> [parse]`.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a benchmark stops at its first failure, with the reason"
)]
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tc_corpus::{Corpus, Spec};

fn kind_of(json: &[u8]) -> String {
    let s = std::str::from_utf8(json).unwrap();
    s.find("\"record\":{\"")
        .map(|i| {
            let r = &s[i + 11..];
            r[..r.find('"').unwrap()].to_string()
        })
        .unwrap_or_default()
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    let spec = if which == "bible" {
        Spec::whole_bible(1)
    } else {
        Spec::psalms(1)
    };
    let corpus = Corpus::generate(&spec).expect("corpus");
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for f in corpus
        .journal
        .packs
        .iter()
        .map(|p| &p.bytes)
        .chain(corpus.journal.pending.iter().map(|p| &p.bytes))
    {
        for (i, l) in f.split(|b| *b == b'\n').enumerate() {
            if i == 0 || l.is_empty() {
                continue;
            }
            lines.push(l.to_vec());
        }
    }
    drop(corpus);
    let total: usize = lines.iter().map(Vec::len).sum();
    eprintln!("{} lines, {} bytes", lines.len(), total);
    let t = Instant::now();
    let jsons: Vec<_> = lines
        .iter()
        .map(|l| {
            let (id, json) = tc_journal::line::verify_line(l).expect("verify");
            (id, Arc::<[u8]>::from(json))
        })
        .collect();
    eprintln!("verify_line (sha) {:?}", t.elapsed());
    drop(lines);
    let t = Instant::now();
    let mut ok = 0;
    for (id, j) in &jsons {
        ok += matches!(
            tc_journal::parse_change(*id, j.clone()),
            tc_journal::Verified::Change(_)
        ) as usize;
    }
    eprintln!("parse_change {:?} ({ok})", t.elapsed());
    if std::env::args().nth(2).as_deref() == Some("parse") {
        return;
    }
    let mut by_kind = BTreeMap::<String, (usize, usize, Duration)>::new();
    for (id, j) in &jsons {
        let t0 = Instant::now();
        let v = tc_journal::parse_change(*id, j.clone());
        let e = t0.elapsed();
        drop(v);
        let entry = by_kind.entry(kind_of(j)).or_default();
        entry.0 += 1;
        entry.1 += j.len();
        entry.2 += e;
    }
    for (k, (c, b, d)) in by_kind {
        eprintln!(
            "{k}: {c} changes {b} bytes {:?} ({:.1} MB/s)",
            d,
            b as f64 / d.as_secs_f64() / 1e6
        );
    }
    let t = Instant::now();
    let p: Vec<_> = jsons
        .iter()
        .map(|(id, j)| {
            tc_journal::fold::FoldInput::new(tc_journal::parse_change(*id, j.clone()), None)
                .prepare()
        })
        .collect();
    eprintln!("parse_change and prepare {:?} {}", t.elapsed(), p.len());
}
