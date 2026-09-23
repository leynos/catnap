//! The CV-005 judgements on the push-to-`main` publisher.
//!
//! Split from `rules.rs` under the repository's 400-line cap. As there, each
//! function returns the reasons a workflow fails, so a fixture case can assert
//! which clause fired.

use std::iter;

use serde_norway::{Mapping, Value};

use super::{
    reader::{self, get},
    rules::{
        ACCESS_TOKEN,
        COVERAGE_CLI,
        input_is,
        input_str,
        is_coverage,
        is_upload,
        is_upload_action,
        normalized,
        runs_the_cli,
    },
    text::{computes_a_secret, folded, folded_mapping},
    token_check::{check_findings, check_id, check_steps},
};

/// The expression that hands a step the secret itself.
///
/// The publisher's placement clauses look for this reference to the secret
/// itself, which is what hands a step the value.
///
/// Case-folded, as the searched text is: context and secret names are
/// case-insensitive, so `secrets.Cs_Access_Token` is the same reference.
const SECRET_REFERENCE: &str = "secrets.cs_access_token";
/// The upload action's `access-token` input, whitespace normalized.
///
/// Passed as the secret itself, not through the step's `env`: the action is
/// composite and hands its step `env` to its nested `upload-artifact` and cache
/// steps, while it binds the token itself from `inputs.access-token`.
const TOKEN_INPUT: &str = "${{ secrets.CS_ACCESS_TOKEN }}";
/// The conjunct that restricts the publisher's upload to the trunk.
const MAIN_REF_GUARD: &str = "github.ref == 'refs/heads/main'";
/// The triggers the publisher answers, exactly: a push to `main` writes the
/// baseline and uploads, and a dispatch measures without advancing it.
const PUBLISHER_TRIGGERS: [&str; 2] = ["push", "workflow_dispatch"];

/// Returns whether a workflow is triggered by a push restricted to `main`.
///
/// A push with no branch filter is not a main publisher: it fires on every
/// branch, so the baseline it writes would be whichever branch pushed last.
/// A tag filter fails too, since it names no branch at all.
pub fn publishes_from_main(workflow: &Value) -> bool {
    reader::trigger(workflow, "push")
        .and_then(Value::as_mapping)
        .and_then(|push| get(push, "branches"))
        .and_then(Value::as_sequence)
        .is_some_and(|branches| {
            !branches.is_empty()
                && branches
                    .iter()
                    .all(|branch| branch.as_str() == Some("main"))
        })
}

/// Blanks every character inside a single-quoted literal, keeping the quotes.
///
/// Blanked byte for byte, so an offset found in the result is an offset in
/// the input even when a literal holds a multi-byte character.
fn unquoted(body: &str) -> String {
    let mut in_quote = false;
    body.chars()
        .map(|character| {
            in_quote ^= character == '\'';
            if in_quote && character != '\'' {
                " ".repeat(character.len_utf8())
            } else {
                character.to_string()
            }
        })
        .collect()
}

/// Returns the conjuncts of an `if:` condition, or `None` if it has a `||` or
/// a parenthesis outside a quoted literal.
///
/// Quoted literals are blanked before the operators are looked for, so a
/// `||` inside a string does not count and an `&&` inside one does not
/// split. A disjunction anywhere makes every conjunct optional, which is why
/// it is refused rather than parsed: `... && ref == main || dispatch` passes
/// any substring search for the ref and uploads a dispatch from any branch.
/// A parenthesis can group or negate conjuncts, as in `!(a && ref == main)`,
/// so a flat split would report a conjunct the expression does not require.
pub fn conjuncts(condition: &str) -> Option<Vec<String>> {
    let trimmed = condition.trim();
    let body = trimmed
        .strip_prefix("${{")
        .and_then(|inner| inner.strip_suffix("}}"))
        .unwrap_or(trimmed);
    let blanked = unquoted(body);
    if blanked.contains("||") || blanked.contains(['(', ')']) {
        return None;
    }
    let operators: Vec<usize> = blanked.match_indices("&&").map(|(at, _)| at).collect();
    let starts = iter::once(0).chain(operators.iter().map(|at| at + 2));
    let ends = operators.iter().copied().chain(iter::once(body.len()));
    Some(
        starts
            .zip(ends)
            .map(|(start, end)| normalized(body.get(start..end).unwrap_or_default()))
            .collect(),
    )
}

/// Returns the conjunct that reads the token check step's answer.
fn check_guard(id: &str) -> String { format!("steps.{id}.outputs.available == 'true'") }

/// Returns whether an upload step's condition is exactly the token check's
/// answer and the ref guard.
///
/// Exact set equality, not "contains the ref conjunct": an extra conjunct
/// can only narrow or defeat the upload (`&& false`, a second ref), and a
/// negated group would carry the ref conjunct while uploading everywhere
/// else. Nothing a publisher needs is lost by refusing all of them.
fn guarded_to_main(step: &Mapping, check_id: Option<&str>) -> bool {
    check_id.is_some_and(|id| guarded_exactly(step, &[check_guard(id).as_str(), MAIN_REF_GUARD]))
}

