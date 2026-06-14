//! Duration parsing and progress cadence selection.

use std::time::Duration;

const NANOS_PER_SECOND: u128 = 1_000_000_000;
const NANOS_PER_MINUTE: u128 = NANOS_PER_SECOND * 60;
const NANOS_PER_HOUR: u128 = NANOS_PER_MINUTE * 60;
const NANOS_PER_DAY: u128 = NANOS_PER_HOUR * 24;

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

    let mut total = 0_u128;
    for operand in operands {
        let nanos = parse_operand_nanos(operand)?;
        total = total
            .checked_add(nanos)
            .ok_or_else(|| DurationParseError::Overflow {
                operand: operand.clone(),
            })?;
    }
    duration_from_total_nanos(total, "total".to_owned())
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

fn parse_operand_nanos(operand: &str) -> Result<u128, DurationParseError> {
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

    let (number, unit) = split_number_and_unit(operand)?;
    parse_decimal_nanos(number, unit, operand)
}

fn split_number_and_unit(operand: &str) -> Result<(&str, u128), DurationParseError> {
    for (suffix, unit) in [
        ('s', NANOS_PER_SECOND),
        ('m', NANOS_PER_MINUTE),
        ('h', NANOS_PER_HOUR),
        ('d', NANOS_PER_DAY),
    ] {
        if let Some(number) = operand.strip_suffix(suffix) {
            return Ok((number, unit));
        }
    }

    match operand.chars().next_back() {
        Some(character) if character.is_ascii_alphabetic() => {
            Err(DurationParseError::InvalidSuffix {
                operand: operand.to_owned(),
            })
        }
        Some(_) => Ok((operand, NANOS_PER_SECOND)),
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
mod tests {
    //! Unit tests for sleep duration parsing and cadence selection.

    use std::time::Duration;

    use rstest::rstest;

    use super::{DurationParseError, parse_sleep_duration, report_interval};

    #[rstest]
    #[case::seconds("2", Duration::from_secs(2))]
    #[case::explicit_seconds("2s", Duration::from_secs(2))]
    #[case::minutes("2m", Duration::from_mins(2))]
    #[case::hours("2h", Duration::from_hours(2))]
    #[case::days("2d", Duration::from_hours(48))]
    #[case::fractional_seconds("1.5", Duration::from_millis(1_500))]
    fn parses_supported_sleep_operands(#[case] operand: &str, #[case] expected: Duration) {
        assert_eq!(parse_sleep_duration(&[operand.to_owned()]), Ok(expected));
    }

    #[test]
    fn sums_multiple_operands() {
        assert_eq!(
            parse_sleep_duration(&["1m".to_owned(), "5s".to_owned()]),
            Ok(Duration::from_secs(65))
        );
    }

    #[rstest]
    #[case::missing(Vec::<String>::new(), DurationParseError::MissingOperand)]
    #[case::negative(
        vec!["-1".to_owned()],
        DurationParseError::InvalidNumber {
            operand: "-1".to_owned()
        }
    )]
    #[case::bad_suffix(
        vec!["1w".to_owned()],
        DurationParseError::InvalidSuffix {
            operand: "1w".to_owned()
        }
    )]
    #[case::too_precise(
        vec!["0.0000000001".to_owned()],
        DurationParseError::TooPrecise {
            operand: "0.0000000001".to_owned()
        }
    )]
    fn rejects_invalid_operands(
        #[case] operands: Vec<String>,
        #[case] expected: DurationParseError,
    ) {
        assert_eq!(parse_sleep_duration(&operands), Err(expected));
    }

    #[rstest]
    #[case::twenty(Duration::from_secs(20), Duration::from_secs(1))]
    #[case::twenty_one(Duration::from_secs(21), Duration::from_secs(5))]
    #[case::sixty(Duration::from_mins(1), Duration::from_secs(5))]
    #[case::sixty_one(Duration::from_secs(61), Duration::from_secs(30))]
    fn selects_progress_interval(#[case] total: Duration, #[case] expected: Duration) {
        assert_eq!(report_interval(total), expected);
    }
}
