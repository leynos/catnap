//! `catnap` application entry point.

use std::process::ExitCode;

/// Application entry point.
fn main() -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    catnap::run_application(std::env::args_os(), &mut stdout, &mut stderr)
}
