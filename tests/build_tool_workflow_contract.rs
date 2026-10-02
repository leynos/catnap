//! Ensures every Linux Rust suite installs its pinned build tools first.

use rstest::rstest;
use serde_norway::Value;

#[path = "build_tool_workflow_contract/fixtures.rs"]
mod fixtures;
#[path = "build_tool_workflow_contract/support.rs"]
mod support;

use fixtures::{Mutation, mutate_valid_workflow, suite_job_mut, valid_workflow, yaml_value};
use support::{repository_root, run_installs_distro_mold, workflow_names, workflow_violations};

#[test]
fn every_reachable_linux_rust_suite_has_pinned_mold_setup() {
    let root = repository_root().expect("open repository");
    let workflow_names = workflow_names(&root).expect("enumerate workflows");
    let mut violations = Vec::new();

    assert!(!workflow_names.is_empty(), "workflow directory is empty");
    for name in workflow_names {
        let path = format!(".github/workflows/{name}");
        let text = root.read_to_string(&path).expect("read workflow");
        let workflow: Value = serde_norway::from_str(&text).expect("parse workflow YAML");
        violations.extend(workflow_violations(&workflow, &name).expect("classify runners"));
    }

    assert!(
        violations.is_empty(),
        "Linux Rust suite build-tool contract failed:\n{}",
        violations.join("\n")
    );
}

#[test]
fn ci_installs_rust_analyzer_before_running_the_lint_suite() {
    let root = repository_root().expect("open repository");
    let source = root
        .read_to_string(".github/workflows/ci.yml")
        .expect("read CI workflow");
    let workflow: Value = serde_norway::from_str(&source).expect("parse CI workflow");
    let steps = workflow
        .get("jobs")
        .and_then(|jobs| jobs.get("build-test"))
        .and_then(|job| job.get("steps"))
        .and_then(Value::as_sequence)
        .expect("CI build-test steps");
    let install_index = steps
        .iter()
        .position(|step| {
            step.get("run").and_then(Value::as_str) == Some("rustup component add rust-analyzer")
        })
        .expect("CI rust-analyzer installation step");
    let lint_index = steps
        .iter()
        .position(|step| {
            step.get("run")
                .and_then(Value::as_str)
                .is_some_and(|run| run.starts_with("/usr/bin/make ") && run.ends_with(" lint"))
        })
        .expect("CI lint step");
    let install_step = steps
        .get(install_index)
        .expect("rust-analyzer installation step index");

    assert!(
        install_index < lint_index,
        "rust-analyzer must be installed before lint"
    );
    assert!(
        install_step.get("if").is_none() && install_step.get("continue-on-error").is_none(),
        "rust-analyzer installation must be unconditional and fail the job"
    );
}

#[test]
fn ci_runs_ui_contract_tests_before_coverage() {
    let root = repository_root().expect("open repository");
    let source = root
        .read_to_string(".github/workflows/ci.yml")
        .expect("read CI workflow");
    let workflow: Value = serde_norway::from_str(&source).expect("parse CI workflow");
    let steps = workflow
        .get("jobs")
        .and_then(|jobs| jobs.get("build-test"))
        .and_then(|job| job.get("steps"))
        .and_then(Value::as_sequence)
        .expect("CI build-test steps");
    let runner_index = steps
        .iter()
        .position(|step| step.get("name").and_then(Value::as_str) == Some("Install test runner"))
        .expect("CI test runner installation step");
    let ui_index = steps
        .iter()
        .position(|step| step.get("run").and_then(Value::as_str) == Some("make test-ui"))
        .expect("CI UI test step");
    let coverage_index = steps
        .iter()
        .position(|step| {
            step.get("name").and_then(Value::as_str) == Some("Test and Measure Coverage")
        })
        .expect("CI coverage step");
    let ui_step = steps.get(ui_index).expect("UI test step index");

    assert!(
        runner_index < ui_index,
        "install nextest before the UI test target"
    );
    assert!(
        ui_index < coverage_index,
        "UI contracts must run before coverage"
    );
    assert!(
        ui_step.get("if").is_none() && ui_step.get("continue-on-error").is_none(),
        "UI contract tests must run unconditionally and fail the job"
    );
}

