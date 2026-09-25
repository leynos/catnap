//! The CV-005 judgement on second writers of the ratchet baseline.
//!
//! Split from `publisher_rules.rs` under the repository's 400-line cap. The
//! publisher is the baseline's only writer; these rules read every other
//! workflow a push can start, and every one such a workflow calls.

use serde_norway::Value;

use super::{
    publisher_rules::{guarded_exactly, publishes_from_main},
    reader,
    rules::{input_is, is_coverage},
};

/// The conjunct that keeps a lane's coverage step off a push.
const PULL_REQUEST_GUARD: &str = "github.event_name == 'pull_request'";

/// Returns the reasons a workflow other than the publisher could write the
/// ratchet baseline.
///
/// The shared action saves the baseline on any push to `main` that runs a
/// ratcheted coverage step. A workflow a push starts, or one such a workflow
/// calls (a callee runs with its caller's event), would race the publisher
/// for the baseline every pull request is measured against, so each of its
/// ratcheted coverage steps must carry the pull-request guard as a conjunct.
pub fn second_writer_findings(workflow: &Value) -> Vec<String> {
    reader::steps(workflow)
        .into_iter()
        .filter(|step| {
            is_coverage(step)
                && input_is(step, "with-ratchet", true)
                && !guarded_exactly(step, &[PULL_REQUEST_GUARD])
        })
        .map(|_| {
            format!("a ratcheted coverage step can run on push without `{PULL_REQUEST_GUARD}`")
        })
        .collect()
}

/// Returns each ratcheted coverage step a push can run outside the publisher.
///
/// Seeds are the workflows a push starts, other than the publisher; the
/// closure then takes in every local workflow they call, since a callee runs
/// with its caller's push. Each entry names the workflow and the finding.
pub fn second_writers(all: &reader::Workflows) -> Vec<String> {
    let is_push_lane = |workflow: &Value| {
        reader::trigger_names(workflow)
            .iter()
            .any(|name| name == "push")
            && !publishes_from_main(workflow)
    };
    let seeds = all
        .iter()
        .filter(|(_, workflow)| is_push_lane(workflow))
        .map(|(name, _)| name.clone())
        .collect();
    reader::closure_from(all, seeds)
        .iter()
        .filter_map(|name| Some((name, all.get(name)?)))
        .filter(|(_, workflow)| !publishes_from_main(workflow))
        .flat_map(|(name, workflow)| {
            second_writer_findings(workflow)
                .into_iter()
                .map(move |finding| format!("{name}: {finding}"))
        })
        .collect()
}
