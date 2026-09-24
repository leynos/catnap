//! The CV-005 token check step the publisher's upload is guarded on.
//!
//! Split from `publisher_rules.rs` under the repository's 400-line cap. The
//! upload action is composite and hands a step `env` to its nested steps, so
//! no step holds the token in its `env`: a check step reports whether the
//! secret is set, and the upload reads that answer.

use serde_norway::{Mapping, Value};

use super::{
    reader::{self, get},
    rules::is_upload,
};

/// The token check step's one command, exactly.
///
/// The expression evaluates to `true` or `false` before the shell runs, so
/// there is no shell conditional and the step binds nothing: the upload can
/// be guarded on the answer while no step holds the token in its `env`.
const CHECK_COMMAND: &str =
    r#"echo "available=${{ secrets.CS_ACCESS_TOKEN != '' }}" >> "$GITHUB_OUTPUT""#;

/// Returns the steps that run [`CHECK_COMMAND`], and nothing else.
pub(super) fn check_steps(workflow: &Value) -> Vec<&Mapping> {
    reader::steps(workflow)
        .into_iter()
        .filter(|step| {
            get(step, "run")
                .and_then(Value::as_str)
                .is_some_and(|run| run.trim() == CHECK_COMMAND)
        })
        .collect()
}

/// Returns the token check step's id, when there is exactly one such step.
pub(super) fn check_id(workflow: &Value) -> Option<&str> {
    match check_steps(workflow).as_slice() {
        [step] => get(step, "id").and_then(Value::as_str),
        _ => None,
    }
}

/// The keys the token check step may declare, besides its absent `if:`.
///
/// Anything else can stop it answering while it still reads as present: a
/// `shell` of `bash -c 'exit 0; {0}'` runs nothing, and `continue-on-error`
/// lets a failed write pass, so the upload skips forever either way.
const CHECK_STEP_KEYS: [&str; 3] = ["id", "name", "run"];

/// Returns whether every upload runs after the token check, in its job.
///
/// A `steps.<id>` context resolves only in the job that ran the step, and
/// only once it has run, so a check in another job or after the upload
/// leaves the answer empty and the upload skipped on every push.
fn check_precedes_every_upload(workflow: &Value, check: &Mapping) -> bool {
    reader::jobs(workflow).into_iter().all(|(_, job)| {
        let steps = reader::job_steps(job);
        let check_at = steps.iter().position(|step| std::ptr::eq(*step, check));
        steps
            .iter()
            .enumerate()
            .filter(|(_, step)| is_upload(step))
            .all(|(at, _)| check_at.is_some_and(|position| position < at))
    })
}

/// Returns whether a workflow or job sets a default shell for `run` steps.
fn sets_a_default_shell(scope: &Mapping) -> bool {
    get(scope, "defaults")
        .and_then(Value::as_mapping)
        .and_then(|defaults| get(defaults, "run"))
        .and_then(Value::as_mapping)
        .is_some_and(|run| get(run, "shell").is_some())
}

/// Returns whether the token check runs under an inherited default shell.
///
/// A workflow's or the check's own job's `defaults.run.shell` wraps the
/// command exactly as a step `shell` would, so it is refused for the same
/// reason; a default on another job does not reach the check.
fn check_inherits_a_shell(workflow: &Value, check: &Mapping) -> bool {
    workflow.as_mapping().is_some_and(sets_a_default_shell)
        || reader::jobs(workflow).into_iter().any(|(_, job)| {
            sets_a_default_shell(job)
                && reader::job_steps(job)
                    .iter()
                    .any(|step| std::ptr::eq(*step, check))
        })
}

/// Returns the reasons the token check step is missing or cannot be trusted.
///
/// The check must exist exactly once (deleted, the upload skips forever),
/// carry an id the upload can read, run with no `if:` and under no inherited
/// shell, come before every upload in the upload's own job, and declare
/// nothing beyond [`CHECK_STEP_KEYS`], since a check that cannot run, or runs
/// where the upload cannot read it, answers nothing.
pub(super) fn check_findings(workflow: &Value) -> Vec<String> {
    let checks = check_steps(workflow);
    let [check] = checks.as_slice() else {
        return vec![format!(
            "the publisher needs exactly one token check step running `{CHECK_COMMAND}`, found {}",
            checks.len()
        )];
    };
    let extra_keys = check
        .keys()
        .map(|key| key.as_str().unwrap_or("a non-string key"))
        .filter(|key| *key != "if" && !CHECK_STEP_KEYS.contains(key))
        .map(|key| format!("the token check step declares `{key}`"));
    [
        (
            get(check, "id").and_then(Value::as_str).is_none(),
            "has no id",
        ),
        (get(check, "if").is_some(), "carries an `if:`"),
        (
            check_inherits_a_shell(workflow, check),
            "runs under a `defaults.run.shell`",
        ),
        (
            !check_precedes_every_upload(workflow, check),
            "does not run before every upload in the upload's job",
        ),
    ]
    .into_iter()
    .filter(|(is_broken, _)| *is_broken)
    .map(|(_, reason)| format!("the token check step {reason}"))
    .chain(extra_keys)
    .collect()
}
