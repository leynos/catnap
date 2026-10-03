//! Gives Netsuke integration sandboxes a private index for tracked-YAML selection.
//!
//! This test-only helper stages placeholder files in a caller-owned temporary
//! workspace. Only the action and workflow-lint harnesses use it; it never
//! modifies the repository index or publishes Git data.

use std::{error::Error, io, path::Path, process::Command};

use cap_std::fs::Dir;

/// Stage the supplied YAML paths in the sandbox's own Git repository.
pub(crate) fn initialize(
    directory: &Dir,
    root: &Path,
    yaml_paths: &[String],
) -> Result<(), Box<dyn Error>> {
    for yaml_path in yaml_paths {
        if let Some(parent) = Path::new(yaml_path).parent() {
            directory.create_dir_all(parent)?;
        }
        directory.write(yaml_path, "")?;
    }
    for arguments in [["init", "--quiet"], ["add", "."]] {
        let output = Command::new("/usr/bin/git")
            .current_dir(root)
            .args(arguments)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "sandbox Git command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
            .into());
        }
    }
    Ok(())
}
