//! Gives Netsuke integration sandboxes a private index for tracked-YAML selection.
//!
//! This test-only helper stages placeholder files in a caller-owned temporary
//! workspace. Only the action and workflow-lint harnesses use it; it never
//! modifies the repository index or publishes Git data. Their Git and Netsuke
//! subprocesses share the same narrow environment isolation helper.

use std::{error::Error, io, path::Path, process::Command};

use cap_std::fs::Dir;

/// Keep fixture Git commands and Netsuke's Git queries in the caller-owned repository.
pub(crate) fn isolate(command: &mut Command) {
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
    ] {
        command.env_remove(variable);
    }
}

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
        let mut command = Command::new("/usr/bin/git");
        command.current_dir(root).args(arguments);
        isolate(&mut command);
        let output = command.output()?;
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

/// Command-scoped redirectors cannot make a fixture query another repository's index.
#[test]
fn git_redirectors_cannot_escape_the_fixture() {
    let owned = tempfile::tempdir().expect("create owned Git fixture directory");
    let foreign = tempfile::tempdir().expect("create foreign Git fixture directory");
    let owned_directory = Dir::open_ambient_dir(owned.path(), cap_std::ambient_authority())
        .expect("open owned Git fixture directory");
    let foreign_directory = Dir::open_ambient_dir(foreign.path(), cap_std::ambient_authority())
        .expect("open foreign Git fixture directory");
    initialize(&owned_directory, owned.path(), &["owned.yml".to_owned()])
        .expect("initialize and stage owned Git fixture");
    initialize(
        &foreign_directory,
        foreign.path(),
        &["foreign.yml".to_owned()],
    )
    .expect("initialize and stage foreign Git fixture");
    let foreign_git = foreign.path().join(".git");
    let mut command = Command::new("/usr/bin/git");
    command
        .current_dir(owned.path())
        .args(["ls-files"])
        .env("GIT_DIR", &foreign_git)
        .env("GIT_WORK_TREE", foreign.path())
        .env("GIT_INDEX_FILE", foreign_git.join("index"))
        .env("GIT_COMMON_DIR", &foreign_git)
        .env("GIT_OBJECT_DIRECTORY", foreign_git.join("objects"));
    isolate(&mut command);
    for variable in ["GIT_COMMON_DIR", "GIT_OBJECT_DIRECTORY"] {
        let removal = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new(variable));
        assert!(
            removal.is_some_and(|(_, value)| value.is_none()),
            "{variable} must be removed from the subprocess environment",
        );
    }
    let output = command.output().expect("query isolated Git fixture index");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "owned.yml\n");
}
