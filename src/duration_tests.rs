//! Unit tests for sleep duration parsing and cadence selection.

use std::time::Duration;

use proptest::prelude::*;
use rstest::rstest;

use super::{DurationParseError, UNITS, parse_sleep_duration, report_interval};

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

#[rstest]
#[case::seconds("s")]
#[case::minutes("m")]
#[case::hours("h")]
#[case::days("d")]
fn suggests_separate_operands_for_every_supported_unit(#[case] suffix: &str) {
    let operand = format!("2{suffix}3{suffix}");
    let parsed = parse_sleep_duration(std::slice::from_ref(&operand));
    assert_eq!(
        parsed,
        Err(DurationParseError::CompoundOperand {
            operand,
            suggestion: format!("2{suffix} 3{suffix}"),
        })
    );
}

/// Guard the single-character assumption baked into the compound strategy.
///
/// [`compound_components`] concatenates suffixes without disambiguation, which
/// is safe only while every suffix is one ASCII letter. Adding a
/// multi-character suffix to `UNITS` must fail here so that longest-match
/// coverage is added deliberately rather than assumed.
#[test]
fn unit_suffixes_are_single_ascii_letters() {
    for (suffix, _) in UNITS {
        assert_eq!(suffix.len(), 1, "suffix '{suffix}' is not one byte long");
        assert!(
            suffix
                .chars()
                .all(|character| character.is_ascii_alphabetic()),
            "suffix '{suffix}' is not an ASCII letter"
        );
    }
}

/// Generate a run of one to nine fractional digits.
///
/// The parser rejects anything finer than nanosecond precision, so the run is
/// capped at the nine digits a fraction of a second can hold.
fn fraction_digits() -> impl Strategy<Value = String> {
    proptest::collection::vec(0_u8..10, 1..=9).prop_map(|digits| {
        digits
            .into_iter()
            .map(|digit| char::from(b'0' + digit))
            .collect()
    })
}

/// Generate every number form the duration parser accepts.
///
/// Whole values stay below one thousand so that four components of days still
/// sum well inside the supported duration range.
fn number_text() -> impl Strategy<Value = String> {
    prop_oneof![
        (0_u32..1_000).prop_map(|whole| whole.to_string()),
        (0_u32..1_000, fraction_digits())
            .prop_map(|(whole, fraction)| format!("{whole}.{fraction}")),
        fraction_digits().prop_map(|fraction| format!(".{fraction}")),
        (0_u32..1_000).prop_map(|whole| format!("{whole}.")),
    ]
}

/// Generate a suffix drawn from the shared [`UNITS`] metadata.
fn unit_suffix() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(UNITS.map(|(suffix, _)| suffix).to_vec())
}

/// Generate the components of a compound operand, in order.
///
/// Only the final component may omit its suffix: a bare number anywhere else
/// would run into the following component's digits and form a single larger
/// number rather than two operands.
fn compound_components() -> impl Strategy<Value = Vec<String>> {
    (
        proptest::collection::vec((number_text(), unit_suffix()), 1..4),
        number_text(),
        proptest::option::of(unit_suffix()),
    )
        .prop_map(|(leading, final_number, final_suffix)| {
            let mut components = leading
                .into_iter()
                .map(|(number, suffix)| format!("{number}{suffix}"))
                .collect::<Vec<_>>();
            components.push(format!(
                "{final_number}{}",
                final_suffix.unwrap_or_default()
            ));
            components
        })
}

/// Sum each component parsed on its own, independently of the rewrite.
fn sum_components(components: &[String]) -> Result<Duration, DurationParseError> {
    components.iter().try_fold(Duration::ZERO, |total, part| {
        parse_sleep_duration(std::slice::from_ref(part)).map(|parsed| total + parsed)
    })
}

proptest! {
    /// Every rejected compound operand suggests an equivalent operand list.
    #[test]
    fn compound_operand_suggestion_round_trips(components in compound_components()) {
        let compound = components.concat();
        let Err(DurationParseError::CompoundOperand { operand, suggestion }) =
            parse_sleep_duration(std::slice::from_ref(&compound))
        else {
            return Err(TestCaseError::fail(format!(
                "'{compound}' should be rejected as a compound operand"
            )));
        };
        prop_assert_eq!(&operand, &compound);

        let separated = suggestion
            .split_ascii_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        prop_assert_eq!(&separated, &components);

        let Ok(expected) = sum_components(&components) else {
            return Err(TestCaseError::fail(format!(
                "components of '{compound}' should each parse in isolation"
            )));
        };
        prop_assert_eq!(parse_sleep_duration(&separated), Ok(expected));
    }
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
