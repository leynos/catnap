//! Holds the release matrix's legs: each target occurs once, on its expected
//! builder and runner.
//!
//! Split from `release_workflow.rs` to keep both files within the size limit.
//! The rule under test is `matrix_problems` in the parent module; these cases
//! move a leg to the wrong builder or runner, and drop or duplicate a leg, and
//! assert the clause meant to catch it does.

use rstest::rstest;
use serde_norway::Value;

use super::{assert_reports, violations_after};

/// Sets one field of the matrix leg for `target`.
fn set_leg(workflow: &mut Value, target: &str, key: &str, value: &str) -> Result<(), String> {
    workflow
        .get_mut("jobs")
        .and_then(|jobs| {
            jobs.get_mut("build")?
                .get_mut("strategy")?
                .get_mut("matrix")?
                .get_mut("include")
        })
        .and_then(Value::as_sequence_mut)
        .and_then(|legs| {
            legs.iter_mut()
                .find(|leg| leg.get("target").and_then(Value::as_str) == Some(target))
        })
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| format!("no matrix leg for {target}"))?
        .insert(key.into(), value.into());
    Ok(())
}

/// Applies an edit to the matrix's leg list.
fn edit_legs(workflow: &mut Value, edit: impl FnOnce(&mut Vec<Value>)) -> Result<(), String> {
    let legs = workflow
        .get_mut("jobs")
        .and_then(|jobs| {
            jobs.get_mut("build")?
                .get_mut("strategy")?
                .get_mut("matrix")?
                .get_mut("include")
        })
        .and_then(Value::as_sequence_mut)
        .ok_or("no matrix include list")?;
    edit(legs);
    Ok(())
}

#[rstest]
#[case::apple_back_on_cross("x86_64-apple-darwin", "builder", "cross")]
#[case::arm_apple_back_on_cross("aarch64-apple-darwin", "builder", "cross")]
#[case::apple_on_a_linux_runner("aarch64-apple-darwin", "runner", "ubuntu-latest")]
#[case::linux_on_native_cargo("x86_64-unknown-linux-gnu", "builder", "cargo")]
#[case::windows_on_a_mac_runner("x86_64-pc-windows-gnu", "runner", "macos-latest")]
fn a_leg_on_the_wrong_builder_or_runner_is_refused(
    #[case] target: &str,
    #[case] key: &str,
    #[case] value: &str,
) {
    let found = violations_after(|workflow| set_leg(workflow, target, key, value))
        .expect("the workflow should be readable");
    assert_reports(&found, target);
}

#[rstest]
#[case::linux_x86_64("x86_64-unknown-linux-gnu")]
#[case::linux_aarch64("aarch64-unknown-linux-gnu")]
#[case::windows("x86_64-pc-windows-gnu")]
#[case::freebsd("x86_64-unknown-freebsd")]
#[case::apple("aarch64-apple-darwin")]
fn a_dropped_or_duplicated_leg_is_refused(#[case] target: &str) {
    let dropped = violations_after(|workflow| {
        edit_legs(workflow, |legs| {
            legs.retain(|leg| leg.get("target").and_then(Value::as_str) != Some(target));
        })
    })
    .expect("the workflow should be readable");
    assert_reports(&dropped, target);
    let duplicated = violations_after(|workflow| {
        edit_legs(workflow, |legs| {
            let copy = legs
                .iter()
                .find(|leg| leg.get("target").and_then(Value::as_str) == Some(target))
                .cloned();
            legs.extend(copy);
        })
    })
    .expect("the workflow should be readable");
    assert_reports(&duplicated, "exactly");
    assert_reports(&duplicated, target);
}
