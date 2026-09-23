//! Drives the main-publisher rules against fixtures directly.
//!
//! Split from `pull_request_cases.rs` under the repository's 400-line cap.
//! Each case varies one clause of an otherwise complying publisher and
//! asserts the rule names that clause and nothing else.

use anyhow::{Result, ensure};
use rstest::rstest;

use super::{publisher_rules as rules, pull_request_cases::parse};

/// Scenario: push triggers that name other branches, tags, or none.
///
/// Invariant: only a push restricted to `main` is the publisher.
#[rstest]
#[case::main_only("on:\n  push:\n    branches: [main]\njobs: {}\n", true)]
#[case::unfiltered("on:\n  push:\njobs: {}\n", false)]
#[case::scalar("on: push\njobs: {}\n", false)]
#[case::tags("on:\n  push:\n    tags: ['v*']\njobs: {}\n", false)]
#[case::another_branch("on:\n  push:\n    branches: [develop]\njobs: {}\n", false)]
#[case::main_and_another("on:\n  push:\n    branches: [main, develop]\njobs: {}\n", false)]
fn only_a_push_restricted_to_main_is_the_publisher(
    #[case] source: &str,
    #[case] expected: bool,
) -> Result<()> {
    ensure!(
        rules::publishes_from_main(&parse(source)?) == expected,
        "expected {expected} for {source:?}"
    );
    Ok(())
}

/// A publisher, with the three pieces the cases vary left as placeholders.
const PUBLISHER: &str = r#"
on:
  push:
    branches: [main]
  workflow_dispatch:
@CONCURRENCY@
jobs:
  coverage:
    steps:
      - uses: leynos/shared-actions/.github/actions/generate-coverage@abc
        with:
          with-ratchet: 'true'
@EXTRA_STEP@
      - id: codescene-token
        run: echo "available=${{ secrets.CS_ACCESS_TOKEN != '' }}" >> "$GITHUB_OUTPUT"
      - if: "@UPLOAD_IF@"
        uses: leynos/shared-actions/.github/actions/upload-codescene-coverage@abc
        with:
          access-token: ${{ secrets.CS_ACCESS_TOKEN }}
"#;

/// Renders [`PUBLISHER`] with the pieces a case varies.
pub(super) fn publisher(concurrency: &str, upload_if: &str, extra_step: &str) -> String {
    PUBLISHER
        .replace("@CONCURRENCY@", concurrency)
        .replace("@UPLOAD_IF@", upload_if)
        .replace("@EXTRA_STEP@", extra_step)
}

/// The publisher's concurrency block as this repository writes it.
pub(super) const NEVER_CANCEL: &str = "concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: false";
/// The upload condition as this repository writes it.
pub(super) const GUARD: &str =
    "${{ steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main' }}";

