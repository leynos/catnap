//! Drives the second-writer rule and the pull-request seed events.
//!
//! Split from the other case modules under the repository's 400-line cap.
//! Both halves are about which workflows a clause must read: every lane a
//! push can start must stay off the baseline, and every event that runs a
//! workflow for a pull request must seed the pull-request closure.

use anyhow::{Result, ensure};
use rstest::rstest;

use super::{pull_request_cases::parse, reader, writer_rules};

/// A lane that runs on both a push and a pull request, its coverage guard a
/// placeholder.
const PUSH_AND_PULL_REQUEST: &str = "on:\n  push:\n    branches: [main]\n  pull_request:\njobs:\n  build:\n    steps:\n      - uses: leynos/shared-actions/.github/actions/generate-coverage@abc\n        if: \"@GUARD@\"\n        with:\n          with-ratchet: 'true'\n";

/// Scenario: a lane runs on a push and on a pull request, its ratcheted
/// coverage step guarded in each way.
///
/// Invariant: only the pull-request guard, as a whole conjunct, keeps the step
/// off a push, where it would write a second baseline.
#[rstest]
#[case::guarded("github.event_name == 'pull_request'", 0)]
#[case::push_only("github.event_name == 'push'", 1)]
#[case::disjunction("github.event_name == 'pull_request' || true", 1)]
#[case::unguarded("always()", 1)]
#[case::negated_group(
    "!(github.actor == 'x' && github.event_name == 'pull_request' && true)",
    1
)]
fn a_lane_on_push_keeps_its_ratchet_off_the_push(
    #[case] guard: &str,
    #[case] expected: usize,
) -> Result<()> {
    let source = PUSH_AND_PULL_REQUEST.replace("@GUARD@", guard);
    let findings = writer_rules::second_writer_findings(&parse(&source)?);
    ensure!(
        findings.len() == expected,
        "expected {expected}, saw {findings:?}"
    );
    Ok(())
}

/// Scenario: a push-started workflow calls a local reusable workflow whose
/// ratcheted coverage step is unguarded.
///
/// Invariant: the query the real-file test runs reaches the callee through
/// the push closure and reports it alone. A callee declares only
/// `workflow_call`, yet runs with its caller's push.
#[test]
fn a_callee_of_a_push_lane_is_a_second_writer() -> Result<()> {
    let push_lane = "on:\n  push:\n    branches: ['**']\njobs:\n  call:\n    uses: \
                     ./.github/workflows/cov.yml\n";
    let reusable = PUSH_AND_PULL_REQUEST
        .replace(
            "on:\n  push:\n    branches: [main]\n  pull_request:\n",
            "on: workflow_call\n",
        )
        .replace("@GUARD@", "always()");
    let all: reader::Workflows = [
        ("caller.yml".to_owned(), parse(push_lane)?),
        ("cov.yml".to_owned(), parse(&reusable)?),
    ]
    .into();
    let writers = writer_rules::second_writers(&all);
    ensure!(
        writers.len() == 1 && writers.iter().all(|writer| writer.starts_with("cov.yml")),
        "expected the callee alone, saw {writers:?}"
    );
    Ok(())
}

/// Scenario: a workflow starts on each event that runs it for a pull request.
///
/// Invariant: each seeds the pull-request closure. A queued merge and a review
/// run with the repository's secrets for a same-repository pull request, and a
/// `workflow_run` workflow runs after whatever it names.
#[rstest]
#[case::merge_group("on: merge_group\njobs: {}\n")]
#[case::review("on:\n  pull_request_review:\n    types: [submitted]\njobs: {}\n")]
#[case::review_comment("on: [pull_request_review_comment]\njobs: {}\n")]
#[case::workflow_run("on:\n  workflow_run:\n    workflows: [CI]\njobs: {}\n")]
#[case::issue_comment("on: issue_comment\njobs: {}\n")]
#[case::push_every_branch("on: push\njobs: {}\n")]
#[case::push_glob("on:\n  push:\n    branches: ['**']\njobs: {}\n")]
#[case::push_ignoring("on:\n  push:\n    branches-ignore: [gh-pages]\njobs: {}\n")]
#[case::push_main_and_more("on:\n  push:\n    branches: [main, 'feature/*']\njobs: {}\n")]
fn every_pull_request_event_seeds_the_closure(#[case] source: &str) -> Result<()> {
    ensure!(
        reader::starts_on_pull_request(&parse(source)?),
        "not read as a pull-request workflow: {source:?}"
    );
    Ok(())
}

/// Scenario: a workflow runs on a push to `main` only, on tags only, or on a
/// schedule.
///
/// Invariant: none seeds the pull-request closure. The seed rule must be
/// narrow as well as sufficient: reading every push as a pull request would
/// put the publisher itself on the pull-request surface.
#[rstest]
#[case::push_main("on:\n  push:\n    branches: [main]\njobs: {}\n")]
#[case::push_tags("on:\n  push:\n    tags: ['v*']\njobs: {}\n")]
#[case::schedule("on:\n  schedule:\n    - cron: '0 0 * * *'\njobs: {}\n")]
fn trunk_tag_and_scheduled_runs_stay_off_the_surface(#[case] source: &str) -> Result<()> {
    ensure!(
        !reader::starts_on_pull_request(&parse(source)?),
        "read as a pull-request workflow: {source:?}"
    );
    Ok(())
}
