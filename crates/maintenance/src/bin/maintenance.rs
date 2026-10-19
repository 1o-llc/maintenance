//! `maintenance`: runs the lines `maintenance::runner` declares, and nothing else.
//!
//! ```text
//! maintenance check <which> --workspace <root> [--registry <file>] [--decl <glob> --key <path>]
//! maintenance timed report <dir>
//! ```
//!
//! One JSON document on stdout; exit 0 clean, 1 findings, 2 refused.

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let outcome = maintenance::runner::run_argv(&args);
    println!("{}", outcome.stdout);
    std::process::exit(outcome.code);
}
