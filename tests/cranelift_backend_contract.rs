//! Holds the Cranelift development default and component provisioning contract.

use anyhow::{Result, bail};
use rstest::rstest;
use serde_norway::Value;

const BACKEND_FLAG: &str = "-Zcodegen-backend=cranelift";
const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.cargo/config.toml"));
const TOOLCHAIN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rust-toolchain.toml"));
/// Reads each Cargo command in a public development action.
fn rustflags_commands(target: &str) -> Result<Vec<String>> {
    let manifest: Value = serde_norway::from_str(include_str!("../Netsukefile"))?;
    let action_name = if target == "lint" {
        "rust-lint"
    } else {
        target
    };
    let selected_action = manifest
        .get("actions")
        .and_then(Value::as_sequence)
        .and_then(|actions| {
            actions.iter().find(|candidate| {
                candidate.get("name").and_then(Value::as_str) == Some(action_name)
            })
        });
    let Some(command_value) = selected_action.and_then(|candidate| candidate.get("command")) else {
        bail!("Netsuke action {action_name} has no command");
    };
    let action_commands = match command_value {
        Value::String(single) => vec![single.clone()],
        Value::Sequence(list) => list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => bail!("Netsuke action {action_name} has invalid commands"),
    };
    let cargo_commands = action_commands
        .into_iter()
        .filter(|candidate_command| candidate_command.contains("cargo "))
        .collect::<Vec<_>>();
    if cargo_commands.is_empty() {
        bail!("Netsuke action {action_name} has no Cargo commands");
    }
    Ok(cargo_commands)
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
fn netsuke_development_commands_keep_the_backend(#[case] target: &str) {
    let commands = rustflags_commands(target).expect("read Netsuke commands");
    for command in commands {
        assert!(
            command.contains(BACKEND_FLAG),
            "Netsuke {target} drops Cranelift from a Cargo command: {command}"
        );
    }
}
