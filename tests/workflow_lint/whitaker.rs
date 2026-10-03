//! Consumer contracts for the pinned Whitaker action and the local lint gate.

use std::{error::Error, process::Output};

use rstest::rstest;

use super::{
    LintSandbox,
    lint_sandbox,
    read_repository_file,
    workflow_step,
    workflow_step_field,
    write_fake_tool,
};

const ACTION: &str = concat!(
    "leynos/shared-actions/.github/actions/install-whitaker@",
    "ff1dd759dfffc0db3459e30e833f52437ee62b57"
);

/// Reports a consumer that bypasses the pinned action or its defaults.
fn installation_problems(workflow: &str) -> Vec<&'static str> {
    let mut problems = installation_step_problems(workflow);
    problems.extend(legacy_installation_problems(workflow));
    problems.extend(installation_order_problems(workflow));
    problems
}

/// Reports defects in the named step that installs Whitaker.
fn installation_step_problems(workflow: &str) -> Vec<&'static str> {
    workflow_step(workflow, "Install Whitaker").map_or_else(
        |_| vec!["Whitaker action is absent"],
        installation_step_field_problems,
    )
}

/// Reports pin, input, and failure-handling defects in the installer step.
fn installation_step_field_problems(step: &str) -> Vec<&'static str> {
    let mut problems = Vec::new();
    if !workflow_step_field(step, "uses").is_ok_and(|action| action == ACTION) {
        problems.push("Whitaker action is not pinned to the approved commit");
    }
    if step.contains("\n        with:") {
        problems.push("Whitaker action overrides its pinned defaults");
    }
    if workflow_step_field(step, "if").is_ok() {
        problems.push("Whitaker installation must be unconditional");
    }
    if workflow_step_field(step, "continue-on-error").is_ok() {
        problems.push("Whitaker installation must propagate failures");
    }
    problems
}

/// Reports legacy provisioning routes that can bypass the shared action.
fn legacy_installation_problems(workflow: &str) -> Vec<&'static str> {
    let forbidden = [
        ("Cache Whitaker installer", "separate Whitaker cache"),
        ("WHITAKER_INSTALLER_VERSION", "installer version override"),
        ("suite-version:", "suite version override"),
        ("installer-version:", "installer version override"),
        ("whitaker-installer@", "direct installer dependency"),
        (
            "cargo install --locked whitaker-installer",
            "source install fallback",
        ),
    ];
    let mut problems: Vec<&'static str> = forbidden
        .into_iter()
        .filter(|(pattern, _)| workflow.contains(pattern))
        .map(|(_, complaint)| complaint)
        .collect();
    if workflow
        .lines()
        .any(|line| line.trim() == "whitaker-installer")
    {
        problems.push("direct Whitaker installer invocation");
    }
    problems
}

/// Reports when the pinned installer does not precede both consuming gates.
fn installation_order_problems(workflow: &str) -> Vec<&'static str> {
    let mut problems = Vec::new();
    let install_at = workflow.find("      - name: Install Whitaker\n");
    let lint_at = workflow.find("      - name: Lint\n");
    if !steps_are_ordered(install_at, lint_at) {
        problems.push("Whitaker must install before the lint step");
    }
    let coverage_at = workflow.find("      - name: Test and Measure Coverage\n");
    if !steps_are_ordered(install_at, coverage_at) {
        problems.push("Whitaker must install before the coverage suite step");
    }
    problems
}

/// Returns whether both named steps exist and the installer appears first.
const fn steps_are_ordered(install_at: Option<usize>, consumer_at: Option<usize>) -> bool {
    matches!((install_at, consumer_at), (Some(install), Some(consumer)) if install < consumer)
}

/// The one Linux job consumes the approved action and its defaults before lint.
#[test]
fn ci_installs_whitaker_through_the_pinned_action() {
    let workflow = read_repository_file(".github/workflows/ci.yml").expect("read CI workflow");
    let problems = installation_problems(&workflow);
    assert!(
        problems.is_empty(),
        "Whitaker consumer defects: {problems:?}"
    );
    assert_eq!(workflow.matches("  build-test:\n").count(), 1);
    assert_eq!(
        workflow.matches("      - name: Install Whitaker\n").count(),
        1
    );
}

/// Each deprecated installation route must make the consumer contract fail.
#[rstest]
#[case::unreviewed_ref(
    "install-whitaker@ff1dd759dfffc0db3459e30e833f52437ee62b57",
    "install-whitaker@main"
)]
#[case::suite_override(
    "      - name: Install Whitaker\n",
    "      - name: Install Whitaker\n        with:\n          suite-version: v1\n"
)]
#[case::installer_override(
    "      - name: Install Whitaker\n",
    "      - name: Install Whitaker\n        with:\n          installer-version: 0.2.8\n"
)]
#[case::old_cache(
    "      - name: Install Whitaker\n",
    "      - name: Cache Whitaker installer\n      - name: Install Whitaker\n"
)]
#[case::direct_installer(
    "      - name: Install Whitaker\n",
    "      - name: Install Whitaker\n        run: cargo install --locked whitaker-installer\n"
)]
#[case::bare_installer(
    "      - name: Install Whitaker\n",
    "      - name: Install Whitaker\n        run: |\n          whitaker-installer\n"
)]
fn deprecated_whitaker_routes_are_refused(#[case] old: &str, #[case] replacement: &str) {
    let workflow = read_repository_file(".github/workflows/ci.yml").expect("read CI workflow");
    let changed = workflow.replacen(old, replacement, 1);
    assert_ne!(changed, workflow, "mutation did not change CI");
    assert!(!installation_problems(&changed).is_empty());
}

