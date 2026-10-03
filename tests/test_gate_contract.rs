//! Keeps doctests in the test gate when CI uses cargo-nextest.

use std::process::Command;

use anyhow::{Context, Result, ensure};

#[test]
fn nextest_test_path_runs_workspace_doctests_after_the_suite() -> Result<()> {
    let stdout = make_test_output("nextest run")?;
    let nextest_position = stdout
        .find("cargo nextest run")
        .context("Make output should include the nextest run")?;
    let doctest_position = stdout
        .find("cargo test --workspace --doc --all-features")
        .context("Make output should include workspace doctests")?;
    ensure!(
        nextest_position < doctest_position,
        "workspace doctests must follow nextest: {stdout}"
    );
    Ok(())
}

#[test]
fn cargo_fallback_keeps_its_default_doctests_without_a_second_run() -> Result<()> {
    let stdout = make_test_output("test")?;
    ensure!(
        stdout.contains("cargo test --all-targets --all-features"),
        "Make output should include the Cargo fallback: {stdout}"
    );
    ensure!(
        !stdout.contains("--doc"),
        "the Cargo fallback should not schedule doctests twice: {stdout}"
    );
    Ok(())
}

fn make_test_output(test_command: &str) -> Result<String> {
    let command = format!("TEST_CMD={test_command}");
    let output = Command::new("make")
        .args(["-n", "-B", "test"])
        .arg(command)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .context("running `make -n -B test`")?;
    ensure!(output.status.success(), "`make -n -B test` failed");
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
