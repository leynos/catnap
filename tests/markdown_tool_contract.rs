//! Checks the pinned Markdown lint tool and Git-aware formatter selection.

use std::{
    error::Error,
    io,
    process::{Command, Output},
};

use cap_std::{ambient_authority, fs::Dir};
use tempfile::TempDir;

const MARKDOWNLINT_CLI2_VERSION: &str = "0.23.3";

#[test]
fn netsuke_uses_the_cli_version_bundled_by_the_ci_action() {
    let manifest = repository_directory()
        .expect("open repository root")
        .read_to_string("Netsukefile")
        .expect("read Netsukefile");
    let version_pin = format!("markdownlint-cli2@{MARKDOWNLINT_CLI2_VERSION}");

    assert!(manifest.contains(&format!("bunx --silent {version_pin}")));
    assert!(manifest.contains(&format!("{version_pin} --fix \"**/*.md\"")));
    assert!(manifest.contains(&format!("xargs -0 bunx --silent {version_pin}")));
    assert!(manifest.contains("mdtablefix --check --git --include-untracked"));
    assert!(manifest.contains("mdtablefix --in-place --git --include-untracked"));
}

#[test]
fn formatter_selects_tracked_and_untracked_but_not_ignored_markdown() {
    let temporary_directory = tempfile::tempdir().expect("create Git fixture");
    let directory = Dir::open_ambient_dir(temporary_directory.path(), ambient_authority())
        .expect("open Git fixture");
    directory
        .write(".gitignore", "target/\n")
        .expect("write ignore rules");
    directory
        .write("tracked.md", "# Tracked\n")
        .expect("write tracked Markdown");
    directory
        .create_dir("target")
        .expect("create target directory");
    directory
        .write("target/generated.md", "# Generated\n")
        .expect("write ignored Markdown");
    initialize_fixture_repository(&temporary_directory).expect("initialize Git fixture");
    directory
        .write("new.md", "# New\n")
        .expect("write untracked Markdown");

    let output = run_mdtablefix_selection(&temporary_directory).expect("run mdtablefix");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "mdtablefix failed; stdout={stdout}; stderr={stderr}"
    );
    assert!(stdout.lines().any(|path| path == "tracked.md"));
    assert!(stdout.lines().any(|path| path == "new.md"));
    assert!(!stdout.lines().any(|path| path == "target/generated.md"));
}

fn initialize_fixture_repository(directory: &TempDir) -> Result<(), Box<dyn Error>> {
    run_git(directory, &["init", "--quiet"])?;
    run_git(directory, &["config", "user.email", "test@example.invalid"])?;
    run_git(directory, &["config", "user.name", "Markdown contract"])?;
    run_git(directory, &["add", ".gitignore", "tracked.md"])?;
    run_git(directory, &["commit", "--quiet", "-m", "fixture baseline"])
}

fn run_git(directory: &TempDir, arguments: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(directory.path())
        .args(arguments)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into())
    }
}

fn run_mdtablefix_selection(directory: &TempDir) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new("mdtablefix")
        .current_dir(directory.path())
        .args([
            "--list-files",
            "--git",
            "--include-untracked",
            "--wrap",
            "--renumber",
            "--breaks",
            "--ellipsis",
            "--fences",
        ])
        .output()?)
}

fn repository_directory() -> Result<Dir, Box<dyn Error>> {
    Ok(Dir::open_ambient_dir(
        env!("CARGO_MANIFEST_DIR"),
        ambient_authority(),
    )?)
}
