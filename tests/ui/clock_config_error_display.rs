//! Pins the user-facing display output of `ClockConfigError`.

use catnap::ClockConfigError;

/// Checks every public `ClockConfigError` variant against its pinned message.
fn main() {
    assert_eq!(
        ClockConfigError::ZeroLogicalSecond.to_string(),
        "logical second duration must be greater than zero",
    );
}
