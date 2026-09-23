//! The CV-005 judgements, each returning the reasons a workflow fails.
//!
//! An empty vector means the workflow complies. Returning reasons rather than
//! a boolean lets the fixture cases assert *which* clause fired, so a rule
//! that fails for the wrong reason cannot pass as one that works.

use serde_norway::{Mapping, Value};

use super::{
    reader::{self, get, uses},
    text::{computes_a_secret, folded},
};

/// The action the estate uses to publish coverage to `CodeScene`.
pub const UPLOAD_ACTION: &str = "leynos/shared-actions/.github/actions/upload-codescene-coverage";
/// The action that measures coverage and maintains the ratchet baseline.
pub const COVERAGE_ACTION: &str = "leynos/shared-actions/.github/actions/generate-coverage";
/// The secret a pull-request lane must not receive.
pub(super) const ACCESS_TOKEN: &str = "CS_ACCESS_TOKEN";
/// [`ACCESS_TOKEN`] case-folded, as the searched text is: secret names are
/// case-insensitive.
const ACCESS_TOKEN_FOLDED: &str = "cs_access_token";
/// The CLI a lane must not reach for directly either.
pub(super) const COVERAGE_CLI: &str = "cs-coverage";
/// The service host, however it is reached.
const CODESCENE_HOST: &str = "codescene.io";
/// The variable the retired installer-digest refresher wrote, case-folded.
const CLI_DIGEST_VARIABLE: &str = "codescene_cli_sha256";
/// The uploader's retired digest input, case-folded.
const INSTALLER_CHECKSUM: &str = "installer-checksum";

/// Returns whether a workflow names the retired installer digest anywhere.
///
/// The uploader now verifies its archive against a committed manifest and
/// rejects a non-empty `installer-checksum`, so either name is a relic that
/// would fail the step or feed nothing; no workflow may carry one, the
/// publisher included.
pub(super) fn names_the_retired_digest(workflow: &Value) -> bool {
    let text = folded(workflow);
    text.contains(CLI_DIGEST_VARIABLE) || text.contains(INSTALLER_CHECKSUM)
}

/// Collapses every run of whitespace to one space.
pub(super) fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Returns a step's `with` input as written, if present.
fn input<'a>(step: &'a Mapping, key: &str) -> Option<&'a Value> {
    get(step, "with")
        .and_then(Value::as_mapping)
        .and_then(|with| get(with, key))
}

/// Returns a step's `with` input as a string, if it is one.
pub(super) fn input_str<'a>(step: &'a Mapping, key: &str) -> Option<&'a str> {
    input(step, key).and_then(Value::as_str)
}

/// Reads a workflow input as a boolean, accepting the string and native forms.
pub(super) fn input_is(step: &Mapping, key: &str, expected: bool) -> bool {
    input(step, key).is_some_and(|value| {
        value.as_bool() == Some(expected) || value.as_str() == Some(expected.to_string().as_str())
    })
}

/// Returns whether a step runs the shared coverage action.
pub(super) fn is_coverage(step: &Mapping) -> bool {
    uses(step).is_some_and(|r| r.to_ascii_lowercase().starts_with(COVERAGE_ACTION))
}

/// Returns whether a step runs the upload action.
pub(super) fn is_upload_action(step: &Mapping) -> bool {
    uses(step).is_some_and(|r| r.to_ascii_lowercase().starts_with(UPLOAD_ACTION))
}

/// Returns whether a step uploads to `CodeScene` through the shared action.
///
/// `upload` is the action's default mode, so an absent mode is an upload,
/// and `check` is not one: it gates changed lines and publishes nothing.
pub(super) fn is_upload(step: &Mapping) -> bool {
    is_upload_action(step) && matches!(input_str(step, "mode"), None | Some("upload"))
}

/// Returns whether a step's `run` body names the `CodeScene` CLI.
pub(super) fn runs_the_cli(step: &Mapping) -> bool {
    get(step, "run")
        .and_then(Value::as_str)
        .is_some_and(|run| run.contains(COVERAGE_CLI))
}

