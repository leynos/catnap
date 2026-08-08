//! Pins `DurationParseError` as non-exhaustive for downstream crates.

use catnap::DurationParseError;

/// Matches every currently public variant without a wildcard arm.
///
/// This must not compile. `DurationParseError` is `#[non_exhaustive]`, so
/// downstream crates are required to keep a wildcard arm and adding a variant
/// stays a non-breaking change.
fn classify(error: &DurationParseError) -> &'static str {
    match error {
        DurationParseError::MissingOperand => "missing-operand",
        DurationParseError::EmptyOperand { .. } => "empty-operand",
        DurationParseError::InvalidSuffix { .. } => "invalid-suffix",
        DurationParseError::InvalidNumber { .. } => "invalid-number",
        DurationParseError::TooPrecise { .. } => "too-precise",
        DurationParseError::Overflow { .. } => "overflow",
    }
}

fn main() {
    println!("{}", classify(&DurationParseError::MissingOperand));
}
