//! The program of the tC4 rebuild (APP-DESIGN §5.1): `tc4 serve --app-data <dir> --ui <dir>`
//! takes the app-data lock, starts the local server and prints its address as the first line
//! on stdout. The shell reads that line and then keeps stdin open: a closed stdin means the
//! shell is gone, and `tc4` quits.
//!
//! Exit codes: 0 a quit; 1 the server stopped with an error; 2 a usage error; 3 the app-data
//! lock is held; 4 the start failed; 86 a test crash.

mod args;

use std::ffi::OsString;
use std::io::Write as _;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use tc_server::stub::Stub;
use tc_server::{AppLock, AppLockError, Server, ServerConfig, Stdio};

use args::{Args, USAGE};

/// How long a new `tc4` waits for an old one that is still closing (AD39).
const LOCK_WAIT: Duration = Duration::from_secs(10);

const EXIT_SERVER_ERROR: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_LOCK_HELD: u8 = 3;
const EXIT_START_FAILED: u8 = 4;

/// Runs the program with its arguments (without the program's name). On the serve path it
/// ends the process itself, once the server has closed.
pub fn cli(args: impl Iterator<Item = OsString>) -> ExitCode {
    match args::parse(args) {
        Ok(args) => serve(&args),
        Err(error) => {
            eprintln!("tc4: {error}\n{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// One line of JSON on stdout, flushed. A closed stdout is the shell gone: not worth a panic.
fn say(line: &serde_json::Value) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{line}")?;
    out.flush()
}

fn start_failed(reason: impl std::fmt::Display) -> ExitCode {
    eprintln!("tc4: cannot start: {reason}");
    let line =
        serde_json::json!({"error": {"type": "start", "value": {"reason": reason.to_string()}}});
    let _ = say(&line);
    ExitCode::from(EXIT_START_FAILED)
}

fn serve(args: &Args) -> ExitCode {
    if let Err(error) = std::fs::create_dir_all(&args.app_data) {
        return start_failed(format_args!("{}: {error}", args.app_data.display()));
    }
    // Before anything is bound or printed: an old `tc4` still closing holds the lock, and a
    // second server beside it would be a second writer of the same folders.
    let lock = match AppLock::take(&args.app_data, LOCK_WAIT) {
        Ok(lock) => lock,
        Err(AppLockError::Held(holder)) => {
            eprintln!(
                "tc4: {holder} still holds {} after {} s; it may still be closing. \
                 End it, then start translationCore again.",
                args.app_data.join("app.lock").display(),
                LOCK_WAIT.as_secs()
            );
            let line =
                serde_json::json!({"error": {"type": "appLockHeld", "value": {"pid": holder.pid}}});
            let _ = say(&line);
            return ExitCode::from(EXIT_LOCK_HELD);
        }
        Err(error @ AppLockError::Io { .. }) => return start_failed(error),
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return start_failed(error),
    };
    let mut config = ServerConfig::new(args.app_data.clone(), args.ui.clone());
    config.handle_signals = true;
    let server = match Server::bind(config) {
        Ok(server) => server,
        Err(error) => return start_failed(error),
    };
    #[cfg(feature = "test-hooks")]
    let server = if args.test_hooks {
        server.with_test_hooks()
    } else {
        server
    };
    // Nothing else may be on stdout before this line, and the token is in no other place.
    if say(&serde_json::json!({"url": server.address().url, "token": server.address().token}))
        .is_err()
    {
        eprintln!("tc4: stdout is closed; the shell is gone");
        return ExitCode::from(EXIT_SERVER_ERROR);
    }
    eprintln!(
        "tc4: serving {} (projects: {}, Door43: {})",
        server.address().url,
        args.projects.as_ref().map_or_else(
            || "the default folder".to_owned(),
            |p| p.display().to_string()
        ),
        args.door43
    );
    let stub = match Stub::new() {
        Ok(stub) => stub,
        Err(error) => return start_failed(error),
    };
    let stdio = Stdio {
        input: tokio::io::BufReader::new(tokio::io::stdin()),
        output: tokio::io::stdout(),
    };
    let result = runtime.block_on(server.run(Arc::new(stub), stdio));
    // The lock is held to here. The process ends by `exit`, not by dropping the runtime:
    // tokio reads stdin on a blocking-pool thread that stays in `read()` while the shell keeps
    // stdin open, and dropping the runtime waits for blocking tasks, so it would never return.
    // The operating system releases the lock when the process ends.
    let _lock = lock;
    match result {
        Ok(_) => std::process::exit(0),
        Err(error) => {
            eprintln!("tc4: {error}");
            std::process::exit(i32::from(EXIT_SERVER_ERROR));
        }
    }
}
