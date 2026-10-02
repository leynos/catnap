//! Builds valid and deliberately broken workflows for contract tests.

use serde_norway::{Mapping, Value};

const VALID_WORKFLOW: &str = r"jobs:
  suite:
    runs-on: ubuntu-latest
    steps:
      - uses: leynos/shared-actions/.github/actions/setup-rust@6cec89bac47a21cf756d68d638a9a510998e57f8
        with:
          install-mold: true
      - run: make install-cranelift
      - run: cargo test
";

#[derive(Clone, Copy)]
pub(super) enum Mutation {
    MissingSetup,
    SetupAfterSuite,
    ConditionalSetup,
    IgnoredSetup,
    DistroMold,
    MissingCraneliftSetup,
    CraneliftSetupAfterSuite,
    ConditionalCraneliftSetup,
    IgnoredCraneliftSetup,
}

pub(super) fn valid_workflow() -> Result<Value, String> {
    serde_norway::from_str(VALID_WORKFLOW).map_err(|error| error.to_string())
}

pub(super) fn yaml_value(yaml: &str) -> Result<Value, String> {
    serde_norway::from_str(yaml).map_err(|error| error.to_string())
}

pub(super) fn suite_job_mut(workflow: &mut Value) -> Result<&mut Mapping, String> {
    workflow
        .get_mut("jobs")
        .and_then(|jobs| jobs.get_mut("suite"))
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| "valid workflow has no suite job mapping".to_owned())
}

fn suite_steps_mut(workflow: &mut Value) -> Result<&mut Vec<Value>, String> {
    let suite_job = suite_job_mut(workflow)?;
    suite_job
        .get_mut("steps")
        .and_then(Value::as_sequence_mut)
        .ok_or_else(|| "suite job has no steps sequence".to_owned())
}

pub(super) fn mutate_valid_workflow(mutation: Mutation) -> Result<Value, String> {
    let mut workflow = valid_workflow()?;
    match mutation {
        Mutation::MissingSetup => add_suite_without_setup(&mut workflow)?,
        Mutation::SetupAfterSuite => move_setup_after_suite(&mut workflow)?,
        Mutation::ConditionalSetup => make_setup_conditional(&mut workflow)?,
        Mutation::IgnoredSetup => ignore_setup_failure(&mut workflow)?,
        Mutation::DistroMold => add_distro_mold_install(&mut workflow)?,
        Mutation::MissingCraneliftSetup => remove_cranelift_setup(&mut workflow)?,
        Mutation::CraneliftSetupAfterSuite => move_cranelift_setup_after_suite(&mut workflow)?,
        Mutation::ConditionalCraneliftSetup => make_cranelift_setup_conditional(&mut workflow)?,
        Mutation::IgnoredCraneliftSetup => ignore_cranelift_setup_failure(&mut workflow)?,
    }
    Ok(workflow)
}

fn add_suite_without_setup(workflow: &mut Value) -> Result<(), String> {
    let suite = yaml_value("runs-on: ubuntu-latest\nsteps:\n  - run: make test")?;
    workflow
        .get_mut("jobs")
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| "valid workflow has no jobs mapping".to_owned())?
        .insert("new-suite".into(), suite);
    Ok(())
}

fn move_setup_after_suite(workflow: &mut Value) -> Result<(), String> {
    let steps = suite_steps_mut(workflow)?;
    if steps.len() < 3 {
        return Err("valid workflow needs mold setup, Cranelift setup, and suite steps".to_owned());
    }
    steps.swap(0, 2);
    Ok(())
}

fn remove_cranelift_setup(workflow: &mut Value) -> Result<(), String> {
    let steps = suite_steps_mut(workflow)?;
    if steps.len() < 3 {
        return Err("valid workflow needs a Cranelift setup and suite step".to_owned());
    }
    steps.remove(1);
    Ok(())
}

fn move_cranelift_setup_after_suite(workflow: &mut Value) -> Result<(), String> {
    let steps = suite_steps_mut(workflow)?;
    if steps.len() < 3 {
        return Err("valid workflow needs a Cranelift setup and suite step".to_owned());
    }
    steps.swap(1, 2);
    Ok(())
}

fn add_cranelift_setup_field(
    workflow: &mut Value,
    field: &str,
    value: Value,
) -> Result<(), String> {
    let steps = suite_steps_mut(workflow)?;
    let setup = steps
        .get_mut(1)
        .ok_or_else(|| "valid workflow has no Cranelift setup step".to_owned())?;
    setup
        .as_mapping_mut()
        .ok_or_else(|| "Cranelift setup is not a mapping".to_owned())?
        .insert(field.into(), value);
    Ok(())
}

fn add_setup_field(workflow: &mut Value, field: &str, value: Value) -> Result<(), String> {
    let setup_step = suite_steps_mut(workflow)?
        .first_mut()
        .ok_or_else(|| "valid workflow has no setup step".to_owned())?;
    setup_step
        .as_mapping_mut()
        .ok_or_else(|| "setup step is not a mapping".to_owned())?
        .insert(field.into(), value);
    Ok(())
}

fn make_setup_conditional(workflow: &mut Value) -> Result<(), String> {
    add_setup_field(workflow, "if", Value::String("runner.os == 'Linux'".into()))
}

fn ignore_setup_failure(workflow: &mut Value) -> Result<(), String> {
    add_setup_field(workflow, "continue-on-error", Value::Bool(true))
}

fn make_cranelift_setup_conditional(workflow: &mut Value) -> Result<(), String> {
    add_cranelift_setup_field(workflow, "if", Value::String("runner.os == 'Linux'".into()))
}

fn ignore_cranelift_setup_failure(workflow: &mut Value) -> Result<(), String> {
    add_cranelift_setup_field(workflow, "continue-on-error", Value::Bool(true))
}

fn add_distro_mold_install(workflow: &mut Value) -> Result<(), String> {
    let steps = suite_steps_mut(workflow)?;
    if steps.is_empty() {
        return Err("valid workflow has no steps".to_owned());
    }
    steps.insert(
        1,
        yaml_value("run: sudo apt-get install --yes --no-install-recommends mold")?,
    );
    Ok(())
}
