//! Pins `ClockConfigError` as non-exhaustive for downstream crates.

use catnap::ClockConfigError;

/// Matches every currently public variant without a wildcard arm.
///
/// This must not compile. `ClockConfigError` is `#[non_exhaustive]`, so
/// downstream crates are required to keep a wildcard arm and adding a variant
/// stays a non-breaking change.
fn classify(error: &ClockConfigError) -> &'static str {
    match error {
        ClockConfigError::ZeroLogicalSecond => "zero-logical-second",
    }
}

fn main() {
    println!("{}", classify(&ClockConfigError::ZeroLogicalSecond));
}
