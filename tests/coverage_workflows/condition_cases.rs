//! Drives the publisher's condition splitter against fixtures directly.
//!
//! Split from `publisher_cases.rs` under the repository's 400-line cap. The
//! ref-guard clause is only as good as the split beneath it, so the quoting
//! and byte-offset rules it depends on are pinned here on their own.

use rstest::rstest;

use super::publisher_rules as rules;

/// Scenario: conditions whose operators sit inside quoted literals.
///
/// Invariant: a `||` inside a string is not a disjunction, and an `&&`
/// inside one does not split a conjunct.
#[rstest]
#[case::quoted_or("${{ github.ref == 'refs/heads/main' && env.X != 'a||b' }}", Some(2))]
#[case::quoted_and("${{ github.ref == 'refs/heads/main' && env.X != 'a&&b' }}", Some(2))]
#[case::bare_or("github.ref == 'refs/heads/main' || true", None)]
fn quoted_operators_are_not_operators(#[case] condition: &str, #[case] expected: Option<usize>) {
    assert_eq!(
        rules::conjuncts(condition).map(|parts| parts.len()),
        expected,
        "for {condition}"
    );
}

/// Scenario: a quoted literal holds multi-byte characters before an operator.
///
/// Invariant: each conjunct is cut at its own operator. The blanked copy
/// that locates operators must keep the input's byte offsets, or the cut
/// lands inside a character and the ref conjunct reads as something else.
#[test]
fn a_multibyte_literal_does_not_shift_the_split() {
    assert_eq!(
        rules::conjuncts("${{ env.X != 'é&&ü' && github.ref == 'refs/heads/main' }}"),
        Some(vec![
            "env.X != 'é&&ü'".to_owned(),
            "github.ref == 'refs/heads/main'".to_owned(),
        ]),
    );
}
