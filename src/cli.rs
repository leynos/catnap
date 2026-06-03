//! Command-line parsing for GNU-like sleep operands.

use std::{ffi::OsString, time::Duration};

use crate::duration::{DurationParseError, parse_sleep_duration};

/// Parsed command-line action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    /// Display help and exit successfully.
    Help,
    /// Display version information and exit successfully.
    Version,
    /// Run the sleep stopwatch.
    Sleep(Command),
}

/// Parsed command for a sleep stopwatch run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    duration: Duration,
    logical_second: Duration,
}

impl Command {
    /// Return the requested sleep duration.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use vsleep::{Command, parse_command};
    ///
    /// let action = parse_command(["vsleep", "2"])?;
    /// let vsleep::CommandAction::Sleep(command) = action else {
    ///     panic!("expected sleep command");
    /// };
    /// assert_eq!(command.duration(), Duration::from_secs(2));
    /// # Ok::<(), vsleep::CliError>(())
    /// ```
    #[must_use]
    pub const fn duration(&self) -> Duration { self.duration }

    /// Return the real duration of one logical second.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use vsleep::{CommandAction, parse_command};
    ///
    /// let action = parse_command(["vsleep", "--logical-second-ms", "10", "1"])?;
    /// let CommandAction::Sleep(command) = action else {
    ///     panic!("expected sleep command");
    /// };
    /// assert_eq!(command.logical_second(), Duration::from_millis(10));
    /// # Ok::<(), vsleep::CliError>(())
    /// ```
    #[must_use]
    pub const fn logical_second(&self) -> Duration { self.logical_second }
}

/// Error returned for invalid command-line usage.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CliError {
    /// An argument was not valid UTF-8.
    #[error("invalid non-Unicode argument")]
    NonUnicodeArgument,
    /// A recognised option was missing its required value.
    #[error("option '{option}' requires an argument")]
    MissingOptionValue {
        /// Option that required an additional value.
        option: &'static str,
    },
    /// The hidden logical-second duration was invalid.
    #[error("invalid logical second duration '{value}'")]
    InvalidLogicalSecond {
        /// Invalid option value.
        value: String,
    },
    /// The user supplied an unsupported option.
    #[error("unrecognized option '{option}'")]
    UnknownOption {
        /// Unsupported option text.
        option: String,
    },
    /// Sleep operands could not be parsed.
    #[error(transparent)]
    Duration(#[from] DurationParseError),
}

/// Parse command-line arguments into a command action.
///
/// The first argument is treated as the executable name and ignored.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use vsleep::{CommandAction, parse_command};
///
/// let action = parse_command(["vsleep", "1m", "5s"])?;
/// let CommandAction::Sleep(command) = action else {
///     panic!("expected sleep command");
/// };
/// assert_eq!(command.duration(), Duration::from_secs(65));
/// # Ok::<(), vsleep::CliError>(())
/// ```
///
/// # Errors
///
/// Returns an error when an argument is not Unicode, when an option is unknown
/// or missing a value, when the hidden logical-second value is invalid, or when
/// the sleep operands cannot be parsed.
pub fn parse_command<I, S>(args: I) -> Result<CommandAction, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut logical_second = Duration::from_secs(1);
    let mut operands = Vec::new();
    let mut iter = args.into_iter().map(Into::into);
    let _program = iter.next();

    while let Some(argument) = iter.next() {
        let text = os_string_to_string(argument)?;
        match text.as_str() {
            "--help" => return Ok(CommandAction::Help),
            "--version" => return Ok(CommandAction::Version),
            "--logical-second-ms" => {
                logical_second = parse_logical_second(next_option_value(&mut iter)?)?;
            }
            "--" => {
                collect_operands(&mut operands, iter)?;
                break;
            }
            option if option.starts_with("--") => {
                return Err(CliError::UnknownOption {
                    option: option.to_owned(),
                });
            }
            _ => operands.push(text),
        }
    }

    Ok(CommandAction::Sleep(Command {
        duration: parse_sleep_duration(&operands)?,
        logical_second,
    }))
}

fn collect_operands<I>(operands: &mut Vec<String>, iter: I) -> Result<(), CliError>
where
    I: IntoIterator<Item = OsString>,
{
    for argument in iter {
        operands.push(os_string_to_string(argument)?);
    }
    Ok(())
}

fn next_option_value<I>(iter: &mut I) -> Result<String, CliError>
where
    I: Iterator<Item = OsString>,
{
    iter.next().map_or_else(
        || {
            Err(CliError::MissingOptionValue {
                option: "--logical-second-ms",
            })
        },
        os_string_to_string,
    )
}

fn os_string_to_string(argument: OsString) -> Result<String, CliError> {
    argument
        .into_string()
        .map_err(|_| CliError::NonUnicodeArgument)
}

fn parse_logical_second(value: String) -> Result<Duration, CliError> {
    match value.parse::<u64>() {
        Ok(0) | Err(_) => Err(CliError::InvalidLogicalSecond { value }),
        Ok(milliseconds) => Ok(Duration::from_millis(milliseconds)),
    }
}
