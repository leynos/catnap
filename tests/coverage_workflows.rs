//! Holds `CodeScene` coverage publication on `main`, per concordat's CV-005.
//!
//! The rule has three clauses, and this file asserts each of them:
//!
//! 1. no workflow a pull request can reach invokes a `CodeScene` action, runs `cs-coverage`,
//!    contacts `codescene.io`, receives `CS_ACCESS_TOKEN`, forwards every secret with `secrets:
//!    inherit`, or publishes its coverage report;
//! 2. every `generate-coverage` step such a workflow runs sets `with-ratchet: true`, with baseline
//!    paths matching the main publisher's;
//! 3. exactly one workflow triggered by a push restricted to `main` generates ratcheted coverage
//!    and uploads it through the shared action, from the one step binding the token, behind a
//!    `github.ref == 'refs/heads/main'` conjunct, in a concurrency group that never cancels a run
//!    in progress.
//!
//! Why each clause is worth a test rather than a convention: a pull request
//! from a fork cannot read `secrets.CS_ACCESS_TOKEN`, so an upload step on the
//! pull-request lane is silently inert for exactly the changes that most need
//! reviewing, and `CodeScene` accepts an upload only for a branch it analyses,
//! which a pull request head is not. Both failures are quiet. Nothing goes
//! red; the data simply never arrives.
//!
//! "A workflow a pull request can reach" is a closure, not a trigger list. A
//! workflow declaring only `workflow_call` never names a pull request, yet a
//! pull-request job can call it with `secrets: inherit` and hand it the token.
//! [`reader::pull_request_closure`] follows local calls until nothing new is
//! reached, and every pull-request clause runs over what it returns.
//!
//! The judgements live in [`rules`] and [`publisher_rules`] and are driven
//! directly against complying and breaching fixtures in `pull_request_cases`
//! and `publisher_cases`, and the closure against generated call graphs in
//! `closure_properties`, because a rule exercised only over this repository's
//! own correct workflows would pass whether or not it detects anything. The
//! tests below then apply the same functions to the real files.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use serde_norway::Value;

#[path = "coverage_workflows/closure_properties.rs"]
mod closure_properties;
#[path = "coverage_workflows/condition_cases.rs"]
mod condition_cases;
#[path = "coverage_workflows/guard_cases.rs"]
mod guard_cases;
#[path = "coverage_workflows/publisher_cases.rs"]
mod publisher_cases;
#[path = "coverage_workflows/publisher_rules.rs"]
mod publisher_rules;
#[path = "coverage_workflows/pull_request_cases.rs"]
mod pull_request_cases;
#[path = "coverage_workflows/reader.rs"]
mod reader;
#[path = "coverage_workflows/rules.rs"]
mod rules;
#[path = "coverage_workflows/text.rs"]
mod text;
#[path = "coverage_workflows/token_check.rs"]
mod token_check;
#[path = "coverage_workflows/writer_cases.rs"]
mod writer_cases;
#[path = "coverage_workflows/writer_rules.rs"]
mod writer_rules;

/// Workflows a pull request is known to start.
///
/// The closure is computed from the directory, so a new workflow is covered
/// the day it lands. This names the floor it must still reach: a closure that
/// silently emptied would make the first clause pass having read nothing.
const KNOWN_PULL_REQUEST_WORKFLOWS: [&str; 2] = ["ci.yml", "dependabot-automerge.yml"];

/// Scenario: every workflow a pull request can reach is examined.
///
/// Invariant: none of them reaches `CodeScene` or publishes its report, and
/// each coverage step ratchets locally instead. The closure must contain the
/// known pull-request workflows, so emptying it cannot pass.
#[test]
fn no_workflow_a_pull_request_reaches_touches_codescene() -> Result<()> {
    let all = reader::workflows()?;
    let closure = reader::pull_request_closure(&all);
    for known in KNOWN_PULL_REQUEST_WORKFLOWS {
        ensure!(
            closure.contains(known),
            "{known} is no longer read as a pull-request workflow; the closure is {closure:?}"
        );
    }
    let breaches: Vec<String> = closure
        .iter()
        .filter_map(|name| Some((name, all.get(name)?)))
        .flat_map(|(name, workflow)| {
            rules::pull_request_findings(workflow)
                .into_iter()
                .map(move |finding| format!("{name}: {finding}"))
        })
        .chain(reader::missing_callees(&all, &closure))
        .collect();
    ensure!(breaches.is_empty(), "CV-005 breaches: {breaches:?}");
    Ok(())
}

