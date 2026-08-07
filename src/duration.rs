//! Duration parsing and progress cadence selection.

use std::time::Duration;

use crate::duration_number::{NANOS_PER_SECOND, duration_from_total_nanos, parse_decimal_nanos};

const NANOS_PER_MINUTE: u128 = NANOS_PER_SECOND * 60;
const NANOS_PER_HOUR: u128 = NANOS_PER_MINUTE * 60;
const NANOS_PER_DAY: u128 = NANOS_PER_HOUR * 24;
const UNITS: [(&str, u128); 4] = [
    ("s", NANOS_PER_SECOND),
    ("m", NANOS_PER_MINUTE),
    ("h", NANOS_PER_HOUR),
    ("d", NANOS_PER_DAY),
];

enum ParsedOperand {
    Single(u128),
    Compound { nanos: u128, replacement: String },
}

/// Error returned when parsing a GNU-like sleep operand fails.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum DurationParseError {
    /// No sleep operands were supplied.
    #[error("missing operand")]
    MissingOperand,
    /// An operand was empty.
    #[error("invalid time interval '{operand}'")]
    EmptyOperand {
        /// Operand text received from the command line.
        operand: String,
    },
    /// An operand used a suffix that `catnap` does not support.
    #[error("invalid time suffix in '{operand}'")]
    InvalidSuffix {
        /// Operand text received from the command line.
        operand: String,
    },
    /// An operand did not contain a valid non-negative decimal number.
    #[error("invalid time interval '{operand}'")]
    InvalidNumber {
        /// Operand text received from the command line.
        operand: String,
    },
    /// An operand concatenated otherwise valid duration operands.
    ///
    /// The display text stays in domain language; presenting `suggestion` as
    /// advice is the command-line layer's responsibility.
    #[error("invalid time interval '{operand}'")]
    CompoundOperand {
        /// Operand text received from the command line.
        operand: String,
        /// Complete equivalent operand list, separated by spaces.
        suggestion: String,
    },
    /// An operand was too precise for nanosecond storage.
    #[error("time interval '{operand}' has more than nanosecond precision")]
    TooPrecise {
        /// Operand text received from the command line.
        operand: String,
    },
    /// The summed duration is larger than `std::time::Duration` can hold.
    #[error("time interval '{operand}' is too large")]
    Overflow {
        /// Operand text received from the command line.
        operand: String,
    },
}

/// Parse one or more GNU-like sleep operands into a total duration.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use catnap::parse_sleep_duration;
///
/// let duration = parse_sleep_duration(&["1m".to_owned(), "5s".to_owned()])?;
/// assert_eq!(duration, Duration::from_secs(65));
/// # Ok::<(), catnap::DurationParseError>(())
/// ```
///
/// # Errors
///
/// Returns an error when no operands are provided, an operand has an invalid
/// number or suffix, fractional precision exceeds nanoseconds, or the summed
/// duration overflows [`Duration`].
pub fn parse_sleep_duration(operands: &[String]) -> Result<Duration, DurationParseError> {
    if operands.is_empty() {
        return Err(DurationParseError::MissingOperand);
    }

    let parsed = operands
        .iter()
        .map(|operand| parse_operand(operand))
        .collect::<Result<Vec<_>, _>>()?;
    let total = parsed.iter().try_fold(0_u128, |total, operand| {
        let nanos = match operand {
            ParsedOperand::Single(nanos) | ParsedOperand::Compound { nanos, .. } => *nanos,
        };
        total
            .checked_add(nanos)
            .ok_or_else(|| DurationParseError::Overflow {
                operand: "total".to_owned(),
            })
    })?;
    let duration = duration_from_total_nanos(total, "total".to_owned())?;

    if let Some((compound_operand, _)) = operands
        .iter()
        .zip(&parsed)
        .find(|(_, parsed_operand)| matches!(parsed_operand, ParsedOperand::Compound { .. }))
    {
        let compound_text = compound_operand.clone();
        let suggestion = operands
            .iter()
            .zip(parsed)
            .map(|(operand, parsed_operand)| match parsed_operand {
                ParsedOperand::Single(_) => operand.clone(),
                ParsedOperand::Compound { replacement, .. } => replacement,
            })
            .collect::<Vec<_>>()
            .join(" ");
        return Err(DurationParseError::CompoundOperand {
            operand: compound_text,
            suggestion,
        });
    }

    Ok(duration)
}

/// Select the progress reporting interval for a requested sleep duration.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use catnap::report_interval;
///
/// assert_eq!(
///     report_interval(Duration::from_secs(20)),
///     Duration::from_secs(1)
/// );
/// assert_eq!(
///     report_interval(Duration::from_secs(60)),
///     Duration::from_secs(5)
/// );
/// assert_eq!(
///     report_interval(Duration::from_secs(61)),
///     Duration::from_secs(30)
/// );
/// ```
#[must_use]
pub fn report_interval(total: Duration) -> Duration {
    if total <= Duration::from_secs(20) {
        Duration::from_secs(1)
    } else if total <= Duration::from_mins(1) {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(30)
    }
}

