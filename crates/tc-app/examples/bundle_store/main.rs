//! Builds the bundled store an installer ships from `bundle/en` (unit A9):
//!
//! ```text
//! cargo run -p tc-app --example bundle_store -- --from bundle/en --out <dir> [--only en_tw,en_obs]
//! ```
//!
//! `--only` names repositories, as `en_tw`; without it every pin is built. Each install, each
//! skip and the count of files are printed. `tc4 serve --bundle <dir>` seeds from the result.

mod build;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("bundle_store: {message}");
            eprintln!("usage: bundle_store --from <dir> --out <dir> [--only <repository>,...]");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let mut from = None;
    let mut out = None;
    let mut only: Option<Vec<String>> = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--from" => from = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--only" => only = Some(value()?.split(',').map(str::to_owned).collect()),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    let from = from.ok_or("--from is required")?;
    let out = out.ok_or("--out is required")?;
    let wanted = |pin: &tc_resources::ResourcePin| {
        only.as_ref()
            .is_none_or(|names| names.iter().any(|name| name == pin.repo.name().as_str()))
    };
    let built = build::build_bundle(&from, &out, &wanted)?;
    for name in &built.installed {
        println!("installed {name}");
    }
    for (name, why) in &built.skipped {
        println!("skipped   {name}: {why}");
    }
    println!(
        "{} installs, {} skipped, {} content files in {}",
        built.installed.len(),
        built.skipped.len(),
        built.files,
        out.display()
    );
    Ok(())
}
