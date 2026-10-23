use std::process::ExitCode;

fn main() -> ExitCode {
    tc4::cli(std::env::args_os().skip(1))
}
