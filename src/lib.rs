//! Library support for the `catnap` command-line application.

mod cli;
mod clock;
mod duration;
mod format;
mod runner;

use std::{ffi::OsString, io::Write, process::ExitCode};

pub use cli::{CliError, Command, CommandAction, parse_command};
pub use clock::{ClockConfigError, MonotonicClock, MonotonicTimestamp, RealMonotonicClock};
pub use duration::{DurationParseError, parse_sleep_duration, report_interval};
pub use format::format_remaining_time;
pub use runner::{RunConfig, run_sleep};

const HELP: &str = concat!(
    "Usage: catnap NUMBER[SUFFIX]...\n",
    "  or:  catnap OPTION\n",
    "Pause for the sum of the requested durations while reporting remaining time.\n\n",
    "Each NUMBER may be followed by a suffix:\n",
    "  s  seconds (default)\n",
    "  m  minutes\n",
    "  h  hours\n",
    "  d  days\n\n",
    "Options:\n",
    "      --help     display this help and exit\n",
    "      --version  output version information and exit\n",
);

const VERSION: &str = concat!("catnap ", env!("CARGO_PKG_VERSION"), "\n");

/// Run the `catnap` application with injectable streams.
///
/// # Examples
///
/// ```
/// use std::ffi::OsString;
///
/// use catnap::run_application;
///
/// let mut stdout = Vec::new();
/// let mut stderr = Vec::new();
/// // `ExitCode` exposes neither a success predicate nor equality, so the
/// // example asserts the observable stream behaviour instead.
/// let _status = run_application(
///     [OsString::from("catnap"), OsString::from("--help")],
///     &mut stdout,
///     &mut stderr,
/// );
///
/// assert!(!stdout.is_empty());
/// assert!(stderr.is_empty());
/// ```
pub fn run_application<I, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
    W: Write,
    E: Write,
{
    match parse_command(args) {
        Ok(CommandAction::Help) => write_success(stdout, HELP),
        Ok(CommandAction::Version) => write_success(stdout, VERSION),
        Ok(CommandAction::Sleep(command)) => run_command(&command, stderr),
        Err(error) => write_cli_error(&error, stderr),
    }
}

fn run_command<E>(command: &Command, stderr: &mut E) -> ExitCode
where
    E: Write,
{
    match RealMonotonicClock::new(command.logical_second()) {
        Ok(mut clock) => {
            let locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_owned());
            let config = RunConfig::new(command.duration(), locale);
            match run_sleep(&mut clock, stderr, &config) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => write_io_error(&error, stderr),
            }
        }
        Err(error) => write_clock_error(&error, stderr),
    }
}

fn write_success<W>(stdout: &mut W, text: &str) -> ExitCode
where
    W: Write,
{
    match stdout.write_all(text.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn write_cli_error<E>(error: &CliError, stderr: &mut E) -> ExitCode
where
    E: Write,
{
    write_error(
        stderr,
        &format!("catnap: {error}\nTry 'catnap --help' for more information.\n"),
    )
}

fn write_clock_error<E>(error: &ClockConfigError, stderr: &mut E) -> ExitCode
where
    E: Write,
{
    write_error(stderr, &format!("catnap: {error}\n"))
}

fn write_io_error<E>(error: &std::io::Error, stderr: &mut E) -> ExitCode
where
    E: Write,
{
    write_error(
        stderr,
        &format!("catnap: failed to write progress: {error}\n"),
    )
}

fn write_error<E>(stderr: &mut E, text: &str) -> ExitCode
where
    E: Write,
{
    match stderr.write_all(text.as_bytes()) {
        Ok(()) | Err(_) => ExitCode::FAILURE,
    }
}
