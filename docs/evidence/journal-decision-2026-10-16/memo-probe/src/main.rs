use std::time::Instant;
use std::path::{Path, PathBuf};

fn make_buf(n: usize) -> Vec<u8> {
    // JSON-like bytes; content does not affect SHA-256 speed.
    let mut v = Vec::with_capacity(n);
    let mut x: u64 = 0x9E3779B97F4A7C15;
    while v.len() < n {
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        v.push(b' ' + (x % 90) as u8);
    }
    v
}

fn sha_bench(total: usize, line: usize) {
    let buf = make_buf(total);
    for round in 0..3 {
        let t = Instant::now();
        let mut acc = 0u8;
        for chunk in buf.chunks(line) {
            use sha2::Digest;
            let d = sha2::Sha256::digest(chunk);
            acc ^= d[0];
        }
        let s = t.elapsed().as_secs_f64();
        println!("sha2-0.11 line={line} round={round} {:.0} ms {:.0} MB/s acc={acc}", s*1e3, total as f64/1e6/s);
        let t = Instant::now();
        let mut acc = 0u8;
        for chunk in buf.chunks(line) {
            let d = ring::digest::digest(&ring::digest::SHA256, chunk);
            acc ^= d.as_ref()[0];
        }
        let s = t.elapsed().as_secs_f64();
        println!("ring-0.17 line={line} round={round} {:.0} ms {:.0} MB/s acc={acc}", s*1e3, total as f64/1e6/s);
    }
}

fn list(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect()
}

fn read_files(dir: &Path, threads: usize) {
    let t = Instant::now();
    let files = list(dir);
    let tl = t.elapsed().as_secs_f64();
    let chunks: Vec<Vec<PathBuf>> = {
        let mut c = vec![Vec::new(); threads];
        for (i, f) in files.into_iter().enumerate() { c[i % threads].push(f); }
        c
    };
    let n: usize = chunks.iter().map(|c| c.len()).sum();
    let t2 = Instant::now();
    let bytes: usize = std::thread::scope(|s| {
        let hs: Vec<_> = chunks.iter().map(|c| s.spawn(move || {
            let mut b = 0usize;
            for p in c { let v = std::fs::read(p).unwrap(); b += v.len(); }
            b
        })).collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    let tr = t2.elapsed().as_secs_f64();
    println!("files={n} threads={threads} list={:.0} ms read={:.0} ms ({:.1} us/file) bytes={bytes}", tl*1e3, tr*1e3, tr*1e6/n as f64);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "sha" => { let total: usize = args[2].parse().unwrap(); let line: usize = args[3].parse().unwrap(); sha_bench(total, line); }
        "read" => { let dir = PathBuf::from(&args[2]); let th: usize = args[3].parse().unwrap(); read_files(&dir, th); }
        "write" => {
            let dir = PathBuf::from(&args[2]); let n: usize = args[3].parse().unwrap(); let size: usize = args[4].parse().unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            let buf = make_buf(size);
            let t = Instant::now();
            for i in 0..n {
                let name = format!("2026-10-16T10_{:02}_{:02}.{:03}Z,{:04x},a0123456789abcd.action.json", (i/60000)%60, (i/1000)%60, i%1000, i%65536);
                std::fs::write(dir.join(name), &buf).unwrap();
            }
            println!("wrote {n} files in {:.1} s", t.elapsed().as_secs_f64());
        }
        _ => panic!("usage"),
    }
}
