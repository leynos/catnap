//! Holds the release workflow's shape: every leg builds, macOS builds natively,
//! and a dispatch never publishes by accident.
//!
//! `cross` has no Docker image for Apple targets, so the two macOS legs of
//! v0.1.0 failed on a Linux runner: they now build natively, the other four keep
//! `cross`, and the `x86_64` Linux leg overrides `.cargo/config.toml`'s clang
//! linker, which the cross image lacks. A failed leg must not cancel the rest,
//! and only a tag push, or an explicit `dry-run: false` dispatch on a tag ref,
//! publishes. Each test mutates a copy of the workflow and asserts the clause
//! meant to catch it does.

use cap_std::{ambient_authority, fs_utf8::Dir};
use rstest::rstest;
use serde_norway::{Mapping, Value};

/// Every release leg: its target, its builder and the runner it builds on.
const MATRIX: [(&str, &str, &str); 6] = [
    ("x86_64-unknown-linux-gnu", "cross", "ubuntu-latest"),
    ("aarch64-unknown-linux-gnu", "cross", "ubuntu-latest"),
    ("x86_64-pc-windows-gnu", "cross", "ubuntu-latest"),
    ("x86_64-unknown-freebsd", "cross", "ubuntu-latest"),
    ("x86_64-apple-darwin", "cargo", "macos-15-intel"),
    ("aarch64-apple-darwin", "cargo", "macos-latest"),
];
const LINKER_VARIABLE: &str = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER";
const NATIVE_BUILD: &str = "cargo +stable build --release --target ${{ matrix.target }}";
const CROSS_BUILD: &str = "cross +stable build --release --target ${{ matrix.target }}";
const PUBLISH_IF: &str = concat!(
    "github.event_name == 'push' || (github.event_name == 'workflow_dispatch' && ",
    "inputs.dry-run == false && startsWith(github.ref, 'refs/tags/'))"
);

/// Reads the release workflow.
fn read_release() -> Result<Value, String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())
        .map_err(|error| error.to_string())?;
    let text = root
        .read_to_string(".github/workflows/release.yml")
        .map_err(|error| error.to_string())?;
    serde_norway::from_str(&text).map_err(|error| error.to_string())
}

/// Returns the build matrix's legs.
fn legs(workflow: &Value) -> Vec<&Mapping> {
    workflow
        .get("jobs")
        .and_then(|jobs| {
            jobs.get("build")?
                .get("strategy")?
                .get("matrix")?
                .get("include")
        })
        .and_then(Value::as_sequence)
        .map(|legs| legs.iter().filter_map(Value::as_mapping).collect())
        .unwrap_or_default()
}

/// Returns a mapping's string value for a key.
fn text<'a>(map: &'a Mapping, key: &str) -> Option<&'a str> { map.get(key)?.as_str() }

/// Collapses runs of whitespace so a folded scalar compares to one line.
fn squeeze(value: &str) -> String { value.split_whitespace().collect::<Vec<_>>().join(" ") }

/// Returns whether a step is the build step guarded for `builder`.
fn is_build_step(step: &Value, builder: &str) -> bool {
    step.get("if").and_then(Value::as_str)
        == Some(format!("matrix.builder == '{builder}'").as_str())
        && step
            .get("run")
            .and_then(Value::as_str)
            .is_some_and(|run| run.contains("build --release"))
}

/// Returns the build job's step guarded for `builder`.
fn build_step<'a>(workflow: &'a Value, builder: &str) -> Option<&'a Value> {
    workflow
        .get("jobs")?
        .get("build")?
        .get("steps")?
        .as_sequence()?
        .iter()
        .find(|step| is_build_step(step, builder))
}

/// Returns the build job's step guarded for `builder`, to edit.
fn build_step_mut<'a>(workflow: &'a mut Value, builder: &str) -> Option<&'a mut Mapping> {
    workflow
        .get_mut("jobs")?
        .get_mut("build")?
        .get_mut("steps")?
        .as_sequence_mut()?
        .iter_mut()
        .find(|step| is_build_step(step, builder))?
        .as_mapping_mut()
}

/// Returns the run text of the build step guarded for `builder`, squeezed.
fn build_run(workflow: &Value, builder: &str) -> Option<String> {
    Some(squeeze(
        build_step(workflow, builder)?.get("run")?.as_str()?,
    ))
}

