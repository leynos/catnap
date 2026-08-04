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
fn rejects_invalid_operands(#[case] operands: Vec<String>, #[case] expected: DurationParseError) {
    assert_eq!(parse_sleep_duration(&operands), Err(expected));
}

#[rstest]
#[case::hours_and_minutes("5h20m", "1h 5h 20m 10s")]
#[case::hours_and_seconds("1h30s", "1h 1h 30s 10s")]
#[case::minutes_and_bare_seconds("1m30", "1h 1m 30 10s")]
fn suggests_full_command_for_compound_operand(#[case] operand: &str, #[case] suggestion: &str) {
    let operands = ["1h".to_owned(), operand.to_owned(), "10s".to_owned()];
    assert_eq!(
        parse_sleep_duration(&operands),
        Err(DurationParseError::CompoundOperand {
            operand: operand.to_owned(),
            suggestion: suggestion.to_owned()
        })
    );
}

#[test]
fn rejects_compound_rewrite_when_aggregate_overflows() {
    assert_eq!(
        parse_sleep_duration(&["200000d200000d".to_owned()]),
        Err(DurationParseError::Overflow {
            operand: "total".to_owned()
        })
    );
}

#[rstest]
#[case::twenty(Duration::from_secs(20), Duration::from_secs(1))]
#[case::twenty_one(Duration::from_secs(21), Duration::from_secs(5))]
#[case::sixty(Duration::from_mins(1), Duration::from_secs(5))]
#[case::sixty_one(Duration::from_secs(61), Duration::from_secs(30))]
fn selects_progress_interval(#[case] total: Duration, #[case] expected: Duration) {
    assert_eq!(report_interval(total), expected);
}
