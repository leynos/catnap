//! Holds the Cranelift development default and component provisioning contract.

use std::process::Command;

use anyhow::{Context, Result, bail};
use rstest::rstest;

const BACKEND_FLAG: &str = "-Zcodegen-backend=cranelift";
const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.cargo/config.toml"));
const TOOLCHAIN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rust-toolchain.toml"));
/// Runs `make -n` and returns every command assigning `RUSTFLAGS`.
fn rustflags_commands(target: &str) -> Result<Vec<String>> {
    let output = Command::new("make")
        .args(["-n", "-B", target])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .with_context(|| format!("running `make -n -B {target}`"))?;
    if !output.status.success() {
        bail!(
            "`make -n -B {target}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let flattened = String::from_utf8_lossy(&output.stdout).replace("\\\n", " ");
    let commands = flattened
        .lines()
        .filter(|line| line.contains("RUSTFLAGS=") && line.contains("cargo "))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if commands.is_empty() {
        bail!("`make -n -B {target}` printed no Cargo RUSTFLAGS assignments");
    }
    Ok(commands)
}

#[test]
fn cargo_defaults_and_toolchain_pin_select_cranelift() {
    assert_eq!(
        CONFIG.matches(BACKEND_FLAG).count(),
        2,
        "the general and Linux rustflags sources must both select Cranelift"
    );
    assert!(
        TOOLCHAIN.contains("\"rustc-codegen-cranelift-preview\""),
        "the pinned toolchain must install Cranelift"
    );
}

#[rstest]
#[case("build")]
#[case("test")]
#[case("lint")]
#[case("typecheck")]
fn make_development_commands_keep_the_backend(#[case] target: &str) {
    let commands = rustflags_commands(target).expect("read Make commands");
    for command in commands {
        assert!(
            command.contains(BACKEND_FLAG),
            "`make {target}` drops Cranelift from a Cargo command: {command}"
        );
    }
}