#[rstest]
#[case::new_linux_suite_without_setup(Mutation::MissingSetup, "new-suite")]
#[case::installer_after_suite(Mutation::SetupAfterSuite, "suite")]
#[case::conditional_installer(Mutation::ConditionalSetup, "suite")]
#[case::ignored_installer(Mutation::IgnoredSetup, "suite")]
#[case::unpinned_distro_mold(Mutation::DistroMold, "suite")]
#[case::missing_cranelift_installer(Mutation::MissingCraneliftSetup, "Cranelift")]
#[case::late_cranelift_installer(Mutation::CraneliftSetupAfterSuite, "Cranelift")]
#[case::conditional_cranelift_installer(Mutation::ConditionalCraneliftSetup, "Cranelift")]
#[case::ignored_cranelift_installer(Mutation::IgnoredCraneliftSetup, "Cranelift")]
fn suite_mutations_are_rejected(#[case] mutation: Mutation, #[case] expected_job: &str) {
    let workflow = mutate_valid_workflow(mutation).expect("mutate valid workflow");
    let errors = workflow_violations(&workflow, "mutation.yml").expect("classify fixture runner");

    assert!(
        errors.iter().any(|error| error.contains(expected_job)),
        "expected a contract failure for {expected_job}, got {errors:?}"
    );
}

#[rstest]
#[case::scalar("ubuntu-latest")]
#[case::label_list("[self-hosted, linux, x64]")]
#[case::label_group_mapping("{ group: shared-ci, labels: [self-hosted, linux] }")]
fn supported_linux_runner_forms_are_checked(#[case] runner: &str) {
    let mut workflow = valid_workflow().expect("parse valid workflow");
    suite_job_mut(&mut workflow)
        .expect("suite job mapping")
        .insert("runs-on".into(), yaml_value(runner).expect("parse runner"));

    assert!(
        workflow_violations(&workflow, "runner-form.yml")
            .expect("runner form is classifiable")
            .is_empty(),
        "a valid installer should satisfy the Linux runner form {runner}"
    );
}

#[test]
fn matrix_include_runner_forms_are_checked() {
    let mut workflow = valid_workflow().expect("parse valid workflow");
    let job = suite_job_mut(&mut workflow).expect("suite job mapping");
    job.insert(
        "runs-on".into(),
        Value::String("${{ matrix.runner }}".into()),
    );
    job.insert(
        "strategy".into(),
        yaml_value(concat!(
            "matrix:\n",
            "  include:\n",
            "    - runner: ubuntu-latest\n",
            "    - runner:\n",
            "        group: shared-ci\n",
            "        labels: [self-hosted, linux]\n",
            "    - runner: windows-latest\n",
        ))
        .expect("parse matrix fixture"),
    );

    assert!(
        workflow_violations(&workflow, "matrix-runner.yml")
            .expect("matrix include runners are classifiable")
            .is_empty()
    );
}

#[test]
fn unknown_suite_runner_fails_closed() {
    let mut workflow = valid_workflow().expect("parse valid workflow");
    suite_job_mut(&mut workflow)
        .expect("suite job mapping")
        .insert(
            "runs-on".into(),
            Value::String("${{ vars.RUNNER_LABEL }}".into()),
        );

    assert!(workflow_violations(&workflow, "unknown-runner.yml").is_err());
}

#[rstest]
#[case::apt("sudo apt install --yes mold")]
#[case::dnf("sudo dnf install --assumeyes mold=2.41.0")]
#[case::apk("sudo apk add --no-cache mold")]
#[case::zypper("sudo zypper install mold")]
fn common_linux_package_managers_are_detected(#[case] command: &str) {
    assert!(run_installs_distro_mold(command));
}
