//! Checks exact tracked-YAML selection in the workflow-lint sandbox.

use std::{error::Error, io, process::Command};

use super::repository_root;

/// The workflow linters run once each: yamllint strictly over the tracked
/// YAML, then actionlint.
pub(super) fn assert_workflow_linters(invocations: &[String]) {
    let [yamllint, actionlint] = invocations else {
        panic!("expected one yamllint and one actionlint run, got {invocations:?}");
    };
    assert_yamllint_covers_every_tracked_file(yamllint);
    assert_eq!(actionlint, "actionlint");
}

/// yamllint runs strictly, over exactly the Git-tracked YAML files.
pub(super) fn assert_yamllint_covers_every_tracked_file(invocation: &str) {
    let mut arguments = invocation.split('\t');
    assert_eq!(arguments.next(), Some("yamllint"), "{invocation}");
    assert_eq!(arguments.next(), Some("--strict"), "{invocation}");
    assert_eq!(arguments.next(), Some("--"), "{invocation}");
    let mut given: Vec<&str> = arguments.collect();
    given.sort_unstable();
    let Ok(mut tracked) = tracked_yaml_files() else {
        panic!("could not list the tracked YAML files");
    };
    tracked.sort_unstable();
    assert_eq!(
        given, tracked,
        "yamllint was not given the tracked YAML files"
    );
}

/// Read the repository index's YAML path list for the isolated Git fixture.
pub(super) fn tracked_yaml_files() -> Result<Vec<String>, Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(repository_root())
        .args(["ls-files", "-z", "*.yml", "*.yaml"])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("git ls-files failed").into());
    }
    Ok(String::from_utf8(output.stdout)?
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}