/// Lists the reasons the matrix is not exactly the six expected legs.
///
/// Each target must occur once, on its expected builder and runner, so a leg
/// dropped, duplicated or moved back to `cross` on a Linux runner (the Apple
/// legs) is refused.
fn matrix_problems(workflow: &Value) -> Vec<String> {
    let found = legs(workflow);
    let mut problems: Vec<String> = MATRIX
        .into_iter()
        .filter(|(target, builder, runner)| {
            let matching: Vec<_> = found
                .iter()
                .filter(|leg| text(leg, "target") == Some(*target))
                .collect();
            let is_expected = matching.len() == 1
                && matching.iter().all(|leg| {
                    text(leg, "builder") == Some(*builder) && text(leg, "runner") == Some(*runner)
                });
            !is_expected
        })
        .map(|(target, builder, runner)| {
            format!("{target} must appear once, built by {builder} on {runner}")
        })
        .collect();
    if found.len() != MATRIX.len() {
        problems.push(format!(
            "the matrix must hold exactly {} legs",
            MATRIX.len()
        ));
    }
    problems
}

/// Lists the reasons the dispatch or publish job could publish by accident.
fn publish_problems(workflow: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let dry_run = workflow
        .get("on")
        .or_else(|| workflow.get(Value::Bool(true)))
        .and_then(|on| on.get("workflow_dispatch")?.get("inputs")?.get("dry-run"));
    let is_default_dry = dry_run.is_some_and(|input| {
        input.get("type").and_then(Value::as_str) == Some("boolean")
            && input.get("default") == Some(&Value::Bool(true))
    });
    if !is_default_dry {
        problems.push("dispatch needs a boolean `dry-run` input defaulting to true".to_owned());
    }
    let publish_if = workflow
        .get("jobs")
        .and_then(|jobs| jobs.get("release")?.get("if")?.as_str())
        .map(squeeze);
    if publish_if.as_deref() != Some(PUBLISH_IF) {
        problems
            .push("the release job must publish only for a tag push or a real dispatch".to_owned());
    }
    problems
}

/// Lists every way the workflow breaks the release shape.
fn violations(workflow: &Value) -> Vec<String> {
    let mut problems = matrix_problems(workflow);
    problems.extend(publish_problems(workflow));
    if build_run(workflow, "cargo").as_deref() != Some(NATIVE_BUILD) {
        problems.push("the native step must run `cargo +stable build --release`".to_owned());
    }
    if build_run(workflow, "cross").as_deref() != Some(CROSS_BUILD) {
        problems.push("the cross step must run `cross +stable build --release`".to_owned());
    }
    let linker = build_step(workflow, "cross")
        .and_then(|step| step.get("env")?.get(LINKER_VARIABLE)?.as_str());
    if linker != Some("cc") {
        problems.push(format!("the cross step must set {LINKER_VARIABLE}=cc"));
    }
    let fail_fast = workflow
        .get("jobs")
        .and_then(|jobs| jobs.get("build")?.get("strategy")?.get("fail-fast"));
    if fail_fast != Some(&Value::Bool(false)) {
        problems.push("fail-fast must be false so one leg cannot cancel the rest".to_owned());
    }
    problems
}

/// Returns the violations after applying one mutation to a fresh copy.
fn violations_after(
    mutation: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    let mut workflow = read_release()?;
    mutation(&mut workflow)?;
    Ok(violations(&workflow))
}

/// Fails unless some violation contains `fragment`.
fn assert_reports(found: &[String], fragment: &str) {
    assert!(
        found.iter().any(|problem| problem.contains(fragment)),
        "expected a violation naming {fragment:?}, got {found:?}"
    );
}

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

/// Sets one field of a top-level job.
fn set_job(workflow: &mut Value, job: &str, key: &str, value: Value) -> Result<(), String> {
    workflow
        .get_mut("jobs")
        .and_then(|jobs| jobs.get_mut(job)?.as_mapping_mut())
        .ok_or_else(|| format!("no {job} job"))?
        .insert(key.into(), value);
    Ok(())
}

/// Rewrites the run text of the build step guarded for `builder`.
fn set_build_run(workflow: &mut Value, builder: &str, run: &str) -> Result<(), String> {
    build_step_mut(workflow, builder)
        .ok_or_else(|| format!("no build step for {builder}"))?
        .insert("run".into(), run.into());
    Ok(())
}

