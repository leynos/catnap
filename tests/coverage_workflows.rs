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

use anyhow::{Result, bail, ensure};
use serde_yaml::Value;

#[path = "coverage_workflows/closure_properties.rs"]
mod closure_properties;
#[path = "coverage_workflows/condition_cases.rs"]
mod condition_cases;
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
