//! The command line, parsed once at the edge into typed values.

use std::ffi::OsString;
use std::path::PathBuf;
#[cfg(feature = "test-hooks")]
use std::time::Duration;

use tc_door43::ServerUrl;

#[cfg(feature = "test-hooks")]
macro_rules! hook_flags {
    () => {
        " [--test-hooks] [--real-sync] [--test-seed-pace <ms>]"
    };
}
#[cfg(not(feature = "test-hooks"))]
macro_rules! hook_flags {
    () => {
        ""
    };
}
#[cfg(feature = "fixture-backend")]
macro_rules! fixture_flags {
    () => {
        " [--scenario <state dir> [--backend memory]]"
    };
}
#[cfg(not(feature = "fixture-backend"))]
macro_rules! fixture_flags {
    () => {
        ""
    };
}

/// The usage line, after "tc4: <what was wrong>". It names only the flags this build has.
pub(crate) const USAGE: &str = concat!(
    "usage: tc4 serve --app-data <dir> --ui <dir> [--projects <dir>] [--door43 <url>] ",
    "[--bundle <dir>]",
    hook_flags!(),
    fixture_flags!()
);

/// What `tc4 serve` was asked to do.
#[derive(Debug)]
pub(crate) struct Args {
    /// Where the app keeps its own files, and its lock.
    pub app_data: PathBuf,
    /// The built UI: `index.html`, `ui/`, `help/`, `prepare.html`.
    pub ui: PathBuf,
    /// The projects folder; `None`: the app chooses its default (Q-4).
    pub projects: Option<PathBuf>,
    /// The Door43 server: QA in a development build, production otherwise (§5.6 item 9).
    pub door43: ServerUrl,
    /// The bundled resource store the installer ships, read only; `None`: nothing is seeded
    /// (development, tests, the parity captures).
    pub bundle: Option<PathBuf>,
    /// Route the `test` service (AD35). The flag exists only with the cargo feature.
    #[cfg(feature = "test-hooks")]
    pub test_hooks: bool,
    /// `--real-sync`: with the test hooks, sync and open from Door43 go to `--door43` for real
    /// (a fake Door43) instead of the scripted port. Exists only with the cargo feature.
    #[cfg(feature = "test-hooks")]
    pub real_sync: bool,
    /// `--test-seed-pace <ms>`: the seed waits this long before each file, so a process test
    /// can watch it and close it on any disk. Exists only with the cargo feature.
    #[cfg(feature = "test-hooks")]
    pub seed_pace: Option<Duration>,
    /// `--scenario <state dir>`: the projects are a parity scenario's, written into the
    /// `--projects` folder through the store (or held by the memory backend with `--backend
    /// memory`), and its sideloaded resources are installed into the app's store (APP-DESIGN
    /// §4.2, §5.1). Exists only with the cargo feature `fixture-backend`.
    #[cfg(feature = "fixture-backend")]
    pub scenario: Option<PathBuf>,
    /// What holds a scenario's projects. Exists only with the cargo feature
    /// `fixture-backend`.
    #[cfg(feature = "fixture-backend")]
    pub backend: ScenarioBackend,
}

