use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    rm_progress::run(env::args_os().skip(1))
}
