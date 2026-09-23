//! Drives the publisher's pinned group, triggers and upload wiring.
//!
//! Split from `publisher_cases.rs` under the repository's 400-line cap. Each
//! case varies one of these on an otherwise complying publisher.

use anyhow::{Result, ensure};
use rstest::rstest;

use super::{
    publisher_cases::{GUARD, NEVER_CANCEL, publisher},
    publisher_rules as rules,
    pull_request_cases::parse,
};

/// Scenario: the publisher's job declares a constant group beside a keyed
/// workflow group, or its trigger set gains or loses an event.
///
/// Invariant: each is named. A constant job group serializes the upload
/// across refs and events whatever the workflow group says, and the trigger
/// set is pinned because a lost dispatch or an added schedule changes when
/// the baseline is written without failing any other clause.
#[rstest]
#[case::constant_job_group(
    "  coverage:\n",
    "  coverage:\n    concurrency:\n      group: upload\n",
    "a dispatch can replace a pending push"
)]
#[case::schedule_added(
    "  workflow_dispatch:\n",
    "  workflow_dispatch:\n  schedule:\n    - cron: '0 0 * * *'\n",
    "not exactly"
)]
#[case::dispatch_dropped("  workflow_dispatch:\n", "", "not exactly")]
fn the_publisher_group_and_triggers_are_pinned(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: &str,
) -> Result<()> {
    let complying = publisher(NEVER_CANCEL, GUARD, "");
    let source = complying.replacen(from, to, 1);
    ensure!(source != complying, "the case changed nothing");
    let findings = rules::publisher_findings(&parse(&source)?);
    ensure!(
        findings.len() == 1 && findings.iter().all(|f| f.contains(expected)),
        "expected one finding naming {expected:?}, saw {findings:?}"
    );
    Ok(())
}

/// A publisher wired as this repository wires it: the upload reads what the
/// coverage step writes and passes the token its step was given.
const WIRED: &str = r"
on:
  push:
    branches: [main]
jobs:
  coverage:
    steps:
      - uses: leynos/shared-actions/.github/actions/generate-coverage@abc
        with:
          output-path: lcov.info
          format: lcov
      - env:
          CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}
        uses: leynos/shared-actions/.github/actions/upload-codescene-coverage@abc
        with:
          path: lcov.info
          format: lcov
          access-token: ${{ env.CS_ACCESS_TOKEN }}
";

/// Scenario: the upload is rewired away from what was measured, or from the
/// token.
///
/// Invariant: each variation is named, and the wired publisher has none.
/// Every other publisher clause passes all five variations, because each
/// judges one step at a time.
#[rstest]
#[case::wired("", "", None)]
#[case::other_path(
    "path: lcov.info",
    "path: other.info",
    Some("which no coverage step writes")
)]
#[case::other_format(
    "          format: lcov\n          access",
    "          format: cobertura\n          access",
    Some("which no coverage step writes")
)]
#[case::no_token(
    "          access-token: ${{ env.CS_ACCESS_TOKEN }}\n",
    "",
    Some("not the token its step binds")
)]
#[case::other_token(
    "${{ env.CS_ACCESS_TOKEN }}",
    "${{ env.OTHER }}",
    Some("not the token its step binds")
)]
#[case::secret_directly(
    "${{ env.CS_ACCESS_TOKEN }}",
    "${{ secrets.CS_ACCESS_TOKEN }}",
    Some("not the token its step binds")
)]
fn the_upload_sends_what_was_measured(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: Option<&str>,
) -> Result<()> {
    let source = if from.is_empty() {
        WIRED.to_owned()
    } else {
        WIRED.replacen(from, to, 1)
    };
    ensure!(
        source != WIRED || from.is_empty(),
        "the case changed nothing"
    );
    let findings = rules::wiring_findings(&parse(&source)?);
    match expected {
        None => ensure!(findings.is_empty(), "unexpected findings: {findings:?}"),
        Some(clause) => ensure!(
            findings.len() == 1 && findings.iter().all(|f| f.contains(clause)),
            "expected one finding naming {clause:?}, saw {findings:?}"
        ),
    }
    Ok(())
}
