//! Holds the disabled Cranelift default, its opt-in and component provisioning contract.

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

/// Returns the configuration's lines with comments removed.
fn active_config_lines() -> Vec<&'static str> {
    CONFIG
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Cranelift is disabled by default: no active source selects it, the commented
/// opt-in stays beside each `rustflags` source, and the toolchain keeps the
/// component. Uncommenting the selection is a persistent opt-in that needs this
/// contract updated.
#[test]
fn cargo_defaults_leave_the_backend_on_llvm_and_keep_the_opt_in() {
    assert!(
        active_config_lines()
            .iter()
            .all(|line| !line.contains(BACKEND_FLAG) && !line.contains("codegen-backend")),
        "no active Cargo configuration line may select a codegen backend"
    );
    assert_eq!(
        CONFIG
            .lines()
            .filter(
                |line| line.trim_start().starts_with("# rustflags") && line.contains(BACKEND_FLAG)
            )
            .count(),
        2,
        "the general and Linux rustflags sources must keep their commented opt-in"
    );
    assert!(
        TOOLCHAIN.contains("\"rustc-codegen-cranelift-preview\""),
        "the pinned toolchain must keep the Cranelift component"
    );
}

/// The Netsukefile must not define the opt-in variable that feeds the Cranelift
/// flag into the development actions; its commented `vars` block is the opt-in.
#[test]
fn netsuke_manifest_leaves_the_opt_in_variable_undefined() {
    let manifest = include_str!("../Netsukefile");
    let active_definitions = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter(|line| line.trim_start().starts_with("cranelift_flag:"))
        .count();
    assert_eq!(
        active_definitions, 0,
        "an active `cranelift_flag` definition enables Cranelift in every development action"
    );
    assert!(
        manifest.contains("#   cranelift_flag: \" -Zcodegen-backend=cranelift\""),
        "the commented opt-in block must stay in the Netsukefile"
    );
}

#[rstest]
#[case("build")]
#[case("test")]
#[case("lint")]
#[case("typecheck")]
fn netsuke_development_commands_leave_the_backend_to_the_opt_in_variable(#[case] target: &str) {
    let commands = rustflags_commands(target).expect("read Netsuke commands");
    for command in commands {
        assert!(
            !command.contains(BACKEND_FLAG),
            "Netsuke {target} hard-codes Cranelift in a Cargo command: {command}"
        );
        assert!(
            command.contains("{{ cranelift_flag | default(\"\") }}"),
            "Netsuke {target} drops the Cranelift opt-in variable from a Cargo command: {command}"
        );
    }
}
