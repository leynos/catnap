//! The CV-005 token check step the publisher's upload is guarded on.
//!
//! Split from `publisher_rules.rs` under the repository's 400-line cap. The
//! upload action is composite and hands a step `env` to its nested steps, so
//! no step holds the token in its `env`: a check step reports whether the
//! secret is set, and the upload reads that answer.

use serde_norway::{Mapping, Value};

use super::reader::{self, get};

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

/// Returns the reasons the token check step is missing or cannot be trusted.
///
/// The check must exist exactly once (deleted, the upload skips forever),
/// carry an id the upload can read, and run with no `if:`, since a check that
/// cannot run answers nothing.
pub(super) fn check_findings(workflow: &Value) -> Vec<String> {
    let checks = check_steps(workflow);
    let [check] = checks.as_slice() else {
        return vec![format!(
            "the publisher needs exactly one token check step running `{CHECK_COMMAND}`, found {}",
            checks.len()
        )];
    };
    [
        (
            get(check, "id").and_then(Value::as_str).is_none(),
            "has no id",
        ),
        (get(check, "if").is_some(), "carries an `if:`"),
    ]
    .into_iter()
    .filter(|(is_broken, _)| *is_broken)
    .map(|(_, reason)| format!("the token check step {reason}"))
    .collect()
}
