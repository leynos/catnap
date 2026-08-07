//! Decimal number handling for duration operands.
//!
//! This module converts the numeric portion of an operand into nanoseconds.
//! It is deliberately unaware of suffix spelling: callers in
//! [`crate::duration`] resolve a suffix to its nanosecond multiplier and pass
//! that multiplier in as `unit`.

use std::time::Duration;

use crate::duration::DurationParseError;

/// Nanoseconds in one second, and the finest precision an operand may express.
pub(crate) const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Convert decimal `number` text scaled by `unit` into nanoseconds.
///
/// Accepts whole (`5`), fractional (`1.5`), leading-decimal (`.5`) and
/// trailing-decimal (`5.`) forms; `operand` supplies the complete operand text
/// quoted in errors.
///
/// # Errors
///
/// Returns [`DurationParseError::InvalidNumber`] for non-numeric text or a
/// bare `.`, [`DurationParseError::TooPrecise`] for a fraction finer than one
/// nanosecond, and [`DurationParseError::Overflow`] when scaling exceeds
/// [`u128`].
pub(crate) fn parse_decimal_nanos(
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

/// Split a decimal `number` into its whole part and optional fractional part.
///
/// `"1.5"` yields `("1", Some("5"))`, whereas `"1"` yields `("1", None)`.
fn decimal_parts(number: &str) -> (&str, Option<&str>) {
    match number.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (number, None),
    }
}

/// Accumulate the ASCII decimal digits of `text` into a [`u128`].
///
/// Empty `text` yields zero, which lets the caller accept the `.5` and `5.`
/// forms without special-casing them.
///
/// # Errors
///
/// Returns [`DurationParseError::InvalidNumber`] for a non-digit character and
/// [`DurationParseError::Overflow`] when the value exceeds [`u128`].
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

/// Convert an optional fractional part into nanoseconds, reading `None` as zero.
///
/// Delegates to [`fraction_to_nanos`] and propagates its errors unchanged.
///
/// # Errors
///
/// Returns the [`DurationParseError`] produced by [`fraction_to_nanos`].
fn parse_fraction_nanos(
    fraction: Option<&str>,
    unit: u128,
    operand: &str,
) -> Result<u128, DurationParseError> {
    fraction.map_or(Ok(0), |text| fraction_to_nanos(text, unit, operand))
}

/// Scale the fractional digits in `text` by `unit`, truncating towards zero.
///
/// # Errors
///
/// Returns [`DurationParseError::InvalidNumber`] for a non-digit character,
/// [`DurationParseError::TooPrecise`] when the fraction is finer than one
/// nanosecond, and [`DurationParseError::Overflow`] when scaling overflows.
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

/// Compute the power-of-ten divisor matching the digit count of `text`.
///
/// # Errors
///
/// Returns [`DurationParseError::TooPrecise`] once the divisor would exceed
/// [`NANOS_PER_SECOND`], because such a fraction cannot be stored exactly.
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

/// Convert a nanosecond total into a [`Duration`], quoting `operand` on failure.
///
/// # Errors
///
/// Returns [`DurationParseError::Overflow`] when `nanos` exceeds [`u64`], the
/// widest nanosecond count [`Duration::from_nanos`] accepts.
pub(crate) fn duration_from_total_nanos(
    nanos: u128,
    operand: String,
) -> Result<Duration, DurationParseError> {
    u64::try_from(nanos)
        .map(Duration::from_nanos)
        .map_err(|_| DurationParseError::Overflow { operand })
}