/// Returns the reasons a workflow a pull request can reach breaches CV-005.
pub fn pull_request_findings(workflow: &Value) -> Vec<String> {
    let mut findings = document_findings(workflow);
    findings.extend(job_findings(workflow));
    findings.extend(
        reader::steps(workflow)
            .into_iter()
            .flat_map(pull_request_step_findings),
    );
    findings
}

/// Returns the findings for the jobs of a pull-request lane.
///
/// `secrets: inherit` hands a called workflow every secret without naming
/// one, and a refused local call names a file the closure cannot follow.
fn job_findings(workflow: &Value) -> Vec<String> {
    let inherits = reader::jobs(workflow)
        .into_iter()
        .filter(|(_, job)| get(job, "secrets").and_then(Value::as_str) == Some("inherit"))
        .map(|(id, _)| format!("job {id} forwards every secret with `secrets: inherit`"));
    let refused = reader::job_calls(workflow)
        .into_iter()
        .filter(|(_, reference)| reader::classify_call(reference) == reader::Call::Refused)
        .map(|(id, reference)| {
            format!("job {id} calls `{reference}`, which resolves to no workflow here")
        });
    inherits.chain(refused).collect()
}

/// Returns the findings that search the whole parsed document.
///
/// Every scalar is read, case-folded for the host, so a workflow-level
/// `defaults.run.shell`, a `workflow_call` secret declaration or any other
/// place a value can sit is searched without a clause naming it.
fn document_findings(workflow: &Value) -> Vec<String> {
    let text = folded(workflow);
    let mut findings = Vec::new();
    if text.contains(ACCESS_TOKEN_FOLDED) {
        findings.push(format!("a pull-request lane receives {ACCESS_TOKEN}"));
    }
    if computes_a_secret(&text) {
        findings.push("a pull-request lane reaches a secret by a computed name".to_owned());
    }
    if text.contains(CODESCENE_HOST) {
        findings.push(format!("a pull-request lane contacts {CODESCENE_HOST}"));
    }
    findings
}

/// Returns the findings for one step of a pull-request lane.
fn pull_request_step_findings(step: &Mapping) -> Vec<String> {
    let mut findings = Vec::new();
    if is_upload_action(step) {
        findings.push(format!("a pull-request lane invokes {UPLOAD_ACTION}"));
    }
    if runs_the_cli(step) {
        findings.push(format!("a pull-request lane runs {COVERAGE_CLI} directly"));
    }
    if is_coverage(step) && !input_is(step, "with-ratchet", true) {
        findings.push("a pull-request coverage step does not set with-ratchet".to_owned());
    }
    if is_coverage(step) && !input_is(step, "publish-artefact", false) {
        findings.push("a pull-request coverage step publishes its report".to_owned());
    }
    findings
}

/// Returns the reasons a workflow other than the publisher reaches `CodeScene`.
///
/// The pull-request clauses read only what a pull request can reach, and the
/// publisher clauses only the publisher, so a dispatch-only or tag-triggered
/// workflow would escape both. Only the publisher may hold the token, name the
/// host, run the CLI or the uploader, or touch the retired installer digest.
pub fn stray_findings(workflow: &Value) -> Vec<String> {
    let text = folded(workflow);
    let steps = reader::steps(workflow);
    [
        (
            text.contains(ACCESS_TOKEN_FOLDED),
            format!("receives {ACCESS_TOKEN}"),
        ),
        (
            text.contains(CODESCENE_HOST),
            format!("contacts {CODESCENE_HOST}"),
        ),
        (
            names_the_retired_digest(workflow),
            "names the retired installer digest".to_owned(),
        ),
        (
            steps.iter().any(|step| is_upload_action(step)),
            format!("invokes {UPLOAD_ACTION}"),
        ),
        (
            steps.iter().any(|step| runs_the_cli(step)),
            format!("runs {COVERAGE_CLI}"),
        ),
    ]
    .into_iter()
    .filter(|(is_hit, _)| *is_hit)
    .map(|(_, reason)| format!("a workflow other than the publisher {reason}"))
    .collect()
}
