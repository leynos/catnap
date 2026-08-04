//! Pins the user-facing display output of `ClockConfigError`.

use catnap::ClockConfigError;

fn main() {
    assert_eq!(
        ClockConfigError::ZeroLogicalSecond.to_string(),
        "logical second duration must be greater than zero",
    );
}
