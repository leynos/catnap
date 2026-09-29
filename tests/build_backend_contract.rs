//! Holds `.cargo/config.toml` free of a codegen backend while the release
//! builds on stable.
//!
//! The release workflow builds with `cross +stable build --release`, and stable
//! Cargo reads `.cargo/config.toml` like any other Cargo. It refuses a
//! `codegen-backend` key there ("config profile `dev` is not valid") and stops,
//! so a backend selected in the configuration breaks every release build; one
//! broke the v0.1.0 release. The judgement is driven against fixtures first,
//! because a rule exercised only over this repository's own compliant files
//! would pass whether or not it detects anything, and then applied to the real
//! files.

use anyhow::{Context, Result, ensure};
use cap_std::{ambient_authority, fs_utf8::Dir};
use rstest::rstest;
use serde_norway::Value;

/// Returns whether any step of the workflow runs Cargo on the stable
/// toolchain, either as `cargo +stable` / `cross +stable` or by installing
/// `stable` through a toolchain action's `toolchain` input.
fn builds_on_stable(workflow: &Value) -> bool {
    steps(workflow).any(|step| {
        let runs_stable = step
            .get("run")
            .and_then(Value::as_str)
            .is_some_and(|run| run.contains("+stable"));
        let installs_stable = step
            .get("with")
            .and_then(|with| with.get("toolchain"))
            .and_then(Value::as_str)
            .is_some_and(|toolchain| toolchain.trim() == "stable");
        runs_stable || installs_stable
    })
}

/// Returns every step of every job in a workflow.
fn steps(workflow: &Value) -> impl Iterator<Item = &Value> {
    workflow
        .get("jobs")
        .and_then(Value::as_mapping)
        .into_iter()
        .flat_map(|jobs| jobs.values())
        .filter_map(|job| job.get("steps").and_then(Value::as_sequence))
        .flatten()
}

/// Returns the configuration keys that select or enable a codegen backend:
/// `[unstable] codegen-backend` and any profile's `codegen-backend`.
fn backend_keys(config: &toml::Table) -> Vec<String> {
    let unstable = config
        .get("unstable")
        .and_then(toml::Value::as_table)
        .filter(|table| table.contains_key("codegen-backend"))
        .map(|_| "unstable.codegen-backend".to_owned());
    let profiles = config
        .get("profile")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter(|(_, profile)| {
            profile
                .as_table()
                .is_some_and(|table| table.contains_key("codegen-backend"))
        })
        .map(|(name, _)| format!("profile.{name}.codegen-backend"));
    unstable.into_iter().chain(profiles).collect()
}

/// Returns the reasons the configuration would break a stable release build.
fn findings(config: &toml::Table, release: &Value) -> Vec<String> {
    if !builds_on_stable(release) {
        return Vec::new();
    }
    backend_keys(config)
        .into_iter()
        .map(|key| format!("`{key}` is set while the release builds on stable, which refuses it"))
        .collect()
}

/// A release workflow building on stable, as this repository's does.
const STABLE_RELEASE: &str =
    "jobs:\n  build:\n    steps:\n      - run: cross +stable build --release\n";
/// A release workflow building on the pinned nightly.
const NIGHTLY_RELEASE: &str =
    "jobs:\n  build:\n    steps:\n      - run: cross +nightly-2026-05-28 build --release\n";
/// A release workflow installing stable through a toolchain action.
const STABLE_TOOLCHAIN_ACTION: &str = "jobs:\n  build:\n    steps:\n      - uses: \
                                       actions-rust-lang/setup-rust-toolchain@abc\n        \
                                       with:\n          toolchain: stable\n      - run: cargo \
                                       build --release\n";
/// The configuration shape a Cranelift default takes.
const CRANELIFT: &str =
    "[unstable]\ncodegen-backend = true\n\n[profile.dev]\ncodegen-backend = \"cranelift\"\n";
/// A configuration with only a linker table.
const LINKER_ONLY: &str = "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n";

/// Scenario: a configuration with and without a backend key, against release
/// workflows that build on stable or on the pinned nightly.
///
/// Invariant: only a backend key beside a stable release build is reported,
/// once per key, so the rule is as narrow as it is sufficient.
#[rstest]
#[case::cranelift_with_stable_release(CRANELIFT, STABLE_RELEASE, 2)]
#[case::cranelift_with_stable_toolchain_action(CRANELIFT, STABLE_TOOLCHAIN_ACTION, 2)]
#[case::profile_key_alone("[profile.dev]\ncodegen-backend = \"cranelift\"\n", STABLE_RELEASE, 1)]
#[case::release_profile_key("[profile.release]\ncodegen-backend = \"llvm\"\n", STABLE_RELEASE, 1)]
#[case::linker_only_with_stable_release(LINKER_ONLY, STABLE_RELEASE, 0)]
#[case::cranelift_with_nightly_release(CRANELIFT, NIGHTLY_RELEASE, 0)]
fn a_backend_key_is_refused_only_beside_a_stable_release(
    #[case] config: &str,
    #[case] release: &str,
    #[case] expected: usize,
) -> Result<()> {
    let parsed_config: toml::Table = toml::from_str(config)?;
    let parsed_release: Value = serde_norway::from_str(release)?;
    let found = findings(&parsed_config, &parsed_release);
    ensure!(
        found.len() == expected,
        "expected {expected}, saw {found:?}"
    );
    Ok(())
}

/// Reads a repository file relative to the manifest directory.
fn read(path: &str) -> Result<String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())?;
    root.read_to_string(path)
        .with_context(|| format!("reading {path}"))
}

/// Scenario: this repository's own configuration and release workflow.
///
/// Invariant: the release builds on stable, so the configuration selects no
/// backend. The first check keeps the rule from passing vacuously should the
/// release move off stable without this contract being revisited.
#[test]
fn the_configuration_selects_no_backend_while_the_release_builds_on_stable() -> Result<()> {
    let config: toml::Table = toml::from_str(&read(".cargo/config.toml")?)?;
    let release: Value = serde_norway::from_str(&read(".github/workflows/release.yml")?)?;
    ensure!(
        builds_on_stable(&release),
        "the release no longer builds on stable; revisit this contract and the Cranelift \
         exception in docs/developers-guide.md"
    );
    let found = findings(&config, &release);
    ensure!(found.is_empty(), "{found:?}");
    Ok(())
}
