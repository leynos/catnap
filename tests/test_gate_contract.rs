//! Keeps doctests in the Netsuke test gate when CI uses cargo-nextest.

use std::{error::Error, io};

use serde_norway::Value;

/// Read the ordered commands from the public test action.
fn test_commands() -> Result<Vec<String>, Box<dyn Error>> {
    let manifest: Value = serde_norway::from_str(include_str!("../Netsukefile"))?;
    let commands = manifest
        .get("actions")
        .and_then(Value::as_sequence)
        .ok_or_else(|| io::Error::other("Netsukefile has no actions sequence"))?
        .iter()
        .find(|action| action.get("name").and_then(Value::as_str) == Some("test"))
        .and_then(|action| action.get("command"))
        .and_then(Value::as_sequence)
        .ok_or_else(|| io::Error::other("test action has no command list"))?
        .iter()
        .map(|command| {
            command
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| io::Error::other("test command is not a string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(commands)
}

/// Nextest must run first, then a guarded workspace doctest command.
#[test]
fn nextest_test_path_runs_workspace_doctests_after_the_suite() {
    let commands = test_commands().expect("read test action commands");
    let [suite, doctests] = commands.as_slice() else {
        panic!("test action must have two commands");
    };
    assert!(suite.contains("nextest run{% else %}test{% endif %}"));
    assert!(doctests.starts_with("if command -v cargo-nextest"));
    assert!(doctests.contains("cargo test --workspace --doc --all-features"));
}

/// The fallback uses Cargo's ordinary doctest run, without a second one.
#[test]
fn cargo_fallback_keeps_its_default_doctests_without_a_second_run() {
    let commands = test_commands().expect("read test action commands");
    let [suite, doctests] = commands.as_slice() else {
        panic!("test action must have two commands");
    };
    assert!(suite.contains("{% else %}test{% endif %}"));
    assert!(suite.contains("--all-targets --all-features"));
    assert!(doctests.contains("if command -v cargo-nextest"));
}
