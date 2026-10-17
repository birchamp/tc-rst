//! What streaming a download saves, measured. The fake Door43 serves the burrito export
//! of the largest bundled resource (`bundle/en/en_ust_v91.sb.zip`, 93.7 MB unpacked; the
//! fake's export is 8.6 MB) on loopback, and each download runs in a process of its own so
//! that its peak resident memory is its own. Two ways are compared:
//!
//! - `stream`: the client as it is, writing the zip to a file and checking it from there;
//! - `memory`: the zip read whole into memory and checked there, as the client did before.
//!
//! The fake builds each export before it answers (it deflates all 93.7 MB again), so the
//! time that tells the two ways apart is the client's: from the first byte of the answer to
//! the checked export.
//!
//! ```text
//! cargo run --release -p tc-door43 --example download_bench [-- <runs per way>]
//! ```
//!
//! Linux only: the peak comes from `VmHWM` in `/proc/self/status`.

use std::error::Error;
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tc_door43::{
    CommitSha, Door43Client, DownloadTo, DownloadWatch, InternetSwitch, RepoRef, ServerUrl,
    verify_sb_export,
};
use tc_door43_fake::{CommitSeed, FakeDoor43, RepoSeed, SeedFile};

type Outcome<T> = Result<T, Box<dyn Error>>;

/// Keeps the size of the download so far, and when its first byte came.
#[derive(Default)]
struct Size {
    bytes: AtomicU64,
    first: std::sync::OnceLock<Instant>,
}

impl DownloadWatch for Size {
    fn received(&self, received: u64, _: Option<u64>) {
        self.first.get_or_init(Instant::now);
        self.bytes.store(received, Ordering::Relaxed);
    }

    fn cancelled(&self) -> bool {
        false
    }
}

const REPO: &str = "unfoldingWord/en_ust";
const TAG: &str = "v91";

#[tokio::main]
async fn main() -> Outcome<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("fetch") => {
            let [_, _, url, commit, way] = args.as_slice() else {
                return Err("fetch <url> <commit> <stream|memory>".into());
            };
            fetch(url, commit, way).await
        }
        runs => {
            let runs: usize = runs.map_or(Ok(20), str::parse)?;
            drive(runs).await
        }
    }
}

/// Seeds the fake, runs every download in a child process, and prints the summary.
async fn drive(runs: usize) -> Outcome<()> {
    let fake = FakeDoor43::start().await?;
    let commit = seed(&fake).await?;
    let exe = std::env::current_exe()?;
    println!("en_ust v91 export from the fake on loopback, {runs} runs each, one process per run");
    println!(
        "way     whole ms median  p95  first byte to checked ms median  p95  peak MiB median  max  zip bytes"
    );
    for way in ["stream", "memory"] {
        let mut times = Vec::new();
        let mut client_times = Vec::new();
        let mut peaks = Vec::new();
        let mut size = 0;
        for _ in 0..runs {
            let out = Command::new(&exe)
                .args(["fetch", fake.url(), &commit, way])
                .output()?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).into_owned().into());
            }
            let line = String::from_utf8(out.stdout)?;
            let fields: Vec<f64> = line
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()?;
            let [ms, client_ms, peak_kib, bytes] = fields.as_slice() else {
                return Err(format!("unreadable child output {line:?}").into());
            };
            times.push(*ms);
            client_times.push(*client_ms);
            peaks.push(*peak_kib / 1024.0);
            size = *bytes as u64;
        }
        println!(
            "{way:<7} {:>15.1}  {:>4.0}  {:>31.1}  {:>4.0}  {:>15.1}  {:>3.1}  {size}",
            percentile(&mut times, 0.5),
            percentile(&mut times, 0.95),
            percentile(&mut client_times, 0.5),
            percentile(&mut client_times, 0.95),
            percentile(&mut peaks, 0.5),
            percentile(&mut peaks, 1.0),
        );
    }
    Ok(())
}

/// One download in this process; prints the milliseconds it took in all and from its first
/// byte, the process's peak resident memory in KiB, and the zip's size.
async fn fetch(url: &str, commit: &str, way: &str) -> Outcome<()> {
    let server: ServerUrl = url.parse()?;
    let internet = InternetSwitch::new();
    internet.turn_on();
    let client = Door43Client::new(server, internet)?;
    let repo: RepoRef = REPO.parse()?;
    let commit: CommitSha = commit.parse()?;
    let folder = tempfile::tempdir()?;
    let start = Instant::now();
    let (size, first) = match way {
        "stream" => {
            let size = Size::default();
            let to = DownloadTo::folder(folder.path()).watched_by(&size);
            client
                .download_sb_export(&repo, &TAG.parse()?, &commit, to)
                .await?;
            let first = size.first.get().copied().ok_or("no byte arrived")?;
            (size.bytes.load(Ordering::Relaxed), first)
        }
        "memory" => {
            let (bytes, first) = whole_body(&format!("{url}/{REPO}/sb/{TAG}.zip")).await?;
            verify_sb_export(&bytes, &commit, 256 << 20)?;
            (bytes.len() as u64, first)
        }
        other => return Err(format!("no way {other:?}").into()),
    };
    let ms = |since: Instant| since.elapsed().as_secs_f64() * 1000.0;
    println!("{:.3} {:.3} {} {size}", ms(start), ms(first), peak_kib()?);
    Ok(())
}

/// The body read chunk by chunk into one buffer, as the client's `read_body` did, and when
/// its first byte came.
async fn whole_body(url: &str) -> Outcome<(Vec<u8>, Instant)> {
    let mut response = reqwest::Client::builder()
        .no_proxy()
        .build()?
        .get(url)
        .send()
        .await?
        .error_for_status()?;
    let mut body = Vec::new();
    let mut first = None;
    while let Some(chunk) = response.chunk().await? {
        first.get_or_insert_with(Instant::now);
        body.extend_from_slice(&chunk);
    }
    Ok((body, first.ok_or("no byte arrived")?))
}

/// Seeds the fake with en_ust v91 from the bundled export; returns the tag's commit.
async fn seed(fake: &FakeDoor43) -> Outcome<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en/en_ust_v91.sb.zip");
    let mut zip = zip::ZipArchive::new(Cursor::new(std::fs::read(path)?))?;
    let mut files = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_owned();
        let Some((_, inner)) = name.split_once('/') else {
            continue;
        };
        if inner.is_empty() || name.ends_with('/') {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        files.push(SeedFile::regular(inner, bytes));
    }
    let shas = fake
        .add_repo(RepoSeed {
            owner: "unfoldingWord".into(),
            name: "en_ust".into(),
            subject: "Aligned Bible".into(),
            language: "en".into(),
            topics: Vec::new(),
            private: false,
            commits: vec![CommitSeed {
                message: "v91".into(),
                files,
                tag: Some(TAG.into()),
                release: None,
            }],
        })
        .await?;
    shas.into_iter()
        .next()
        .ok_or_else(|| "the fake made no commit".into())
}

fn peak_kib() -> Outcome<u64> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let line = status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .ok_or("no VmHWM in /proc/self/status")?;
    Ok(line
        .split_whitespace()
        .nth(1)
        .ok_or("an empty VmHWM line")?
        .parse()?)
}

/// The value at fraction `q` of the sorted values (nearest rank).
fn percentile(values: &mut [f64], q: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    let rank = ((q * values.len() as f64).ceil() as usize).clamp(1, values.len());
    values[rank - 1]
}
