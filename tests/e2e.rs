//! End-to-end tests for the compiled `vsleep` binary.

use std::{path::PathBuf, process::Command as StdCommand};

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn compiled_binary_exposes_gnu_like_sleep_behaviour() -> Result<(), Box<dyn std::error::Error>> {
    let binary = build_vsleep_binary()?;

    Command::new(&binary)
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Usage: vsleep NUMBER[SUFFIX]...")
                .and(predicate::str::contains("--logical-second-ms").not()),
        );

    Command::new(&binary)
        .args(["--logical-second-ms", "5", "2"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("1 second remaining\n"));

    Command::new(&binary)
        .arg("1w")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "vsleep: invalid time suffix in '1w'",
        ));

    Ok(())
}

fn build_vsleep_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let output = StdCommand::new("cargo")
        .args(["build", "--bin", "vsleep"])
        .output()?;

    if output.status.success() {
        vsleep_binary()
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(std::io::Error::other(format!("cargo build failed: {stderr}")).into())
    }
}

fn vsleep_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let metadata_output = StdCommand::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()?;
    if !metadata_output.status.success() {
        let stderr = String::from_utf8_lossy(&metadata_output.stderr);
        return Err(std::io::Error::other(format!("cargo metadata failed: {stderr}")).into());
    }

    let metadata: serde_json::Value = serde_json::from_slice(&metadata_output.stdout)?;
    let target_directory = metadata
        .get("target_directory")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "cargo metadata omitted target_directory",
            )
        })?;
    Ok(PathBuf::from(target_directory)
        .join("debug")
        .join(format!("vsleep{}", std::env::consts::EXE_SUFFIX)))
}
