// Red-team probe: what an open costs per file when a cache trusts each immutable segment's
// name, size and mtime instead of reading it. 617,143 sparse files of 934 B in tC4's layout
// (three actor folders), list only and list + lstat, 1 and 4 threads, 3 rounds; then deleted.
use std::path::{Path, PathBuf};
use std::time::Instant;

fn make(root: &Path, total: usize, actors: usize) {
    for a in 0..actors {
        let dir = root.join(format!("actor-{a}")).join("segments");
        std::fs::create_dir_all(&dir).unwrap();
        let n = total / actors + usize::from(a < total % actors);
        for i in 0..n {
            let name = format!("2026-{:02}-{:02}T{:02}_{:02}_{:02}.{:03}Z,{:04},actor-{a}.action.json",
                1 + (i / 2_000_000) % 12, 1 + (i / 86_400) % 28, (i / 3600) % 24, (i / 60) % 60, i % 60, i % 1000, i % 10_000);
            let f = std::fs::File::create(dir.join(name)).unwrap();
            f.set_len(934).unwrap();
        }
    }
}

fn dirs(root: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root).unwrap().map(|e| e.unwrap().path().join("segments")).collect();
    v.sort();
    v
}

fn list(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect()
}

fn run(root: &Path, threads: usize, stat: bool) -> (usize, u64, f64) {
    let t = Instant::now();
    let ds = dirs(root);
    let mut files: Vec<PathBuf> = Vec::new();
    for d in &ds { files.extend(list(d)); }
    let n = files.len();
    let mut bytes = 0u64;
    if stat {
        let mut chunks = vec![Vec::new(); threads];
        for (i, f) in files.into_iter().enumerate() { chunks[i % threads].push(f); }
        let hs: Vec<_> = chunks.into_iter().map(|c| std::thread::spawn(move || {
            let mut s = 0u64;
            for f in c { let m = std::fs::symlink_metadata(&f).unwrap(); s += m.len() + (m.modified().is_ok() as u64) * 0; }
            s
        })).collect();
        for h in hs { bytes += h.join().unwrap(); }
    }
    (n, bytes, t.elapsed().as_secs_f64() * 1e3)
}

fn run_at(root: &Path, threads: usize) -> f64 {
    // lstat relative to the open directory (fstatat on the read_dir handle), one actor folder per thread
    let t = Instant::now();
    let ds = dirs(root);
    let hs: Vec<_> = ds.into_iter().map(|d| std::thread::spawn(move || {
        let mut s = 0u64;
        for e in std::fs::read_dir(&d).unwrap() { s += e.unwrap().metadata().unwrap().len(); }
        s
    })).collect();
    let mut b = 0u64;
    if threads == 1 { for h in hs { b += h.join().unwrap(); } } else { for h in hs { b += h.join().unwrap(); } }
    let _ = b;
    t.elapsed().as_secs_f64() * 1e3
}

fn run_at_serial(root: &Path) -> f64 {
    let t = Instant::now();
    let mut s = 0u64;
    for d in dirs(root) { for e in std::fs::read_dir(&d).unwrap() { s += e.unwrap().metadata().unwrap().len(); } }
    let _ = s;
    t.elapsed().as_secs_f64() * 1e3
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("dir"));
    let total: usize = std::env::args().nth(2).map(|s| s.parse().unwrap()).unwrap_or(617_143);
    let t = Instant::now();
    make(&root, total, 3);
    println!("created {total} sparse files in {:.1} s", t.elapsed().as_secs_f64());
    for round in 1..=3 {
        let (n, _, l) = run(&root, 1, false);
        let (_, b1, s1) = run(&root, 1, true);
        let (_, b4, s4) = run(&root, 4, true);
        let a1 = run_at_serial(&root);
        let a3 = run_at(&root, 3);
        println!("round {round}: files={n} list={l:.0} ms; list+lstat by path 1 thread={s1:.0} ms, 4 threads={s4:.0} ms; list+fstatat 1 thread={a1:.0} ms, 3 threads (one per actor folder)={a3:.0} ms (bytes {b1} {b4})");
    }
    let t = Instant::now();
    std::fs::remove_dir_all(&root).unwrap();
    println!("removed in {:.1} s", t.elapsed().as_secs_f64());
}