/// Scenario: the repository is asked whether anything publishes coverage.
///
/// Invariant: exactly one workflow is triggered by a push restricted to
/// `main`, and it publishes as clause 3 requires, uploading the file and
/// format its coverage step writes with the token its step binds. Without
/// this, the first clause is satisfied by deleting the upload altogether.
#[test]
fn exactly_one_main_publisher_uploads_ratcheted_coverage() -> Result<()> {
    let all = reader::workflows()?;
    let publishers: Vec<(&String, &Value)> = all
        .iter()
        .filter(|(_, workflow)| publisher_rules::publishes_from_main(workflow))
        .collect();
    let [(name, workflow)] = publishers.as_slice() else {
        let names: Vec<_> = publishers.iter().map(|(name, _)| name).collect();
        bail!("expected one push-to-main publisher, found {names:?}");
    };
    ensure!(
        !reader::pull_request_closure(&all).contains(*name),
        "{name} publishes from main but a pull request can reach it"
    );
    let mut findings = publisher_rules::publisher_findings(workflow);
    findings.extend(publisher_rules::wiring_findings(workflow));
    ensure!(findings.is_empty(), "{name}: {findings:?}");
    Ok(())
}

/// Scenario: every workflow other than the publisher is examined.
///
/// Invariant: none holds the token, names the host, runs the CLI or the
/// uploader, or touches the retired installer digest. The pull-request and
/// publisher clauses between them leave a dispatch-only or tag-triggered
/// workflow unread; this one reads every file.
#[test]
fn only_the_publisher_reaches_codescene() -> Result<()> {
    let all = reader::workflows()?;
    let others: Vec<(&String, &Value)> = all
        .iter()
        .filter(|(_, workflow)| !publisher_rules::publishes_from_main(workflow))
        .collect();
    ensure!(
        !others.is_empty(),
        "no workflow besides the publisher was read"
    );
    let breaches: Vec<String> = others
        .into_iter()
        .flat_map(|(name, workflow)| {
            rules::stray_findings(workflow)
                .into_iter()
                .map(move |finding| format!("{name}: {finding}"))
        })
        .collect();
    ensure!(
        breaches.is_empty(),
        "CodeScene outside the publisher: {breaches:?}"
    );
    Ok(())
}

/// Scenario: every workflow a push starts, other than the publisher, and
/// every local workflow such a workflow calls, is examined.
///
/// Invariant: none can run a ratcheted coverage step on a push, so the
/// publisher is the baseline's only writer. A callee runs with its caller's
/// event, so the push side is followed as a closure too.
#[test]
fn only_the_publisher_writes_the_baseline() -> Result<()> {
    let breaches = writer_rules::second_writers(&reader::workflows()?);
    ensure!(breaches.is_empty(), "second baseline writers: {breaches:?}");
    Ok(())
}

/// The coverage selection the publisher and every pull-request lane run,
/// pinned here as the repository's own value.
///
/// "Each lane equals the publisher" passes when both change together, a new
/// output path or format on both sides, so the shared value is pinned too.
/// `publish-artefact` is lane-local and left out.
const COVERAGE_SELECTION: [(&str, &str); 3] = [
    ("format", "lcov"),
    ("output-path", "lcov.info"),
    ("with-ratchet", "true"),
];