/// Returns whether a step's condition is exactly the conjuncts `guards`.
pub(super) fn guarded_exactly(step: &Mapping, guards: &[&str]) -> bool {
    get(step, "if")
        .and_then(Value::as_str)
        .and_then(conjuncts)
        .is_some_and(|parts| {
            let found: std::collections::BTreeSet<&str> =
                parts.iter().map(String::as_str).collect();
            found == guards.iter().copied().collect()
        })
}

/// Returns the reasons the token is declared in a scope wider than one step.
fn wide_token_findings(workflow: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    if workflow
        .as_mapping()
        .and_then(|root| get(root, "env"))
        .is_some_and(|env| folded(env).contains(SECRET_REFERENCE))
    {
        findings.push(format!(
            "the publisher declares {ACCESS_TOKEN} for every job"
        ));
    }
    for (id, job) in reader::jobs(workflow) {
        if get(job, "env").is_some_and(|env| folded(env).contains(SECRET_REFERENCE)) {
            findings.push(format!("job {id} declares {ACCESS_TOKEN} for every step"));
        }
        if forwards_the_token(job) {
            findings.push(format!(
                "job {id} forwards {ACCESS_TOKEN} to a reusable workflow"
            ));
        }
    }
    findings
}

/// Returns the reasons the token reaches somewhere other than the upload.
///
/// Only the upload's `access-token` input and the check step's expression
/// may name the secret, and no step may hold it in its `env`: the upload
/// action is composite and hands a step `env` to its nested
/// `upload-artifact` and cache steps, and a wider scope reaches every step at once.
fn token_findings(workflow: &Value) -> Vec<String> {
    let mut findings = wide_token_findings(workflow);
    if computes_a_secret(&folded(workflow)) {
        findings.push("the publisher reaches a secret by a computed name".to_owned());
    }
    findings.extend(check_findings(workflow));
    let checks = check_steps(workflow);
    for step in reader::steps(workflow) {
        let holds = folded_mapping(step).contains(SECRET_REFERENCE);
        let is_exempt = is_upload(step) || checks.iter().any(|check| std::ptr::eq(*check, step));
        if get(step, "env").is_some_and(|env| folded(env).contains("cs_access_token")) {
            findings.push(format!(
                "a publisher step holds {ACCESS_TOKEN} in its env, which the upload's nested \
                 steps would inherit"
            ));
        }
        if !is_exempt && holds {
            findings.push(format!(
                "a step other than the upload and its check receives {ACCESS_TOKEN}"
            ));
        }
    }
    findings
}

/// Returns whether a job calling a reusable workflow hands it the token.
///
/// Such a job has no steps, so the step clauses never see it: the token can
/// travel through its `with:` inputs, a named `secrets:` entry, or
/// `secrets: inherit`, and the called workflow then holds it outside the one
/// upload step the publisher is allowed.
fn forwards_the_token(job: &Mapping) -> bool {
    if get(job, "uses").is_none() {
        return false;
    }
    let inherits = get(job, "secrets").and_then(Value::as_str) == Some("inherit");
    let names_it = ["with", "secrets"]
        .iter()
        .filter_map(|key| get(job, key))
        .any(|value| folded(value).contains(SECRET_REFERENCE));
    inherits || names_it
}

/// Returns the reasons the publisher's runs could cancel one another.
///
/// A cancelled publisher abandons both its upload and its baseline write, so
/// runs share a group that never cancels the run in progress: a newer push
/// replaces a pending run rather than queueing behind it, and the newest
/// baseline wins. Any `cancel-in-progress` other than an absent key or a
/// literal `false` is refused, an expression included: the question is
/// whether a push to `main` can ever be cancelled, and only the literal
/// answers it without evaluation.
fn concurrency_findings(workflow: &Value) -> Vec<String> {
    let group = workflow
        .as_mapping()
        .and_then(|root| get(root, "concurrency"));
    let job_groups = reader::jobs(workflow)
        .into_iter()
        .filter_map(|(_, job)| get(job, "concurrency"));
    let blocks: Vec<&Value> = group.into_iter().chain(job_groups).collect();
    let missing = group
        .is_none()
        .then(|| "the publisher declares no concurrency group".to_owned());
    let cancels = blocks
        .iter()
        .filter(|block| may_cancel(block))
        .map(|_| "the publisher may cancel a run in progress".to_owned());
    let shared = blocks
        .iter()
        .filter(|block| !is_keyed_on_the_ref(block))
        .map(|_| {
            "the publisher's concurrency group is not exactly `${{ github.workflow }}-${{ \
             github.ref }}`"
                .to_owned()
        });
    missing.into_iter().chain(cancels).chain(shared).collect()
}

