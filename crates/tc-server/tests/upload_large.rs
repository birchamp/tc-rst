//! A 1 GiB exchange upload streams to disk (APP-DESIGN §5.5, A-28): its own test binary,
//! because it reads this process's memory high-water mark, which parallel tests would spoil.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read as _, Write as _};
use std::time::Instant;

use common::{TestServer, read_response, stub};
use tc_api::Reply;
use tc_api::upload::{UploadFlow, UploadView};

const GIB: u64 = 1024 * 1024 * 1024;
const PIECE: usize = 64 * 1024;

/// Deletes the folder when the test ends, however it ends.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Byte `j` of piece `i`.
fn pattern(piece: u64, at: usize) -> u8 {
    ((piece * 31 + at as u64) % 251) as u8
}

fn piece(i: u64) -> Vec<u8> {
    (0..PIECE).map(|j| pattern(i, j)).collect()
}

/// The process's peak resident memory in KiB, or `None` where /proc has none.
fn peak_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

#[test]
fn a_1_gib_exchange_upload_streams_to_disk() {
    let scratch = Scratch(
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("upload-large-{}", std::process::id())),
    );
    std::fs::create_dir_all(&scratch.0).unwrap();
    let server = TestServer::start_in(stub(), &scratch.0);
    let request = server
        .upload_req(UploadFlow::Exchange, "work.tcx", b"")
        .with("Content-Length", &GIB.to_string());
    let before = peak_kib();
    let began = Instant::now();
    let mut stream = server.connect();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(120)))
        .unwrap();
    stream.write_all(&request.head_bytes()).unwrap();
    let pieces = GIB / PIECE as u64;
    // The same pieces repeat in turn, so the sender costs no memory and little time.
    let cache: Vec<Vec<u8>> = (0..251).map(piece).collect();
    for i in 0..pieces {
        stream.write_all(&cache[(i % 251) as usize]).unwrap();
    }
    let response = read_response(&mut stream);
    let seconds = began.elapsed().as_secs_f64();
    let after = peak_kib();
    assert_eq!(response.status, 200, "{}", response.text());
    let Reply::Ok(view) = serde_json::from_slice::<Reply<UploadView>>(&response.body).unwrap()
    else {
        panic!("refused: {}", response.text());
    };
    assert_eq!(view.size, GIB);

    // The file on disk is the bytes sent, piece by piece.
    let mut file = std::fs::File::open(server.uploads_dir().join(view.id.as_str())).unwrap();
    assert_eq!(file.metadata().unwrap().len(), GIB);
    let mut buf = vec![0_u8; PIECE];
    for i in 0..pieces {
        file.read_exact(&mut buf).unwrap();
        assert!(buf == cache[(i % 251) as usize], "piece {i} differs");
    }

    // The server runs in this process: a body held whole in memory would show.
    if let (Some(before), Some(after)) = (before, after) {
        let growth_mib = after.saturating_sub(before) / 1024;
        println!("peak memory growth: {growth_mib} MiB");
        assert!(growth_mib < 128, "the peak grew by {growth_mib} MiB");
    }
    println!(
        "1 GiB exchange upload: {seconds:.1} s, {:.0} MB/s",
        GIB as f64 / 1_000_000.0 / seconds
    );
    drop(server);
}