/// The installer cannot be skipped or allowed to fail while later gates run.
#[rstest]
#[case::conditional_install(
    "        if: ${{ github.event_name == 'push' }}\n",
    "Whitaker installation must be unconditional"
)]
#[case::soft_failing_install(
    "        continue-on-error: true\n",
    "Whitaker installation must propagate failures"
)]
fn installer_bypass_is_refused(#[case] inserted_field: &str, #[case] expected_problem: &str) {
    let workflow = read_repository_file(".github/workflows/ci.yml").expect("read CI workflow");
    let install_step = "      - name: Install Whitaker\n";
    let changed = workflow.replacen(install_step, &format!("{install_step}{inserted_field}"), 1);
    assert_ne!(changed, workflow, "mutation did not change CI");
    let problems = installation_problems(&changed);
    assert!(
        problems.contains(&expected_problem),
        "installer bypass was not detected: {problems:?}"
    );
}

/// Moving the action after the coverage suite must fail its separate ordering
/// clause, even though the same job still contains the pinned action.
#[test]
fn whitaker_installation_precedes_the_coverage_suite() {
    let workflow = read_repository_file(".github/workflows/ci.yml").expect("read CI workflow");
    let install_step = format!("      - name: Install Whitaker\n        uses: {ACTION}\n");
    let without_install = workflow.replacen(&install_step, "", 1);
    assert_ne!(
        without_install, workflow,
        "mutation did not move the action"
    );
    let moved = format!("{without_install}{install_step}");
    let problems = installation_problems(&moved);
    assert!(
        problems.contains(&"Whitaker must install before the coverage suite step"),
        "coverage ordering was not checked: {problems:?}"
    );
}

/// A failed Whitaker invocation must stop `netsuke build lint` before workflow linters.
#[rstest]
fn lint_propagates_a_failing_whitaker(lint_sandbox: Result<LintSandbox, Box<dyn Error>>) {
    let sandbox = lint_sandbox.expect("create lint sandbox");
    let output = sandbox
        .run_lint(Some("whitaker"))
        .expect("run Netsuke lint");
    assert!(!output.status.success(), "failed Whitaker was ignored");
    let invocations = sandbox.invocations().expect("read fake tool calls");
    assert!(
        invocations
            .iter()
            .any(|call| call.starts_with("whitaker\t--all\t--"))
    );
    assert!(
        sandbox
            .workflow_linter_invocations()
            .expect("read later calls")
            .is_empty()
    );
}

/// Whitaker inherits warning strictness without the development build flags.
#[rstest]
fn lint_keeps_dev_flags_out_of_whitaker(lint_sandbox: Result<LintSandbox, Box<dyn Error>>) {
    let sandbox = lint_sandbox.expect("create lint sandbox");
    let output = sandbox.run_lint(None).expect("run Netsuke lint");
    assert!(output.status.success(), "Netsuke lint failed");
    let flags = sandbox
        .directory
        .read_to_string("whitaker-rustflags.log")
        .expect("read Whitaker environment");
    assert_eq!(flags.trim(), "-D warnings");
}

/// `all` must not overlap its gates under Netsuke's serial dependency order.
#[rstest]
fn all_gates_stay_sequential_under_netsuke(lint_sandbox: Result<LintSandbox, Box<dyn Error>>) {
    let sandbox = lint_sandbox.expect("create lint sandbox");
    let output = run_fake_all(&sandbox, None).expect("run fake Netsuke all");
    assert!(
        output.status.success(),
        "netsuke build all failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        sandbox.directory.metadata("gate-lock/overlap").is_err(),
        "gate commands overlapped"
    );
    let calls = sandbox.invocations().expect("read fake gate calls");
    let expected = [
        "cargo\tfmt",
        "mdtablefix\t--check",
        "cargo\tdoc",
        "cargo\tclippy",
        "whitaker\t--all",
        "yamllint\t.github/workflows",
        "actionlint",
        "cargo\tnextest\trun",
        "cargo\ttest\t--workspace\t--doc",
        "uv\ttool\trun\t--python\t3.14",
        "uv\ttool\trun\t--python\t3.13",
    ];
    let positions: Vec<usize> = expected
        .iter()
        .map(|prefix| {
            calls
                .iter()
                .position(|call| call.starts_with(prefix))
                .expect("all gate tool was called")
        })
        .collect();
    assert!(
        positions
            .iter()
            .zip(positions.iter().skip(1))
            .all(|(before, after)| before < after),
        "gates ran out of order: {calls:?}"
    );
}

/// A failing Whitaker stops `all` before test or spelling.
#[rstest]
fn all_stops_after_whitaker_fails(lint_sandbox: Result<LintSandbox, Box<dyn Error>>) {
    let sandbox = lint_sandbox.expect("create lint sandbox");
    let output = run_fake_all(&sandbox, Some("whitaker")).expect("run failing Netsuke all");
    assert!(
        !output.status.success(),
        "Netsuke all ignored Whitaker failure"
    );
    let calls = sandbox.invocations().expect("read fake gate calls");
    assert!(calls.iter().any(|call| call.starts_with("whitaker\t--all")));
    assert!(!calls.iter().any(|call| call.starts_with("cargo\ttest")));
    assert!(!calls.iter().any(|call| call.starts_with("uv\ttool\trun")));
}

/// Executes the public composite target with fake gates and an overlap marker.
fn run_fake_all(
    sandbox: &LintSandbox,
    failing_tool: Option<&str>,
) -> Result<Output, Box<dyn Error>> {
    write_fake_tool(&sandbox.directory, "mdtablefix")?;
    write_fake_tool(&sandbox.directory, "uv")?;
    sandbox.directory.create_dir("gate-lock")?;
    sandbox.run_netsuke("all", failing_tool, &sandbox.tool_command("yamllint"))
}