/// Scenario: a publisher is varied one clause at a time.
///
/// Invariant: each variation is reported by the clause it breaks, and the
/// unvaried publisher has no findings. The upload guard must be exactly the
/// token and ref conjuncts, so every disjunction case fails the set
/// comparison whether or not `||` is refused: the refusal is defence in
/// depth here, proved on the splitter's own cases. An extra conjunct fails
/// too, including the ones that silently stop the upload (`&& false`, a
/// second ref) and a negated group that carries the ref conjunct while
/// uploading everywhere but `main`.
///
/// The binding cases delete or move the token's `env` binding. The guard
/// `steps.codescene-token.outputs.available == 'true'` reads a missing binding as empty and skips
/// the upload forever, so the binding is asserted rather than inferred.
#[rstest]
#[case::complies(NEVER_CANCEL, GUARD, "", None)]
#[case::disjunction_appended(
    NEVER_CANCEL,
    "${{ steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main' || \
     github.event_name == 'workflow_dispatch' }}",
    "",
    Some("not guarded")
)]
#[case::disjunction_prepended(
    NEVER_CANCEL,
    "${{ github.event_name == 'workflow_dispatch' || steps.codescene-token.outputs.available == \
     'true' && github.ref == 'refs/heads/main' }}",
    "",
    Some("not guarded")
)]
#[case::disjunction_in_extra_conjunct(
    NEVER_CANCEL,
    "${{ steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main' && \
     github.actor != 'x' || github.event_name == 'workflow_dispatch' }}",
    "",
    Some("not guarded")
)]
#[case::narrowing_conjunct(
    NEVER_CANCEL,
    "${{ steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main' && \
     github.actor != 'x' }}",
    "",
    Some("not guarded")
)]
#[case::never_true(
    NEVER_CANCEL,
    "${{ steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main' && \
     false }}",
    "",
    Some("not guarded")
)]
#[case::second_ref(
    NEVER_CANCEL,
    "${{ steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main' && \
     github.ref == 'refs/heads/develop' }}",
    "",
    Some("not guarded")
)]
#[case::negated_group(
    NEVER_CANCEL,
    "${{ steps.codescene-token.outputs.available == 'true' && !(github.actor == 'x' && github.ref \
     == 'refs/heads/main' && true) }}",
    "",
    Some("not guarded")
)]
#[case::no_ref_guard(
    NEVER_CANCEL,
    "${{ steps.codescene-token.outputs.available == 'true' }}",
    "",
    Some("not guarded")
)]
#[case::cancels(
    "concurrency:\n  group: ${{ github.workflow }}-${{ github.ref }}\n  cancel-in-progress: true",
    GUARD,
    "",
    Some("cancel")
)]
#[case::cancels_by_expression(
    "concurrency:\n  group: ${{ github.workflow }}-${{ github.ref }}\n  cancel-in-progress: ${{ \
     true }}",
    GUARD,
    "",
    Some("cancel")
)]
#[case::no_group("", GUARD, "", Some("no concurrency group"))]
#[case::constant_group(
    "concurrency:\n  group: pub\n  cancel-in-progress: false",
    GUARD,
    "",
    Some("not exactly")
)]
#[case::keyed_on_the_event_too(
    "concurrency:\n  group: ${{ github.workflow }}-${{ github.ref }}-${{ github.event_name }}\n  \
     cancel-in-progress: false",
    GUARD,
    "",
    Some("not exactly")
)]
#[case::keyed_on_the_event_only(
    "concurrency:\n  group: ${{ github.workflow }}-${{ github.event_name }}\n  \
     cancel-in-progress: false",
    GUARD,
    "",
    Some("not exactly")
)]
#[case::literal_ref(
    "concurrency:\n  group: ${{ github.workflow }}-github.ref\n  cancel-in-progress: false",
    GUARD,
    "",
    Some("not exactly")
)]
#[case::spacing_is_not_shape(
    "concurrency:\n  group: ${{github.workflow}}-${{  github.ref  }}\n  cancel-in-progress: false",
    GUARD,
    "",
    None
)]
#[case::token_elsewhere(
    NEVER_CANCEL,
    GUARD,
    "      - run: echo ${{ secrets.CS_ACCESS_TOKEN }}",
    Some("other than the upload")
)]
fn the_publisher_rule_names_the_clause_broken(
    #[case] concurrency: &str,
    #[case] upload_if: &str,
    #[case] extra_step: &str,
    #[case] expected: Option<&str>,
) -> Result<()> {
    let source = publisher(concurrency, upload_if, extra_step);
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

/// The token check step's first line, as [`PUBLISHER`] writes it.
const CHECK_STEP: &str = "      - id: codescene-token\n";
/// A binding of the token in a step's `env`, indented for a step.
const STEP_BINDING: &str =
    "        env:\n          CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}\n";
/// A scope-wide declaration of the token, indented for the workflow root.
const WIDE_TOKEN: &str = "env:\n  CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}\n";

/// A job calling a reusable workflow, awaiting the mapping a case gives it.
const REUSABLE: &str = "  forward:\n    uses: ./.github/workflows/elsewhere.yml\n";

/// Scenario: the token is moved off the upload step, or declared more widely.
///
/// Invariant: each placement is named. Moving the token to the coverage step
/// satisfies "some step holds it" while the check reports it unset and
/// publishing silently stops; a workflow- or job-level `env` hands it to every
/// step; and the upload step's own `env` hands it to the composite action's
/// nested `upload-artifact` and cache steps, so only the check step's `env` and
/// the upload's `access-token` input may carry it.
#[rstest]
#[case::moved_to_coverage(
    |source: String| source.replace("        with:\n          with-ratchet", &format!("{STEP_BINDING}        with:\n          with-ratchet")),
    &["in its env", "other than the upload and its check"][..],
)]
#[case::workflow_env(|source: String| source.replace("jobs:\n", &format!("{WIDE_TOKEN}jobs:\n")), &["for every job"][..])]
#[case::job_env(
    |source: String| source.replace("    steps:\n", &format!("    {}    steps:\n", WIDE_TOKEN.replace("\n  ", "\n      "))),
    &["for every step"][..],
)]
#[case::forwarded_as_an_input(
    |source: String| source.replace("jobs:\n", &format!("jobs:\n{REUSABLE}    with:\n      token: ${{{{ secrets.CS_ACCESS_TOKEN }}}}\n")),
    &["to a reusable workflow"][..],
)]
#[case::forwarded_by_name(
    |source: String| source.replace("jobs:\n", &format!("jobs:\n{REUSABLE}    secrets:\n      CS_ACCESS_TOKEN: ${{{{ secrets.CS_ACCESS_TOKEN }}}}\n")),
    &["to a reusable workflow"][..],
)]
#[case::forwarded_by_inheritance(
    |source: String| source.replace("jobs:\n", &format!("jobs:\n{REUSABLE}    secrets: inherit\n")),
    &["to a reusable workflow"][..],
)]
#[case::bound_in_the_check(
    |source: String| source.replace(CHECK_STEP, &format!("{CHECK_STEP}{STEP_BINDING}")),
    &["in its env"][..],
)]
#[case::token_in_the_upload_env(
    |source: String| source.replace(
        "      - if: \"",
        "      - env:\n          CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}\n        if: \"",
    ),
    &["holds CS_ACCESS_TOKEN in its env"][..],
)]
#[case::held_elsewhere_in_another_case(
    |source: String| source.replace("        with:\n          with-ratchet", "        env:\n          T: ${{ secrets.Cs_Access_Token }}\n        with:\n          with-ratchet"),
    &["in its env", "other than the upload"][..],
)]
#[case::computed_elsewhere(
    |source: String| source.replace("        with:\n          with-ratchet", "        env:\n          T: ${{ secrets['CS_ACCESS_TOKEN'] }}\n        with:\n          with-ratchet"),
    &["computed name", "in its env"][..],
)]
fn the_token_sits_on_the_upload_alone(
    #[case] vary: fn(String) -> String,
    #[case] expected: &[&str],
) -> Result<()> {
    let source = vary(publisher(NEVER_CANCEL, GUARD, ""));
    let findings = rules::publisher_findings(&parse(&source)?);
    ensure!(
        findings.len() == expected.len()
            && expected
                .iter()
                .all(|clause| findings.iter().any(|f| f.contains(clause))),
        "expected findings naming {expected:?}, saw {findings:?}"
    );
    Ok(())
}

