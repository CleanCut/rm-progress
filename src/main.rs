#[cfg(not(unix))]
compile_error!("rmp runs on Linux and macOS only");

#[cfg(unix)]
use std::env;
#[cfg(unix)]
use std::process::ExitCode;

#[cfg(unix)]
fn main() -> ExitCode {
    rm_progress::run(env::args_os().skip(1))
}

#[cfg(not(unix))]
fn main() {}
