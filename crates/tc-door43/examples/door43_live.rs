//! A read-only check of the Door43 client against a real Door43 server. `cargo test` never
//! runs it, and nothing in it writes to Door43.
//!
//! ```text
//! cargo run -p tc-door43 --example door43_live [-- <server url>]
//! ```
//!
//! The server defaults to `https://git.door43.org`. The client's log of each request and the
//! status of each answer goes to stderr. Each check prints its result and how long it took
//! to stdout; a download also prints how many chunks its bytes came in and whether Door43
//! announced their size. The exit code is non-zero when a check fails.

use std::error::Error;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tc_door43::{
    CatalogQuery, CommitSha, Door43Client, DownloadTo, DownloadWatch, InternetSwitch, Limits,
    RepoRef, ServerUrl, TagName,
};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const TN: &str = "unfoldingWord/en_tn";
const TN_V91: &str = "e586762e330f482a60c52aedd1c7b3a2f155df8a";
const TWL: &str = "unfoldingWord/en_twl";
const TWL_V91: &str = "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64";
const UGL: &str = "uW/en_ugl";
const UGL_COMMIT: &str = "d9d29e2d589258ce27f92b59f753a3af03ab7a72";

type Outcome = Result<String, Box<dyn Error>>;

/// Counts a download's chunks and keeps its last report.
#[derive(Default)]
struct Counted {
    chunks: AtomicU64,
    last: Mutex<(u64, Option<u64>)>,
}

impl DownloadWatch for Counted {
    fn received(&self, received: u64, announced: Option<u64>) {
        self.chunks.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut last) = self.last.lock() {
            *last = (received, announced);
        }
    }

    fn cancelled(&self) -> bool {
        false
    }
}

impl Counted {
    fn summary(&self) -> String {
        let (received, announced) = self.last.lock().map(|l| *l).unwrap_or_default();
        let announced = announced.map_or_else(
            || "no size announced".to_owned(),
            |n| format!("{n} bytes announced"),
        );
        format!(
            "{received} bytes in {} chunks, {announced}",
            self.chunks.load(Ordering::Relaxed)
        )
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .finish()
        .with(Targets::new().with_target("tc_door43", tracing::Level::DEBUG));
    if let Err(e) = subscriber.try_init() {
        eprintln!("the request log is off: {e}");
    }
    match run().await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<bool, Box<dyn Error>> {
    let server = match std::env::args().nth(1) {
        Some(url) => url.parse()?,
        None => ServerUrl::production(),
    };
    let internet = InternetSwitch::new();
    internet.turn_on();
    let client = Door43Client::new(server, internet)?;
    println!("server: {}", client.server());

    let mut passed = true;
    let start = Instant::now();
    passed &= report(
        "1. tag commit of en_tn v91",
        start,
        tag_commit(&client).await,
    );
    let start = Instant::now();
    passed &= report(
        "2. burrito export of en_twl v91",
        start,
        export(&client).await,
    );
    let start = Instant::now();
    passed &= report("3. git archive of en_ugl", start, archive(&client).await);
    let start = Instant::now();
    passed &= report("4. catalog search", start, catalog(&client).await);
    Ok(passed)
}

fn report(name: &str, start: Instant, outcome: Outcome) -> bool {
    let ms = start.elapsed().as_millis();
    match outcome {
        Ok(result) => {
            println!("PASS {name} ({ms} ms): {result}");
            true
        }
        Err(e) => {
            println!("FAIL {name} ({ms} ms): {e}");
            false
        }
    }
}

async fn tag_commit(client: &Door43Client) -> Outcome {
    let tag: TagName = "v91".parse()?;
    let commit = client.tag_commit(&TN.parse()?, &tag).await?;
    expect_commit(&commit, TN_V91)?;
    Ok(format!("{TN} {tag} is commit {commit}"))
}

async fn export(client: &Door43Client) -> Outcome {
    let expected: CommitSha = TWL_V91.parse()?;
    let folder = tempfile::tempdir()?;
    let counted = Counted::default();
    let to = DownloadTo::folder(folder.path()).watched_by(&counted);
    let export = client
        .download_sb_export(&TWL.parse()?, &"v91".parse()?, &expected, to)
        .await?;
    Ok(format!(
        "{}; revision {}, {} verified files, {} unlisted, {} listed but missing",
        counted.summary(),
        export.revision,
        export.files.len(),
        export.unlisted.len(),
        export.missing_ingredients.len()
    ))
}

async fn archive(client: &Door43Client) -> Outcome {
    let repo: RepoRef = UGL.parse()?;
    let folder = tempfile::tempdir()?;
    let counted = Counted::default();
    let to = DownloadTo::folder(folder.path()).watched_by(&counted);
    let archive = client
        .download_archive(&repo, &UGL_COMMIT.parse()?, to)
        .await?;
    let bytes: usize = archive.files.iter().map(|f| f.bytes.len()).sum();
    Ok(format!(
        "{}; commit {} has tree {}: {} files, {bytes} bytes unpacked (limit {})",
        counted.summary(),
        archive.commit,
        archive.tree,
        archive.files.len(),
        Limits::default().max_unpacked_bytes
    ))
}

async fn catalog(client: &Door43Client) -> Outcome {
    let query = CatalogQuery {
        owners: vec!["unfoldingWord".parse()?],
        repos: vec!["en_ult".parse()?],
        ..CatalogQuery::default()
    };
    let found = client.catalog_search(&query).await?;
    let entries: Vec<String> = found
        .items
        .iter()
        .map(|e| format!("{} {:?}", e.repo, e.title))
        .collect();
    Ok(format!(
        "{} entries ({}), {} skipped",
        entries.len(),
        entries.join("; "),
        found.skipped.len()
    ))
}

fn expect_commit(found: &CommitSha, expected: &str) -> Result<(), Box<dyn Error>> {
    if found.as_str() == expected {
        Ok(())
    } else {
        Err(format!("expected {expected}, found {found}").into())
    }
}
