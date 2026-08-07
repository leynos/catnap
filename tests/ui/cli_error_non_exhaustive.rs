//! Pins `CliError` as non-exhaustive for downstream crates.

use catnap::CliError;

/// Matches every currently public variant without a wildcard arm.
///
/// This must not compile. `CliError` is `#[non_exhaustive]`, so downstream
/// crates are required to keep a wildcard arm and adding a variant stays a
/// non-breaking change.
fn classify(error: &CliError) -> &'static str {
    match error {
        CliError::NonUnicodeArgument => "non-unicode-argument",
        CliError::MissingOptionValue { .. } => "missing-option-value",
        CliError::InvalidLogicalSecond { .. } => "invalid-logical-second",
        CliError::UnknownOption { .. } => "unknown-option",
        CliError::Duration(_) => "duration",
    }
}

fn main() {
    println!("{}", classify(&CliError::NonUnicodeArgument));
}