/// Validate an operand's overall shape, then parse its components.
///
/// GNU `sleep` accepts neither empty nor explicitly signed operands, so those
/// are rejected before component parsing sees them.
///
/// # Errors
///
/// Returns [`DurationParseError::EmptyOperand`] for empty text,
/// [`DurationParseError::InvalidNumber`] for a leading `-` or `+`, and
/// otherwise whichever error [`parse_operand_components`] reports.
fn parse_operand(operand: &str) -> Result<ParsedOperand, DurationParseError> {
    if operand.is_empty() {
        return Err(DurationParseError::EmptyOperand {
            operand: operand.to_owned(),
        });
    }
    if operand.starts_with('-') || operand.starts_with('+') {
        return Err(DurationParseError::InvalidNumber {
            operand: operand.to_owned(),
        });
    }

    parse_operand_components(operand)
}

/// Parse an operand once, returning a rewrite only for multiple valid parts.
///
/// For example, `5h20m` yields a compound replacement of `5h 20m`, while
/// `5h20x` retains its ordinary invalid-suffix error.
fn parse_operand_components(operand: &str) -> Result<ParsedOperand, DurationParseError> {
    let mut parts = Vec::new();
    let mut total = 0_u128;
    let mut remaining = operand;

    while let Some((number, suffix, unit, rest)) = next_unit(remaining) {
        let Ok(nanos) = parse_decimal_nanos(number, unit, operand) else {
            return parse_simple_operand(operand, operand).map(ParsedOperand::Single);
        };
        total = checked_add_nanos(total, nanos, operand)?;
        parts.push(format!("{number}{suffix}"));
        remaining = rest;
    }

    if !remaining.is_empty() {
        let nanos = parse_simple_operand(remaining, operand)?;
        total = checked_add_nanos(total, nanos, operand)?;
        parts.push(remaining.to_owned());
    }

    if parts.len() > 1 {
        Ok(ParsedOperand::Compound {
            nanos: total,
            replacement: parts.join(" "),
        })
    } else {
        Ok(ParsedOperand::Single(total))
    }
}

/// Split the leading `NUMBER SUFFIX` pair from `text`.
///
/// Scans left to right for the earliest position at which a [`UNITS`] suffix
/// begins, preferring the longest suffix that matches there. Yields the number
/// text, the matched suffix, its nanosecond multiplier and the unconsumed
/// remainder, or `None` when `text` holds no suffix at all.
fn next_unit(text: &str) -> Option<(&str, &'static str, u128, &str)> {
    text.char_indices().find_map(|(index, _)| {
        let (number, suffix_and_rest) = text.split_at(index);
        UNITS
            .iter()
            .filter_map(|&(suffix, unit)| {
                suffix_and_rest
                    .strip_prefix(suffix)
                    .map(|rest| (number, suffix, unit, rest))
            })
            .max_by_key(|(_, suffix, ..)| suffix.len())
    })
}

/// Parse a lone `NUMBER[SUFFIX]` fragment into nanoseconds.
///
/// `text` is the fragment under inspection; `operand` is the complete operand
/// quoted in errors, which differ once a compound operand is being rewritten.
///
/// # Errors
///
/// Returns [`DurationParseError::InvalidSuffix`] for an unsupported suffix, or
/// whichever error [`parse_decimal_nanos`] reports for the number.
fn parse_simple_operand(text: &str, operand: &str) -> Result<u128, DurationParseError> {
    let (number, unit) = split_number_and_unit(text, operand)?;
    parse_decimal_nanos(number, unit, operand)
}

/// Add `nanos` to `total`, attributing any overflow to `operand`.
///
/// # Errors
///
/// Returns [`DurationParseError::Overflow`] when the sum exceeds [`u128`].
fn checked_add_nanos(total: u128, nanos: u128, operand: &str) -> Result<u128, DurationParseError> {
    total
        .checked_add(nanos)
        .ok_or_else(|| DurationParseError::Overflow {
            operand: operand.to_owned(),
        })
}

/// Split `text` into its number text and nanosecond multiplier.
///
/// Prefers the longest matching [`UNITS`] suffix and falls back to seconds when
/// `text` ends in a digit or `.`; `operand` supplies the text quoted in errors.
///
/// # Errors
///
/// Returns [`DurationParseError::InvalidSuffix`] when `text` ends in an
/// unrecognized letter, and [`DurationParseError::EmptyOperand`] when `text` is
/// empty.
fn split_number_and_unit<'a>(
    text: &'a str,
    operand: &str,
) -> Result<(&'a str, u128), DurationParseError> {
    if let Some((number, unit)) = UNITS
        .iter()
        .filter_map(|&(suffix, unit)| {
            text.strip_suffix(suffix)
                .map(|number| (number, unit, suffix))
        })
        .max_by_key(|(_, _, suffix)| suffix.len())
        .map(|(number, unit, _)| (number, unit))
    {
        return Ok((number, unit));
    }

    match text.chars().next_back() {
        Some(character) if character.is_ascii_alphabetic() => {
            Err(DurationParseError::InvalidSuffix {
                operand: operand.to_owned(),
            })
        }
        Some(_) => Ok((text, NANOS_PER_SECOND)),
        None => Err(DurationParseError::EmptyOperand {
            operand: operand.to_owned(),
        }),
    }
}

#[cfg(test)]
#[path = "duration_tests.rs"]
mod tests;
