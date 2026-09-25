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
    "not exactly"
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
/// coverage step writes and passes the secret itself as its token.
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
      - uses: leynos/shared-actions/.github/actions/upload-codescene-coverage@abc
        with:
          path: lcov.info
          format: lcov
          access-token: ${{ secrets.CS_ACCESS_TOKEN }}
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
    "          access-token: ${{ secrets.CS_ACCESS_TOKEN }}\n",
    "",
    Some("not the secret itself")
)]
#[case::other_token(
    "${{ secrets.CS_ACCESS_TOKEN }}",
    "${{ secrets.OTHER }}",
    Some("not the secret itself")
)]
#[case::through_the_env(
    "${{ secrets.CS_ACCESS_TOKEN }}",
    "${{ env.CS_ACCESS_TOKEN }}",
    Some("not the secret itself")
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

/// Scenario: the token check step is broken one way at a time, or the upload
/// stops reading its answer.
///
/// Invariant: each is named. A deleted check leaves the upload skipping
/// forever, a check behind an `if:` can be skipped, one with no id cannot be
/// read, a changed command answers something else, and an upload guarded on
/// the environment or on another step reads no check.
#[rstest]
#[case::check_behind_an_if(
    "      - id: codescene-token\n",
    "      - id: codescene-token\n        if: always()\n",
    "carries an `if:`"
)]
#[case::check_without_an_id(
    "      - id: codescene-token\n        run:",
    "      - run:",
    "has no id"
)]
#[case::check_behind_a_shell(
    "      - id: codescene-token\n",
    "      - id: codescene-token\n        shell: bash -c 'exit 0; {0}'\n",
    "declares `shell`"
)]
#[case::check_allowed_to_fail(
    "      - id: codescene-token\n",
    "      - id: codescene-token\n        continue-on-error: true\n",
    "declares `continue-on-error`"
)]
#[case::check_command_changed(
    "secrets.CS_ACCESS_TOKEN != ''",
    "true",
    "exactly one token check step"
)]
#[case::check_deleted(
    "      - id: codescene-token\n        run: echo \"available=${{ secrets.CS_ACCESS_TOKEN != '' \
     }}\" >> \"$GITHUB_OUTPUT\"\n",
    "",
    "exactly one token check step"
)]
#[case::guard_on_the_environment(
    "steps.codescene-token.outputs.available == 'true'",
    "env.CS_ACCESS_TOKEN != ''",
    "not guarded"
)]
#[case::guard_on_another_step(
    "steps.codescene-token.outputs.available == 'true'",
    "steps.other.outputs.available == 'true'",
    "not guarded"
)]
fn the_token_check_is_exact(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: &str,
) -> Result<()> {
    let complying = publisher(NEVER_CANCEL, GUARD, "");
    let source = complying.replacen(from, to, 1);
    ensure!(source != complying, "the case changed nothing");
    let findings = rules::publisher_findings(&parse(&source)?);
    ensure!(
        findings.iter().any(|f| f.contains(expected)),
        "expected a finding naming {expected:?}, saw {findings:?}"
    );
    Ok(())
}

/// The token check step as the publisher fixture writes it.
const TOKEN_CHECK: &str = "      - id: codescene-token\n        run: echo \"available=${{ \
                           secrets.CS_ACCESS_TOKEN != '' }}\" >> \"$GITHUB_OUTPUT\"\n";
/// A shell that runs nothing, so a step under it never writes its output.
const SILENT_SHELL: &str = "shell: bash -c 'exit 0; {0}'";

/// Scenario: the token check is moved after the upload or into another job,
/// or put under a default shell, on the workflow, its job, or another job.
///
/// Invariant: each placement the upload cannot read is named, and nothing
/// else. A `steps.<id>` output resolves only later in the same job, and a
/// default shell wraps the check as a step `shell` would; a default on a job
/// the check is not in reaches nothing, so it is not refused.
#[rstest]
#[case::after_the_upload(
    |source: String| format!("{}{TOKEN_CHECK}", source.replacen(TOKEN_CHECK, "", 1)),
    Some("before every upload")
)]
#[case::in_another_job(
    |source: String| format!("{}  token:\n    steps:\n{TOKEN_CHECK}", source.replacen(TOKEN_CHECK, "", 1)),
    Some("before every upload")
)]
#[case::workflow_default_shell(
    |source: String| source.replacen("jobs:\n", &format!("defaults:\n  run:\n    {SILENT_SHELL}\njobs:\n"), 1),
    Some("defaults.run.shell")
)]
#[case::job_default_shell(
    |source: String| source.replacen("  coverage:\n", &format!("  coverage:\n    defaults:\n      run:\n        {SILENT_SHELL}\n"), 1),
    Some("defaults.run.shell")
)]
#[case::other_job_default_shell(
    |source: String| format!("{source}  other:\n    defaults:\n      run:\n        {SILENT_SHELL}\n    steps:\n      - run: 'true'\n"),
    None
)]
fn the_token_check_is_where_the_upload_reads_it(
    #[case] edit: fn(String) -> String,
    #[case] expected: Option<&str>,
) -> Result<()> {
    let complying = publisher(NEVER_CANCEL, GUARD, "");
    let source = edit(complying.clone());
    ensure!(source != complying, "the case changed nothing");
    let findings = rules::publisher_findings(&parse(&source)?);
    match expected {
        None => ensure!(findings.is_empty(), "unexpected findings: {findings:?}"),
        Some(clause) => ensure!(
            findings.len() == 1 && findings.iter().all(|f| f.contains(clause)),
            "expected one finding naming {clause:?}, saw {findings:?}"
        ),
    }
    Ok(())
}
