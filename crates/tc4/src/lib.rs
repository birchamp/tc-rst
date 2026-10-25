//! The program of the tC4 rebuild (APP-DESIGN §5.1): `tc4 serve --app-data <dir> --ui <dir>`
//! takes the app-data lock, starts the local server and prints its address as the first line
//! on stdout. The shell reads that line and then keeps stdin open: a closed stdin means the
//! shell is gone, and `tc4` quits.
//!
//! Exit codes: 0 a quit; 1 the server stopped with an error; 2 a usage error; 3 the app-data
//! lock is held; 4 the start failed; 86 a test crash.

mod args;
mod join;

use std::ffi::OsString;
use std::io::Write as _;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use tc_api::app::{BUILD_COMMIT, BUILD_VERSION, BuildInfo};
use tc_app::config::{AppConfig, ProjectsFolder};
use tc_app::{App, AppDeps};
use tc_server::{AppLock, AppLockError, Server, ServerConfig, Stdio};

use args::{Args, USAGE};
use join::{AppBackend, ServerUploads};

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

/// What the app is told at start. Without `--projects` the app chooses its default folder
/// (Q-4).
fn app_config(args: &Args) -> AppConfig {
    AppConfig {
        app_data: args.app_data.clone(),
        projects: args
            .projects
            .clone()
            .map_or(ProjectsFolder::Default, ProjectsFolder::Given),
        door43: args.door43.clone(),
        build: BuildInfo {
            version: BUILD_VERSION.to_owned(),
            commit: BUILD_COMMIT.to_owned(),
        },
        bundle: args.bundle.clone(),
        #[cfg(feature = "test-hooks")]
        test_hooks: args.test_hooks,
        #[cfg(feature = "test-hooks")]
        seed_pace: args.seed_pace,
    }
}

/// `--scenario <dir>`: the parity scenario's projects, and its sideloaded resources in the
/// app's store, before the app starts (APP-DESIGN §4.2, §5.1, §7.1). The projects are written
/// into the `--projects` folder through tc-store (unit C1a), which must be an empty folder: a
/// scenario never writes into a folder that holds anyone's work. `--backend memory` holds them
/// in memory instead and writes nothing. Each project is named on stderr, loaded or refused
/// with every mismatch: a refused project shows nowhere, and the harness's log says why.
#[cfg(feature = "fixture-backend")]
fn scenario_deps(dir: &std::path::Path, args: &Args) -> Result<AppDeps, String> {
    use tc_app::backend::ProjectStore;
    use tc_app::backend::memory::MemoryBackend;
    use tc_app::backend::scenario::{ScenarioState, install_resources_into};
    use tc_app::backend::scenario_store::ScenarioStore;
    use tc_app::backend::seed::SeedRefusal;
    use tc_app::backend::store::StoreBackend;

    let at = dir.display();
    let failed =
        |error: tc_app::backend::scenario::ScenarioError| format!("scenario {at}: {error}");
    // The scenario first, so a missing scenario folder is the error that names it.
    let state = ScenarioState::read(dir).map_err(failed)?;
    let (store, report, written): (Arc<dyn ProjectStore>, _, _) = match args.backend {
        args::ScenarioBackend::Memory => {
            let (backend, report) = MemoryBackend::from_scenario(&state).map_err(failed)?;
            (Arc::new(backend), report, None)
        }
        args::ScenarioBackend::Store => {
            // `args::parse` refuses a scenario on the store without `--projects`.
            let projects = args
                .projects
                .as_deref()
                .ok_or_else(|| format!("scenario {at}: needs --projects"))?;
            // Before the store starts: tc-store's start makes a missing projects folder, and a
            // folder named on the command line that is not there stops the start (§5.1).
            let entries = std::fs::read_dir(projects)
                .map_err(|e| format!("scenario {at}: {}: {e}", projects.display()))?;
            if entries.count() > 0 {
                return Err(format!(
                    "scenario {at}: the projects folder {} is not empty: a scenario writes its \
                     projects only into an empty folder",
                    projects.display()
                ));
            }
            let backend = StoreBackend::on_disk(&args.app_data, projects)
                .map_err(|e| format!("scenario {at}: {e}"))?;
            let (store, report) = ScenarioStore::write(backend, &state).map_err(failed)?;
            (Arc::new(store), report, Some(projects))
        }
    };
    for (folder, seed) in &report.loaded {
        eprintln!(
            "tc4: scenario {at}: loaded {folder} ({} declared differences)",
            seed.declared.len()
        );
    }
    for (folder, refusal) in &report.refused {
        match refusal {
            SeedRefusal::Mismatches(mismatches) => {
                eprintln!(
                    "tc4: scenario {at}: refused {folder}, {} mismatches \
                     (docs/evidence/state-seed-check-2026-10-18.md):",
                    mismatches.len()
                );
                for mismatch in mismatches {
                    eprintln!("tc4:   {mismatch}");
                }
            }
            other => eprintln!("tc4: scenario {at}: refused {folder}: {other}"),
        }
    }
    if let Some(projects) = written {
        eprintln!(
            "tc4: scenario {at}: {} projects written to {}",
            report.loaded.len(),
            projects.display()
        );
    }
    let installed = install_resources_into(&args.app_data, &state.resources().map_err(failed)?)
        .map_err(failed)?;
    eprintln!(
        "tc4: scenario {at}: {} resources installed",
        installed.len()
    );
    Ok(AppDeps {
        store: tc_app::StoreChoice::Given(store),
        ..AppDeps::production()
    })
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
    #[cfg(feature = "fixture-backend")]
    let deps = match &args.scenario {
        Some(dir) => match scenario_deps(dir, args) {
            Ok(deps) => deps,
            Err(error) => return start_failed(error),
        },
        None => AppDeps::production(),
    };
    #[cfg(not(feature = "fixture-backend"))]
    let deps = AppDeps::production();
    // A test build's journeys sign in with "Stay signed in" and set what the keychain holds
    // (`test/keychain`), so with the hooks on the app keeps its sign-in in a test keychain.
    // The product has none until E1a (`AppDeps::production`).
    #[cfg(feature = "test-hooks")]
    let deps = if args.test_hooks {
        let keychain = Arc::new(tc_app::account::MemKeychain::new());
        AppDeps {
            keychain: keychain.clone(),
            test_keychain: Some(keychain),
            // The journeys script how a sync or an open from Door43 ends (UI-2), or, with
            // `--real-sync`, sync with the fake Door43 for real (E1a).
            sync: if args.real_sync {
                Arc::new(tc_app::ports::door43::Door43Sync)
            } else {
                Arc::new(tc_app::ports::scripted::ScriptedSyncPort::default())
            },
            ..deps
        }
    } else {
        deps
    };
    // The app starts before the address line, so a start that fails (a `--projects` folder
    // that is not there, say) prints its one error line and nothing else: the shell reads the
    // first line, and an address followed by an error would send it to a server that is gone.
    let app = match runtime.block_on(App::start(app_config(args), deps)) {
        Ok(app) => app,
        Err(error) => return start_failed(error),
    };
    // The guided fix installs a file the person chose, which the server stored.
    app.provide_uploads(Arc::new(ServerUploads(Arc::clone(
        server.handle().uploads(),
    ))));
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
        app.projects_root().path.display(),
        args.door43
    );
    let stdio = Stdio {
        input: tokio::io::BufReader::new(tokio::io::stdin()),
        output: tokio::io::stdout(),
    };
    let result = runtime.block_on(server.run(Arc::new(AppBackend(app)), stdio));
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