/// Scenario: a publisher reaches the CLI from a `run` body, plainly, split
/// across a shell continuation, or neutralized behind `false &&`.
///
/// Invariant: each is refused. An upload is read only from the action,
/// because a `run` body containing the command proves nothing about whether
/// it runs: `false && cs-coverage upload` contains it and never uploads.
#[rstest]
#[case::plain("      - run: cs-coverage upload --format lcov\n")]
#[case::continued("      - run: |\n          cs-coverage \\\n            upload --format lcov\n")]
#[case::neutralized("      - run: false && cs-coverage upload --format lcov\n")]
fn the_cli_is_refused_in_the_publisher(#[case] extra: &str) -> Result<()> {
    let findings = rules::publisher_findings(&parse(&publisher(NEVER_CANCEL, GUARD, extra))?);
    ensure!(
        findings.len() == 1
            && findings
                .iter()
                .all(|f| f.contains("runs cs-coverage directly")),
        "the CLI was not refused: {findings:?}"
    );
    Ok(())
}

/// Scenario: the required work sits behind a condition that can stop it.
///
/// Invariant: a conditional ratcheted coverage step does not count as the
/// publisher generating coverage, and a conditional publisher job is refused,
/// since `if: false` on either leaves every other clause reading the step as
/// present.
#[rstest]
#[case::coverage_step_if(
    "        with:\n          with-ratchet: 'true'\n",
    "        if: github.actor == 'x'\n        with:\n          with-ratchet: 'true'\n",
    "generates no ratcheted coverage"
)]
#[case::job_if("  coverage:\n", "  coverage:\n    if: false\n", "carries an `if:`")]
fn conditional_required_work_is_refused(
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

/// Scenario: a publisher that runs the upload action in `check` mode.
///
/// Invariant: that is not an upload, so the omission is reported.
#[test]
fn check_mode_is_not_an_upload() -> Result<()> {
    let source = publisher(NEVER_CANCEL, GUARD, "").replace(
        "          access-token:",
        "          mode: check\n          access-token:",
    );
    let findings = rules::publisher_findings(&parse(&source)?);
    ensure!(
        findings.iter().any(|f| f.contains("uploads nothing")),
        "check mode read as an upload: {findings:?}"
    );
    Ok(())
}
