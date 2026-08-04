//! Duration parsing and progress cadence selection.

use std::time::Duration;

const NANOS_PER_SECOND: u128 = 1_000_000_000;
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
    #[error(
        "invalid time interval '{operand}'\ncatnap: durations are separate operands; did you mean \
         'catnap {suggestion}'?"
    )]
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

fn parse_simple_operand(text: &str, operand: &str) -> Result<u128, DurationParseError> {
    let (number, unit) = split_number_and_unit(text, operand)?;
    parse_decimal_nanos(number, unit, operand)
}

fn checked_add_nanos(total: u128, nanos: u128, operand: &str) -> Result<u128, DurationParseError> {
    total
        .checked_add(nanos)
        .ok_or_else(|| DurationParseError::Overflow {
            operand: operand.to_owned(),
        })
}

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

fn parse_decimal_nanos(
    number: &str,
    unit: u128,
    operand: &str,
) -> Result<u128, DurationParseError> {
    let (whole_text, fraction_text) = decimal_parts(number);
    let whole = parse_digit_text(whole_text, operand)?;
    let fraction = parse_fraction_nanos(fraction_text, unit, operand)?;
    let whole_nanos = whole
        .checked_mul(unit)
        .ok_or_else(|| DurationParseError::Overflow {
            operand: operand.to_owned(),
        })?;

    if whole_text.is_empty() && fraction_text.unwrap_or_default().is_empty() {
        Err(DurationParseError::InvalidNumber {
            operand: operand.to_owned(),
        })
    } else {
        whole_nanos
            .checked_add(fraction)
            .ok_or_else(|| DurationParseError::Overflow {
                operand: operand.to_owned(),
            })
    }
}

fn decimal_parts(number: &str) -> (&str, Option<&str>) {
    match number.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (number, None),
    }
}

fn parse_digit_text(text: &str, operand: &str) -> Result<u128, DurationParseError> {
    let mut value = 0_u128;
    for character in text.chars() {
        let digit = character
            .to_digit(10)
            .ok_or_else(|| DurationParseError::InvalidNumber {
                operand: operand.to_owned(),
            })?;
        value = value
            .checked_mul(10)
            .and_then(|current| current.checked_add(u128::from(digit)))
            .ok_or_else(|| DurationParseError::Overflow {
                operand: operand.to_owned(),
            })?;
    }
    Ok(value)
}

fn parse_fraction_nanos(
    fraction: Option<&str>,
    unit: u128,
    operand: &str,
) -> Result<u128, DurationParseError> {
    fraction.map_or(Ok(0), |text| fraction_to_nanos(text, unit, operand))
}

fn fraction_to_nanos(text: &str, unit: u128, operand: &str) -> Result<u128, DurationParseError> {
    let digits = parse_digit_text(text, operand)?;
    let scale = fraction_scale(text, operand)?;
    digits
        .checked_mul(unit)
        .and_then(|nanos| nanos.checked_div(scale))
        .ok_or_else(|| DurationParseError::Overflow {
            operand: operand.to_owned(),
        })
}

fn fraction_scale(text: &str, operand: &str) -> Result<u128, DurationParseError> {
    let mut scale = 1_u128;
    for _ in text.chars() {
        scale = scale
            .checked_mul(10)
            .ok_or_else(|| DurationParseError::TooPrecise {
                operand: operand.to_owned(),
            })?;
        if scale > NANOS_PER_SECOND {
            return Err(DurationParseError::TooPrecise {
                operand: operand.to_owned(),
            });
        }
    }
    Ok(scale)
}

fn duration_from_total_nanos(nanos: u128, operand: String) -> Result<Duration, DurationParseError> {
    u64::try_from(nanos)
        .map(Duration::from_nanos)
        .map_err(|_| DurationParseError::Overflow { operand })
}

#[cfg(test)]
#[path = "duration_tests.rs"]
mod tests;