/// What holds a scenario's projects (APP-DESIGN §5.1, unit C1a).
#[cfg(feature = "fixture-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ScenarioBackend {
    /// The projects are written into the `--projects` folder and opened through tc-store.
    #[default]
    Store,
    /// `--backend memory`: held in memory; nothing is written (phase 1's environment).
    Memory,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ArgsError {
    #[error("no command; the only one is `serve`")]
    NoCommand,
    #[error("unknown command {0}; the only one is `serve`")]
    UnknownCommand(String),
    #[error("unknown flag {0}")]
    UnknownFlag(String),
    #[error("{0} needs a value")]
    MissingValue(&'static str),
    #[error("{0} was given twice")]
    Repeated(&'static str),
    #[error("{0} is required")]
    Required(&'static str),
    #[error("{flag}: {reason}")]
    BadValue { flag: &'static str, reason: String },
}

pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Args, ArgsError> {
    match args.next() {
        None => return Err(ArgsError::NoCommand),
        Some(command) if command == "serve" => {}
        Some(command) => {
            return Err(ArgsError::UnknownCommand(
                command.to_string_lossy().into_owned(),
            ));
        }
    }
    let mut app_data = None;
    let mut ui = None;
    let mut projects = None;
    let mut door43 = None;
    let mut bundle = None;
    #[cfg(feature = "test-hooks")]
    let mut test_hooks = false;
    #[cfg(feature = "test-hooks")]
    let mut seed_pace = None;
    #[cfg(feature = "test-hooks")]
    let mut real_sync = false;
    #[cfg(feature = "fixture-backend")]
    let mut scenario = None;
    #[cfg(feature = "fixture-backend")]
    let mut backend = None;
    while let Some(flag) = args.next() {
        let name = flag.to_str().unwrap_or_default();
        match name {
            "--app-data" => once("--app-data", &mut app_data, value("--app-data", &mut args)?)?,
            "--ui" => once("--ui", &mut ui, value("--ui", &mut args)?)?,
            "--projects" => once("--projects", &mut projects, value("--projects", &mut args)?)?,
            "--door43" => {
                let text = value("--door43", &mut args)?;
                let url = text
                    .to_str()
                    .ok_or_else(|| bad("--door43", "not text"))?
                    .parse::<ServerUrl>()
                    .map_err(|error| bad("--door43", error))?;
                once("--door43", &mut door43, url)?;
            }
            "--bundle" => once("--bundle", &mut bundle, value("--bundle", &mut args)?)?,
            #[cfg(feature = "test-hooks")]
            "--test-hooks" => {
                if test_hooks {
                    return Err(ArgsError::Repeated("--test-hooks"));
                }
                test_hooks = true;
            }
            #[cfg(feature = "test-hooks")]
            "--real-sync" => {
                if real_sync {
                    return Err(ArgsError::Repeated("--real-sync"));
                }
                real_sync = true;
            }
            #[cfg(feature = "test-hooks")]
            "--test-seed-pace" => {
                let text = value("--test-seed-pace", &mut args)?;
                let ms = text
                    .to_str()
                    .ok_or_else(|| bad("--test-seed-pace", "not text"))?
                    .parse::<u64>()
                    .map_err(|_| bad("--test-seed-pace", "a whole number of milliseconds"))?;
                once(
                    "--test-seed-pace",
                    &mut seed_pace,
                    Duration::from_millis(ms),
                )?;
            }
            #[cfg(feature = "fixture-backend")]
            "--scenario" => once("--scenario", &mut scenario, value("--scenario", &mut args)?)?,
            #[cfg(feature = "fixture-backend")]
            "--backend" => {
                let chosen = match value("--backend", &mut args)?.to_str() {
                    Some("memory") => ScenarioBackend::Memory,
                    Some("store") => ScenarioBackend::Store,
                    _ => return Err(bad("--backend", "memory or store")),
                };
                once("--backend", &mut backend, chosen)?;
            }
            _ => return Err(ArgsError::UnknownFlag(flag.to_string_lossy().into_owned())),
        }
    }
    #[cfg(feature = "fixture-backend")]
    if backend.is_some() && scenario.is_none() {
        return Err(bad("--backend", "needs --scenario"));
    }
    #[cfg(feature = "fixture-backend")]
    let backend = backend.unwrap_or_default();
    #[cfg(feature = "fixture-backend")]
    if scenario.is_some() && backend == ScenarioBackend::Store && projects.is_none() {
        return Err(bad(
            "--scenario",
            "needs --projects: the scenario writes its projects there",
        ));
    }
    Ok(Args {
        app_data: PathBuf::from(app_data.ok_or(ArgsError::Required("--app-data"))?),
        ui: PathBuf::from(ui.ok_or(ArgsError::Required("--ui"))?),
        projects: projects.map(PathBuf::from),
        door43: door43.unwrap_or_else(default_door43),
        bundle: bundle.map(PathBuf::from),
        #[cfg(feature = "test-hooks")]
        test_hooks,
        #[cfg(feature = "test-hooks")]
        real_sync,
        #[cfg(feature = "test-hooks")]
        seed_pace,
        #[cfg(feature = "fixture-backend")]
        scenario: scenario.map(PathBuf::from),
        #[cfg(feature = "fixture-backend")]
        backend,
    })
}

fn default_door43() -> ServerUrl {
    if cfg!(debug_assertions) {
        ServerUrl::qa()
    } else {
        ServerUrl::production()
    }
}

/// The next argument as the value of `flag`. A value that looks like a flag is a forgotten
/// value: `--app-data --ui x` would otherwise name a folder `--ui`.
fn value(
    flag: &'static str,
    args: &mut impl Iterator<Item = OsString>,
) -> Result<OsString, ArgsError> {
    match args.next() {
        Some(value) if !value.to_string_lossy().starts_with("--") => Ok(value),
        _ => Err(ArgsError::MissingValue(flag)),
    }
}

fn once<T>(flag: &'static str, slot: &mut Option<T>, value: T) -> Result<(), ArgsError> {
    if slot.replace(value).is_some() {
        return Err(ArgsError::Repeated(flag));
    }
    Ok(())
}

fn bad(flag: &'static str, reason: impl std::fmt::Display) -> ArgsError {
    ArgsError::BadValue {
        flag,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(args: &[&str]) -> Result<Args, ArgsError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn a_full_command_line_parses_into_typed_values() {
        let args = parse_all(&[
            "serve",
            "--app-data",
            "/a",
            "--ui",
            "/u",
            "--projects",
            "/p",
            "--door43",
            "http://127.0.0.1:3000",
            "--bundle",
            "/b",
        ])
        .unwrap();
        assert_eq!(args.bundle, Some(PathBuf::from("/b")));
        assert_eq!(args.app_data, PathBuf::from("/a"));
        assert_eq!(args.ui, PathBuf::from("/u"));
        assert_eq!(args.projects, Some(PathBuf::from("/p")));
        assert_eq!(args.door43.to_string(), "http://127.0.0.1:3000");
    }

    #[test]
    fn the_door43_server_defaults_by_build() {
        let args = parse_all(&["serve", "--app-data", "/a", "--ui", "/u"]).unwrap();
        let want = if cfg!(debug_assertions) {
            ServerUrl::qa()
        } else {
            ServerUrl::production()
        };
        assert_eq!(args.door43, want);
        assert_eq!(args.projects, None);
        assert_eq!(args.bundle, None, "without a bundle nothing is seeded");
    }

    #[test]
    fn the_bundle_needs_one_value_and_is_given_once() {
        let with = |extra: &[&str]| {
            let mut all = vec!["serve", "--app-data", "/a", "--ui", "/u"];
            all.extend_from_slice(extra);
            parse_all(&all).map(|args| args.bundle)
        };
        assert_eq!(
            with(&["--bundle"]),
            Err(ArgsError::MissingValue("--bundle"))
        );
        assert_eq!(
            with(&["--bundle", "--ui"]),
            Err(ArgsError::MissingValue("--bundle"))
        );
        assert_eq!(
            with(&["--bundle", "/b", "--bundle", "/c"]),
            Err(ArgsError::Repeated("--bundle"))
        );
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn the_seed_pace_is_a_whole_number_of_milliseconds() {
        let with = |extra: &[&str]| {
            let mut all = vec!["serve", "--app-data", "/a", "--ui", "/u"];
            all.extend_from_slice(extra);
            parse_all(&all).map(|args| args.seed_pace)
        };
        assert_eq!(with(&[]), Ok(None));
        assert_eq!(
            with(&["--test-seed-pace", "30"]),
            Ok(Some(Duration::from_millis(30)))
        );
        for text in ["fast", "1.5", "-3", ""] {
            assert!(
                matches!(
                    with(&["--test-seed-pace", text]),
                    Err(ArgsError::BadValue {
                        flag: "--test-seed-pace",
                        ..
                    } | ArgsError::MissingValue(_))
                ),
                "{text:?}"
            );
        }
        assert_eq!(
            with(&["--test-seed-pace", "1", "--test-seed-pace", "2"]),
            Err(ArgsError::Repeated("--test-seed-pace"))
        );
    }

    #[cfg(feature = "fixture-backend")]
    #[test]
    fn the_scenario_needs_one_value_and_is_given_once() {
        let with = |extra: &[&str]| {
            let mut all = vec!["serve", "--app-data", "/a", "--ui", "/u"];
            all.extend_from_slice(extra);
            parse_all(&all).map(|args| args.scenario)
        };
        assert_eq!(with(&[]), Ok(None));
        assert_eq!(
            with(&["--projects", "/p", "--scenario", "/s"]),
            Ok(Some(PathBuf::from("/s")))
        );
        assert_eq!(
            with(&["--scenario"]),
            Err(ArgsError::MissingValue("--scenario"))
        );
        assert_eq!(
            with(&["--scenario", "/s", "--scenario", "/t"]),
            Err(ArgsError::Repeated("--scenario"))
        );
        assert!(USAGE.contains("[--scenario <state dir> [--backend memory]]"));
    }

    /// On the store a scenario writes its projects into the `--projects` folder, so it needs
    /// one; `--backend memory` keeps phase 1's memory backend, which writes nothing.
    #[cfg(feature = "fixture-backend")]
    #[test]
    fn the_scenario_backend_is_the_store_unless_memory_is_asked_for() {
        let with = |extra: &[&str]| {
            let mut all = vec!["serve", "--app-data", "/a", "--ui", "/u"];
            all.extend_from_slice(extra);
            parse_all(&all).map(|args| args.backend)
        };
        assert_eq!(
            with(&["--projects", "/p", "--scenario", "/s"]),
            Ok(ScenarioBackend::Store)
        );
        assert_eq!(
            with(&["--scenario", "/s", "--backend", "memory"]),
            Ok(ScenarioBackend::Memory)
        );
        assert_eq!(
            with(&["--scenario", "/s"]),
            Err(ArgsError::BadValue {
                flag: "--scenario",
                reason: "needs --projects: the scenario writes its projects there".into()
            })
        );
        assert_eq!(
            with(&["--backend", "memory"]),
            Err(ArgsError::BadValue {
                flag: "--backend",
                reason: "needs --scenario".into()
            })
        );
        assert_eq!(
            with(&["--scenario", "/s", "--backend", "disk"]),
            Err(ArgsError::BadValue {
                flag: "--backend",
                reason: "memory or store".into()
            })
        );
        assert_eq!(
            with(&[
                "--scenario",
                "/s",
                "--backend",
                "memory",
                "--backend",
                "memory"
            ]),
            Err(ArgsError::Repeated("--backend"))
        );
    }

    #[cfg(not(feature = "fixture-backend"))]
    #[test]
    fn a_build_without_the_fixture_backend_knows_no_scenario() {
        let refused = parse_all(&[
            "serve",
            "--app-data",
            "/a",
            "--ui",
            "/u",
            "--scenario",
            "/s",
        ]);
        assert_eq!(
            refused.unwrap_err(),
            ArgsError::UnknownFlag("--scenario".into())
        );
        let backend = parse_all(&[
            "serve",
            "--app-data",
            "/a",
            "--ui",
            "/u",
            "--backend",
            "memory",
        ]);
        assert_eq!(
            backend.unwrap_err(),
            ArgsError::UnknownFlag("--backend".into())
        );
        assert!(!USAGE.contains("--scenario"));
        assert!(!USAGE.contains("--backend"));
    }

    #[test]
    fn wrong_command_lines_are_refused_with_the_reason() {
        let cases: [(&[&str], ArgsError); 8] = [
            (&[], ArgsError::NoCommand),
            (&["start"], ArgsError::UnknownCommand("start".into())),
            (&["serve", "--ui", "/u"], ArgsError::Required("--app-data")),
            (&["serve", "--app-data", "/a"], ArgsError::Required("--ui")),
            (
                &["serve", "--app-data", "/a", "--ui", "/u", "--nope"],
                ArgsError::UnknownFlag("--nope".into()),
            ),
            (
                &["serve", "--app-data"],
                ArgsError::MissingValue("--app-data"),
            ),
            (
                &["serve", "--app-data", "--ui", "/u"],
                ArgsError::MissingValue("--app-data"),
            ),
            (
                &["serve", "--app-data", "/a", "--app-data", "/b"],
                ArgsError::Repeated("--app-data"),
            ),
        ];
        for (args, want) in cases {
            assert_eq!(parse_all(args).unwrap_err(), want, "{args:?}");
        }
        let bad_url = parse_all(&[
            "serve",
            "--app-data",
            "/a",
            "--ui",
            "/u",
            "--door43",
            "ftp://x",
        ]);
        assert!(matches!(
            bad_url,
            Err(ArgsError::BadValue {
                flag: "--door43",
                ..
            })
        ));
    }
}
