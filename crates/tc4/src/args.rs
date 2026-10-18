//! The command line, parsed once at the edge into typed values.

use std::ffi::OsString;
use std::path::PathBuf;

use tc_door43::ServerUrl;

/// The usage line, after "tc4: <what was wrong>".
pub(crate) const USAGE: &str = if cfg!(feature = "test-hooks") {
    "usage: tc4 serve --app-data <dir> --ui <dir> [--projects <dir>] [--door43 <url>] [--test-hooks]"
} else {
    "usage: tc4 serve --app-data <dir> --ui <dir> [--projects <dir>] [--door43 <url>]"
};

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
    /// Route the `test` service (AD35). The flag exists only with the cargo feature.
    #[cfg(feature = "test-hooks")]
    pub test_hooks: bool,
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
    #[cfg(feature = "test-hooks")]
    let mut test_hooks = false;
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
            #[cfg(feature = "test-hooks")]
            "--test-hooks" => {
                if test_hooks {
                    return Err(ArgsError::Repeated("--test-hooks"));
                }
                test_hooks = true;
            }
            _ => return Err(ArgsError::UnknownFlag(flag.to_string_lossy().into_owned())),
        }
    }
    Ok(Args {
        app_data: PathBuf::from(app_data.ok_or(ArgsError::Required("--app-data"))?),
        ui: PathBuf::from(ui.ok_or(ArgsError::Required("--ui"))?),
        projects: projects.map(PathBuf::from),
        door43: door43.unwrap_or_else(default_door43),
        #[cfg(feature = "test-hooks")]
        test_hooks,
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
        ])
        .unwrap();
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