/// Returns a coverage step's `with` inputs, less the lane-local ones, and its
/// `env`, as strings.
fn selection(step: &serde_norway::Mapping) -> (BTreeMap<String, String>, String) {
    let inputs = reader::get(step, "with")
        .and_then(Value::as_mapping)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            let name = key.as_str()?;
            let text = value.as_str().map_or_else(
                || serde_norway::to_string(value).unwrap_or_default(),
                str::to_owned,
            );
            (name != "publish-artefact").then(|| (name.to_owned(), text.trim().to_owned()))
        })
        .collect();
    let env = reader::get(step, "env")
        .map(|value| serde_norway::to_string(value).unwrap_or_default())
        .unwrap_or_default();
    (inputs, env)
}

/// Scenario: the publisher's coverage step and each pull-request lane's are
/// compared, and the publisher's against the pinned selection.
///
/// Invariant: the publisher measures exactly [`COVERAGE_SELECTION`], and every
/// lane measures what the publisher does, with the same step environment, so
/// the baseline a pull request is compared against measured the same thing.
#[test]
fn the_coverage_selection_is_pinned() -> Result<()> {
    let all = reader::workflows()?;
    let published: Vec<_> = all
        .values()
        .filter(|workflow| publisher_rules::publishes_from_main(workflow))
        .flat_map(reader::steps)
        .filter(|step| rules::is_coverage(step))
        .map(selection)
        .collect();
    let [(inputs, env)] = published.as_slice() else {
        bail!("expected one publisher coverage step, saw {published:?}");
    };
    let pinned: BTreeMap<String, String> = COVERAGE_SELECTION
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    ensure!(
        *inputs == pinned,
        "the publisher measures {inputs:?}, pinned {pinned:?}"
    );
    for name in reader::pull_request_closure(&all) {
        let Some(workflow) = all.get(&name) else {
            continue;
        };
        for lane in reader::steps(workflow)
            .into_iter()
            .filter(|step| rules::is_coverage(step))
        {
            let (lane_inputs, lane_env) = selection(lane);
            ensure!(
                (&lane_inputs, &lane_env) == (inputs, env),
                "{name} measures {lane_inputs:?} with env {lane_env:?}; the publisher {inputs:?} \
                 with {env:?}"
            );
        }
    }
    Ok(())
}

/// A coverage step's baseline files, as `(workflow, Rust file, Python file)`.
type Baseline = (String, String, String);

/// Returns the baseline paths every coverage step in `workflow` reads.
fn baselines(name: &str, workflow: &Value) -> Vec<Baseline> {
    reader::steps(workflow)
        .into_iter()
        .filter(|step| rules::is_coverage(step))
        .map(|step| {
            let with = reader::get(step, "with").and_then(Value::as_mapping);
            let path = |key: &str| {
                with.and_then(|inputs| reader::get(inputs, key))
                    .and_then(Value::as_str)
                    .unwrap_or("<default>")
                    .to_owned()
            };
            (
                name.to_owned(),
                path("baseline-rust-file"),
                path("baseline-python-file"),
            )
        })
        .collect()
}

/// Scenario: the publisher and each pull-request lane are compared.
///
/// Invariant: every lane reads the baseline the publisher writes. The
/// comparison is per lane against the publisher rather than pooled, so one
/// lane that agrees cannot cover for another that does not.
#[test]
fn every_pull_request_lane_reads_the_publisher_baseline() -> Result<()> {
    let all = reader::workflows()?;
    let written: Vec<_> = all
        .iter()
        .filter(|(_, workflow)| publisher_rules::publishes_from_main(workflow))
        .flat_map(|(name, workflow)| baselines(name, workflow))
        .collect();
    let [(_, rust, python)] = written.as_slice() else {
        bail!("expected one publisher coverage step, saw {written:?}");
    };
    let read: Vec<_> = reader::pull_request_closure(&all)
        .iter()
        .filter_map(|name| Some(baselines(name, all.get(name)?)))
        .flatten()
        .collect();
    ensure!(!read.is_empty(), "no pull-request lane measures coverage");
    for (name, lane_rust, lane_python) in &read {
        ensure!(
            (lane_rust, lane_python) == (rust, python),
            "{name} reads {lane_rust} / {lane_python}, the publisher writes {rust} / {python}"
        );
    }
    Ok(())
}
