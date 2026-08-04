//! Pins the user-facing display output of `DurationParseError`.

use catnap::DurationParseError;

fn assert_display(error: DurationParseError, expected: &str) {
    assert_eq!(error.to_string(), expected);
}

fn main() {
    assert_display(DurationParseError::MissingOperand, "missing operand");
    assert_display(
        DurationParseError::EmptyOperand {
            operand: String::new(),
        },
        "invalid time interval ''",
    );
    assert_display(
        DurationParseError::InvalidSuffix {
            operand: "2fortnights".to_owned(),
        },
        "invalid time suffix in '2fortnights'",
    );
    assert_display(
        DurationParseError::InvalidNumber {
            operand: "soon".to_owned(),
        },
        "invalid time interval 'soon'",
    );
    assert_display(
        DurationParseError::TooPrecise {
            operand: "0.0000000001s".to_owned(),
        },
        "time interval '0.0000000001s' has more than nanosecond precision",
    );
    assert_display(
        DurationParseError::Overflow {
            operand: "340282366920938463464s".to_owned(),
        },
        "time interval '340282366920938463464s' is too large",
    );
}