/// Sets, or with `None` removes, the linker variable on the cross build step.
fn set_linker(workflow: &mut Value, linker: Option<&str>) -> Result<(), String> {
    let env = build_step_mut(workflow, "cross")
        .and_then(|step| step.get_mut("env")?.as_mapping_mut())
        .ok_or("no cross build env")?;
    match linker {
        Some(value) => env.insert(LINKER_VARIABLE.into(), value.into()),
        None => env.remove(LINKER_VARIABLE),
    };
    Ok(())
}

#[test]
fn repository_release_holds_the_shape() {
    let found = violations_after(|_| Ok(())).expect("the workflow should be readable");
    assert!(found.is_empty(), "expected no violations, got {found:?}");
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
}

#[rstest]
#[case::native_uses_cross("cargo", "cross +stable build --release --target ${{ matrix.target }}")]
#[case::cross_uses_cargo("cross", "cargo +stable build --release --target ${{ matrix.target }}")]
#[case::native_loses_stable("cargo", "cargo build --release --target ${{ matrix.target }}")]
fn a_build_step_running_the_wrong_tool_is_refused(#[case] builder: &str, #[case] run: &str) {
    let found = violations_after(|workflow| set_build_run(workflow, builder, run))
        .expect("the workflow should be readable");
    assert_reports(&found, builder);
}

#[rstest]
#[case::absent(None)]
#[case::explicitly_true(Some(true))]
fn a_failing_leg_may_not_cancel_the_rest(#[case] fail_fast: Option<bool>) {
    let found = violations_after(|workflow| {
        let strategy = workflow
            .get_mut("jobs")
            .and_then(|jobs| jobs.get_mut("build")?.get_mut("strategy")?.as_mapping_mut())
            .ok_or("no build strategy")?;
        match fail_fast {
            Some(value) => strategy.insert("fail-fast".into(), Value::Bool(value)),
            None => strategy.remove("fail-fast"),
        };
        Ok(())
    })
    .expect("the workflow should be readable");
    assert_reports(&found, "fail-fast");
}

#[rstest]
#[case::no_input(None, None)]
#[case::defaults_to_publishing(Some("boolean"), Some(false))]
#[case::not_a_boolean(Some("string"), Some(true))]
fn a_dispatch_that_is_not_a_dry_run_by_default_is_refused(
    #[case] kind: Option<&str>,
    #[case] default: Option<bool>,
) {
    let found = violations_after(|workflow| {
        let dispatch = workflow
            .get_mut("on")
            .and_then(|on| on.get_mut("workflow_dispatch")?.as_mapping_mut())
            .ok_or("no workflow_dispatch trigger")?;
        let mut input = Mapping::new();
        if let Some(text) = kind {
            input.insert("type".into(), text.into());
        }
        if let Some(value) = default {
            input.insert("default".into(), Value::Bool(value));
        }
        let mut inputs = Mapping::new();
        if kind.is_some() {
            inputs.insert("dry-run".into(), Value::Mapping(input));
        }
        dispatch.insert("inputs".into(), Value::Mapping(inputs));
        Ok(())
    })
    .expect("the workflow should be readable");
    assert_reports(&found, "dry-run");
}

#[rstest]
#[case::any_dispatch("github.event_name == 'push' || github.event_name == 'workflow_dispatch'")]
#[case::no_tag_check(
    "github.event_name == 'push' || (github.event_name == 'workflow_dispatch' && inputs.dry-run \
     == false)"
)]
#[case::unconditional("true")]
fn a_publish_job_that_can_run_on_a_branch_dispatch_is_refused(#[case] condition: &str) {
    let found = violations_after(|workflow| set_job(workflow, "release", "if", condition.into()))
        .expect("the workflow should be readable");
    assert_reports(&found, "publish");
}

#[rstest]
#[case::clang(Some("clang"))]
#[case::empty(Some(""))]
#[case::dropped(None)]
fn a_cross_linux_leg_linked_by_a_missing_clang_is_refused(#[case] linker: Option<&str>) {
    let found = violations_after(|workflow| set_linker(workflow, linker))
        .expect("the workflow should be readable");
    assert_reports(&found, LINKER_VARIABLE);
}
