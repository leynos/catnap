//! Pins the user-facing display output of `CliError`.

use catnap::{CliError, DurationParseError};

fn assert_display(error: CliError, expected: &str) {
    assert_eq!(error.to_string(), expected);
}

fn main() {
    assert_display(CliError::NonUnicodeArgument, "invalid non-Unicode argument");
    assert_display(
        CliError::MissingOptionValue { option: "--colour" },
        "option '--colour' requires an argument",
    );
    assert_display(
        CliError::InvalidLogicalSecond {
            value: "0".to_owned(),
        },
        "invalid logical second duration '0'",
    );
    assert_display(
        CliError::UnknownOption {
            option: "--unknown".to_owned(),
        },
        "unrecognized option '--unknown'",
    );
    assert_display(
        CliError::Duration(DurationParseError::MissingOperand),
        "missing operand",
    );
}