/// Returns whether a concurrency block may cancel the run in progress.
fn may_cancel(concurrency: &Value) -> bool {
    concurrency
        .as_mapping()
        .and_then(|mapping| get(mapping, "cancel-in-progress"))
        .is_some_and(|value| value.as_bool() != Some(false))
}

/// The publisher's concurrency group, exactly, whitespace removed.
const PUBLISHER_GROUP: &str = "${{github.workflow}}-${{github.ref}}";

/// Returns whether a concurrency block's group is exactly the workflow and the
/// evaluated ref.
///
/// One group per ref, never per event: runs in it never overlap, and the
/// survivor of any replacement is the newest trigger, whose commit is the
/// newest `main` at trigger time, so uploads land in commit order. A group
/// keyed on the event as well lets an earlier dispatch finish after a newer
/// push and upload older coverage last. The key must be the evaluated
/// expression; a literal `github.ref` keys nothing. Every level is read,
/// since a job-level group of another shape overrides the workflow's.
fn is_keyed_on_the_ref(concurrency: &Value) -> bool {
    concurrency
        .as_str()
        .or_else(|| {
            concurrency
                .as_mapping()
                .and_then(|mapping| get(mapping, "group"))
                .and_then(Value::as_str)
        })
        .is_some_and(|group| group.split_whitespace().collect::<String>() == PUBLISHER_GROUP)
}

/// Returns the reasons the publisher's required work might never run.
///
/// A requirement met by a step that cannot run is not met: a ratcheted
/// coverage step or an upload inside a job carrying `if: false` reads as
/// present to every other clause. So no publisher job may carry a condition,
/// the ratcheted coverage step must carry none, and the upload is read only
/// from the action, never from a `run` body, where `false && cs-coverage
/// upload` still contains the command.
fn reachability_findings(workflow: &Value) -> Vec<String> {
    let mut findings: Vec<String> = reader::jobs(workflow)
        .into_iter()
        .filter(|(_, job)| get(job, "if").is_some())
        .map(|(id, _)| format!("publisher job {id} carries an `if:`"))
        .collect();
    let steps = reader::steps(workflow);
    if !steps.iter().any(|step| {
        is_coverage(step) && input_is(step, "with-ratchet", true) && get(step, "if").is_none()
    }) {
        findings.push("the main publisher generates no ratcheted coverage".to_owned());
    }
    if steps.iter().any(|step| runs_the_cli(step)) {
        findings.push(format!(
            "the publisher runs {COVERAGE_CLI} directly rather than through the action"
        ));
    }
    findings
}

/// Returns the reasons a main publisher fails to publish what CV-005 requires.
pub fn publisher_findings(workflow: &Value) -> Vec<String> {
    let mut findings = reachability_findings(workflow);
    let mut triggers = reader::trigger_names(workflow);
    triggers.sort();
    if triggers != PUBLISHER_TRIGGERS {
        findings.push(format!(
            "the publisher answers {triggers:?}, not exactly {PUBLISHER_TRIGGERS:?}"
        ));
    }
    let steps = reader::steps(workflow);
    let uploads: Vec<_> = steps.iter().filter(|step| is_upload(step)).collect();
    if uploads.is_empty() {
        findings.push("the main publisher uploads nothing to CodeScene".to_owned());
    }
    let check = check_id(workflow);
    if uploads.iter().any(|step| !guarded_to_main(step, check)) {
        findings.push(format!(
            "an upload step is not guarded by exactly the token check's answer and \
             `{MAIN_REF_GUARD}`"
        ));
    }
    findings.extend(token_findings(workflow));
    findings.extend(concurrency_findings(workflow));
    findings
}

/// Returns the reasons the publisher's upload would not send what it measured.
///
/// Kept apart from [`publisher_findings`] because it compares two steps of a
/// complete publisher: each upload must read the file, in the format, that a
/// coverage step writes, or it uploads nothing useful while every other
/// clause passes; and it must pass the secret itself as its
/// `access-token`, or its own guard holds while the action runs
/// unauthenticated.
pub fn wiring_findings(workflow: &Value) -> Vec<String> {
    let steps = reader::steps(workflow);
    let written: Vec<(Option<&str>, Option<&str>)> = steps
        .iter()
        .filter(|step| is_coverage(step))
        .map(|step| (input_str(step, "output-path"), input_str(step, "format")))
        .collect();
    let mut findings = Vec::new();
    for upload in steps.iter().filter(|step| is_upload_action(step)) {
        let read = (input_str(upload, "path"), input_str(upload, "format"));
        if !written.contains(&read) {
            findings.push(format!(
                "the upload reads {read:?}, which no coverage step writes; written: {written:?}"
            ));
        }
        let token = input_str(upload, "access-token").map(normalized);
        if token.as_deref() != Some(TOKEN_INPUT) {
            findings.push(format!(
                "the upload's access-token is {token:?}, not the secret itself"
            ));
        }
    }
    findings
}
