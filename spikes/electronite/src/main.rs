//! `tc4-standin`: a stand-in for `tc4 serve`, for the Electronite spike G0 only.
//!
//! The real server (unit A2) is being built at the same time, so the spike brings this small
//! server on the same stack the design names (docs/APP-DESIGN.md §5.1, §5.2): axum on
//! `127.0.0.1` with a port the OS picks, a per-launch token that every request must carry in
//! `X-Tc-Token` (ARCHITECTURE.md, "Security of the local server"), the `Host` and `Origin`
//! checks, one JSON address line on stdout before anything else, and a quit when stdin closes
//! or says `{"quit":true}`. It serves exactly what the spike's probes need. It is never
//! shipped, and its loopback cost is not `tc4 serve`'s: unit A2 measures that.
//!
//! ```text
//! tc4-standin serve --app-data DIR [--port N] [--reply-bytes N]
//! tc4-standin machine
//! ```

#![forbid(unsafe_code)]

mod content;
mod png;
mod web;

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use tokio::io::AsyncBufReadExt as _;

/// What a launch was asked to do.
enum Command {
    Serve(ServeOptions),
    Machine,
}

struct ServeOptions {
    app_data: PathBuf,
    port: u16,
    reply_bytes: usize,
}

fn parse(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        Some("machine") => Ok(Command::Machine),
        Some("serve") => {
            let mut app_data = None;
            let mut port = 0;
            let mut reply_bytes = content::REPLY_BYTES;
            let mut rest = args.iter().skip(1);
            while let Some(flag) = rest.next() {
                let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--app-data" => app_data = Some(PathBuf::from(value)),
                    "--port" => port = value.parse().map_err(|_| "--port needs a number")?,
                    "--reply-bytes" => {
                        reply_bytes = value.parse().map_err(|_| "--reply-bytes needs a number")?;
                    }
                    other => return Err(format!("unknown flag {other}")),
                }
            }
            let app_data = app_data.ok_or("serve needs --app-data DIR")?;
            Ok(Command::Serve(ServeOptions {
                app_data,
                port,
                reply_bytes,
            }))
        }
        _ => Err(
            "usage: tc4-standin serve --app-data DIR [--port N] [--reply-bytes N] | machine"
                .to_owned(),
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Command::Machine) => {
            println!("{}", machine_json());
            ExitCode::SUCCESS
        }
        Ok(Command::Serve(options)) => match serve(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("tc4-standin: {error}");
                ExitCode::FAILURE
            }
        },
        Err(usage) => {
            eprintln!("{usage}");
            ExitCode::from(2)
        }
    }
}

/// The CPU features that move the storage budgets (PERFORMANCE.md, "Budgets").
fn machine_json() -> String {
    #[cfg(target_arch = "x86_64")]
    let features = serde_json::json!({
        "arch": "x86_64",
        "sha": std::is_x86_feature_detected!("sha"),
        "avx2": std::is_x86_feature_detected!("avx2"),
        "avx512f": std::is_x86_feature_detected!("avx512f"),
        "avx512bw": std::is_x86_feature_detected!("avx512bw"),
        "avx512vl": std::is_x86_feature_detected!("avx512vl"),
        "threads": std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
    });
    #[cfg(not(target_arch = "x86_64"))]
    let features = serde_json::json!({
        "arch": std::env::consts::ARCH,
        "threads": std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
    });
    features.to_string()
}

fn serve(options: ServeOptions) -> Result<(), String> {
    std::fs::create_dir_all(&options.app_data)
        .map_err(|e| format!("cannot create {}: {e}", options.app_data.display()))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the runtime: {e}"))?;
    runtime.block_on(run(options))
}

async fn run(options: ServeOptions) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", options.port))
        .await
        .map_err(|e| format!("cannot bind 127.0.0.1:{}: {e}", options.port))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("no local address: {e}"))?
        .port();
    let token = web::new_token()?;
    let shared = Arc::new(web::Shared::new(
        &options.app_data,
        port,
        token,
        options.reply_bytes,
    )?);
    // The address line comes before anything else on stdout (§5.1). The token is never logged.
    {
        let mut out = std::io::stdout().lock();
        writeln!(out, "{}", shared.address_line()).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    shared.log_event("started", serde_json::json!({ "port": port }));
    // Pictures are made once, off the request path, so the first image probe pays nothing.
    tokio::task::spawn_blocking(|| {
        let _ = content::picture();
    });
    tokio::spawn(watch_stdin(Arc::clone(&shared)));
    axum::serve(listener, web::router(shared))
        .await
        .map_err(|e| format!("server stopped: {e}"))
}

/// Quits on `{"quit":true}` and when stdin closes: the shell that started this process died
/// or let go of it (AD39).
async fn watch_stdin(shared: Arc<web::Shared>) {
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let why = loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let quit = serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .and_then(|v| v.get("quit").and_then(serde_json::Value::as_bool))
                    .unwrap_or(false);
                if quit {
                    break "quit-message";
                }
            }
            Ok(None) => break "stdin-closed",
            Err(_) => break "stdin-error",
        }
    };
    shared.log_event("quit", serde_json::json!({ "why": why }));
    if why == "quit-message" {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{{\"closed\":true}}");
        let _ = out.flush();
    }
    std::process::exit(0);
}
