//! Runs stable Cargo against this repository's `.cargo/config.toml`.
//!
//! `build_backend_contract.rs` reads the configuration and the release
//! workflow as text. This test asks the tool the release actually uses: stable
//! Cargo resolves every configured profile before it looks for the target it
//! was asked to build, and stops with "config profile `dev` is not valid" when
//! one selects a codegen backend. Asking it to build a binary that does not
//! exist therefore reaches that check and then fails on the missing target,
//! without compiling anything, so a refused configuration and an accepted one
//! differ in the message. That is the failure that broke the v0.1.0 release.

use std::process::Command;

use anyhow::{Context, Result, bail};
use rstest::rstest;

/// Stable Cargo's refusal of a configured profile.
const REFUSED: &str = "is not valid";
/// What stable Cargo reports once the configuration has loaded.
const ACCEPTED: &str = "no bin target named";

/// Judges the diagnostics stable Cargo printed for the probe build.
///
/// Only the missing-target message proves the configuration loaded. Any other
/// output, including a stable toolchain that is not installed, is reported
/// rather than read as a pass, so an environment that cannot run the probe
/// cannot certify the configuration.
fn judge(stderr: &str) -> Result<()> {
    if stderr.contains(REFUSED) {
        bail!("stable Cargo refused the repository configuration:\n{stderr}");
    }
    if !stderr.contains(ACCEPTED) {
        bail!(
            "the probe did not reach the target lookup, so the configuration was not judged (is \
             the stable toolchain installed? `rustup toolchain install stable --profile \
             minimal`):\n{stderr}"
        );
    }
    Ok(())
}

/// Scenario: the diagnostics stable Cargo prints for the probe in each state.
///
/// Invariant: only the missing-target message passes; a refused profile and
/// unrecognised output, such as a missing toolchain, both fail.
#[rstest]
#[case::configuration_loaded("error: no bin target named `no-such-bin`\n", true)]
#[case::profile_refused(
    concat!(
        "error: config profile `dev` is not valid (defined in `.cargo/config.toml`)\n\n",
        "Caused by:\n  feature `codegen-backend` is required\n"
    ),
    false
)]
#[case::toolchain_missing(
    "error: toolchain 'stable-x86_64-unknown-linux-gnu' is not installed\n",
    false
)]
#[case::no_output("", false)]
fn the_probe_output_is_judged_strictly(#[case] stderr: &str, #[case] passes: bool) {
    assert_eq!(judge(stderr).is_ok(), passes, "{stderr:?}");
}

/// Scenario: stable Cargo, run through `rustup` so the pinned nightly does not
/// answer for it, is asked to build a binary that does not exist.
///
/// Invariant: it reaches the target lookup, so it accepted the configuration.
#[test]
fn stable_cargo_accepts_the_repository_configuration() -> Result<()> {
    let output = Command::new("rustup")
        .args([
            "run",
            "stable",
            "cargo",
            "build",
            "--release",
            "--offline",
            "--bin",
            "no-such-bin",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        // The release assigns an empty `RUSTFLAGS`, which displaces the
        // configuration's nightly-only `-Zthreads` flag before stable rustc
        // sees it. The probe does the same: the `RUSTFLAGS` the make targets
        // export carry `-Zthreads`, which stable rustc refuses outright. Profile
        // validity, the subject here, does not depend on the flags.
        .env("RUSTFLAGS", "")
        .output()
        .context("running `rustup run stable cargo`")?;
    judge(&String::from_utf8_lossy(&output.stderr))
}
