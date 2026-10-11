//! Integration tests for the repository's GitHub Actions linting boundary.

use std::{
    error::Error,
    io,
    path::PathBuf,
    process::{Command, Output},
};

use cap_std::{
    ambient_authority,
    fs::{Dir, PermissionsExt},
};
use rstest::{fixture, rstest};
use tempfile::TempDir;

const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
const WORKFLOW_FILES: [&str; 5] = [
    ".github/workflows/ci.yml",
    ".github/workflows/coverage-main.yml",
    ".github/workflows/delayed-pr-comment.yml",
    ".github/workflows/dependabot-automerge.yml",
    ".github/workflows/release.yml",
];
const YAML_POLICY: &str = ".yamllint-estate.yml";
const YAML_INSTALLER: &str = "leynos/shared-actions/.github/actions/install-yaml-lint@";

#[path = "workflow_lint/codescene_token.rs"]
mod codescene_token;
#[path = "workflow_lint/whitaker.rs"]
mod whitaker;

#[rstest]
fn lint_target_invokes_the_workflow_linters(lint_sandbox: Result<LintSandbox, Box<dyn Error>>) {
    let sandbox = lint_sandbox.expect("create lint sandbox");

    let output = sandbox.run_lint(None).expect("run make lint");

    assert!(
        output.status.success(),
        "make lint failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_workflow_linters(
        &sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations"),
    );
}

#[rstest]
fn lint_target_propagates_a_workflow_linter_failure(
    lint_sandbox: Result<LintSandbox, Box<dyn Error>>,
) {
    let sandbox = lint_sandbox.expect("create lint sandbox");

    let output = sandbox.run_lint(Some("actionlint")).expect("run make lint");

    assert!(!output.status.success(), "make lint unexpectedly succeeded");
    assert_workflow_linters(
        &sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations"),
    );
}

#[rstest]
fn lint_target_stops_at_a_failing_yamllint(lint_sandbox: Result<LintSandbox, Box<dyn Error>>) {
    let sandbox = lint_sandbox.expect("create lint sandbox");

    let output = sandbox.run_lint(Some("yamllint")).expect("run make lint");

    assert!(!output.status.success(), "make lint unexpectedly succeeded");
    let invocations = sandbox
        .workflow_linter_invocations()
        .expect("read workflow linter invocations");
    let [yamllint] = invocations.as_slice() else {
        panic!("actionlint ran after a yamllint failure: {invocations:?}");
    };
    assert_yamllint_covers_every_tracked_file(yamllint);
}

#[rstest]
fn lint_target_fails_when_a_workflow_linter_is_missing(
    lint_sandbox: Result<LintSandbox, Box<dyn Error>>,
) {
    let sandbox = lint_sandbox.expect("create lint sandbox");

    let output = sandbox.run_with_missing_yamllint().expect("run make lint");

    assert!(!output.status.success(), "make lint unexpectedly succeeded");
    assert!(
        sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations")
            .is_empty()
    );
}

#[rstest]
fn workflow_lint_policy_supports_github_actions_and_pinned_ci_tools(
    lint_sandbox: Result<LintSandbox, Box<dyn Error>>,
) {
    let _sandbox = lint_sandbox.expect("create lint sandbox");
    let yamllint_policy = read_repository_file(YAML_POLICY).expect("read yamllint policy");
    assert!(yamllint_policy.contains("check-keys: false"));
    assert!(yamllint_policy.contains("allowed-values: ['true', 'false']"));

    for workflow_file in WORKFLOW_FILES {
        assert!(
            !read_repository_file(workflow_file)
                .expect("read GitHub Actions workflow")
                .trim()
                .is_empty(),
            "workflow {workflow_file} is empty"
        );
    }

    let ci_workflow = read_repository_file(CI_WORKFLOW).expect("read CI workflow");
    for retired in ["YAMLLINT_VERSION", "Cache yamllint", "Download actionlint"] {
        assert!(
            !ci_workflow.contains(retired),
            "CI still provisions the lint tools by hand: {retired}"
        );
    }

    let installer = workflow_step(&ci_workflow, "Install the YAML lint tools")
        .expect("find the YAML lint installer");
    let reference = workflow_step_field(installer, "uses")
        .expect("find the installer action")
        .strip_prefix(YAML_INSTALLER)
        .expect("the installer is the shared install-yaml-lint action");
    assert!(
        reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "the installer is pinned to {reference}, not a full commit SHA"
    );

    let lint_step = workflow_step(&ci_workflow, "Lint").expect("find lint step");
    assert_eq!(
        workflow_step_field(lint_step, "run").expect("find lint command"),
        "/usr/bin/make lint"
    );
    assert!(
        ci_workflow.find("Install the YAML lint tools") < ci_workflow.find("      - name: Lint\n"),
        "the installer must run before the lint gate"
    );
}

/// The workflow linters run once each: yamllint strictly over the tracked
/// YAML, then actionlint.
fn assert_workflow_linters(invocations: &[String]) {
    let [yamllint, actionlint] = invocations else {
        panic!("expected one yamllint and one actionlint run, got {invocations:?}");
    };
    assert_yamllint_covers_every_tracked_file(yamllint);
    assert_eq!(actionlint, "actionlint");
}

/// yamllint runs strictly, over exactly the Git-tracked YAML files.
fn assert_yamllint_covers_every_tracked_file(invocation: &str) {
    let mut arguments = invocation.split('\t');
    assert_eq!(arguments.next(), Some("yamllint"), "{invocation}");
    assert_eq!(arguments.next(), Some("--strict"), "{invocation}");
    assert_eq!(arguments.next(), Some("--"), "{invocation}");
    let mut given: Vec<&str> = arguments.collect();
    given.sort_unstable();
    let Ok(mut tracked) = tracked_yaml_files() else {
        panic!("could not list the tracked YAML files");
    };
    tracked.sort_unstable();
    assert_eq!(
        given, tracked,
        "yamllint was not given the tracked YAML files"
    );
}

fn tracked_yaml_files() -> Result<Vec<String>, Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(repository_root())
        .args(["ls-files", "-z", "*.yml", "*.yaml"])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("git ls-files failed").into());
    }
    Ok(String::from_utf8(output.stdout)?
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

struct LintSandbox {
    directory: Dir,
    invocation_log: PathBuf,
    temporary_directory: TempDir,
}

#[fixture]
fn lint_sandbox() -> Result<LintSandbox, Box<dyn Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let directory = Dir::open_ambient_dir(temporary_directory.path(), ambient_authority())?;
    let invocation_log = temporary_directory.path().join("invocations.log");
    for tool in ["cargo", "whitaker", "yamllint", "actionlint"] {
        write_fake_tool(&directory, tool)?;
    }
    Ok(LintSandbox {
        directory,
        invocation_log,
        temporary_directory,
    })
}

impl LintSandbox {
    fn invocations(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let invocations = self.directory.read_to_string("invocations.log")?;
        Ok(invocations.lines().map(str::to_owned).collect())
    }

    fn workflow_linter_invocations(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self
            .invocations()?
            .into_iter()
            .filter(|invocation| {
                matches!(
                    invocation.split('\t').next(),
                    Some("yamllint" | "actionlint")
                )
            })
            .collect())
    }

    fn run_lint(&self, failing_tool: Option<&str>) -> Result<Output, Box<dyn Error>> {
        self.run_make(failing_tool, &self.tool_command("yamllint"))
    }

    fn run_with_missing_yamllint(&self) -> Result<Output, Box<dyn Error>> {
        self.run_make(None, "/missing/yamllint")
    }

    fn run_make(
        &self,
        failing_tool: Option<&str>,
        yamllint: &str,
    ) -> Result<Output, Box<dyn Error>> {
        let mut command = Command::new("make");
        command
            .current_dir(repository_root())
            .arg("lint")
            .arg(format!("CARGO={}", self.tool_command("cargo")))
            .arg(format!("WHITAKER={}", self.tool_command("whitaker")))
            .arg(format!("YAMLLINT={yamllint}"))
            .arg(format!("ACTIONLINT={}", self.tool_command("actionlint")))
            .env("LINT_INVOCATION_LOG", &self.invocation_log)
            .env(
                "WHITAKER_RUSTFLAGS_LOG",
                self.temporary_directory
                    .path()
                    .join("whitaker-rustflags.log"),
            );
        if let Some(tool_to_fail) = failing_tool {
            command.env("FAILING_TOOL", tool_to_fail);
        }
        Ok(command.output()?)
    }

    fn tool_command(&self, tool: &str) -> String {
        self.temporary_directory
            .path()
            .join(tool)
            .display()
            .to_string()
    }
}

fn read_repository_file(path: &str) -> Result<String, Box<dyn Error>> {
    Ok(repository_directory()?.read_to_string(path)?)
}

fn repository_root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")) }

fn repository_directory() -> Result<Dir, Box<dyn Error>> {
    Ok(Dir::open_ambient_dir(
        repository_root(),
        ambient_authority(),
    )?)
}

fn workflow_step<'workflow>(
    workflow: &'workflow str,
    name: &str,
) -> Result<&'workflow str, io::Error> {
    let step_start = format!("      - name: {name}\n");
    workflow
        .split_once(&step_start)
        .map(|(_, step)| step.split("\n      - ").next().unwrap_or(step))
        .ok_or_else(|| io::Error::other("CI workflow contains the required step"))
}

fn workflow_step_field<'workflow>(
    step: &'workflow str,
    name: &str,
) -> Result<&'workflow str, io::Error> {
    let field_start = format!("        {name}: ");
    step.lines()
        .find_map(|line| line.strip_prefix(&field_start))
        .ok_or_else(|| io::Error::other("CI workflow step contains the required field"))
}

fn write_fake_tool(directory: &Dir, tool: &str) -> Result<(), Box<dyn Error>> {
    directory.write(
        tool,
        concat!(
            "#!/bin/sh\n",
            "tool_name=${0##*/}\n",
            "{\n",
            "  printf '%s' \"${tool_name}\"\n",
            "  for argument in \"$@\"; do\n",
            "    printf '\\t%s' \"${argument}\"\n",
            "  done\n",
            "  printf '\\n'\n",
            "} >> \"${LINT_INVOCATION_LOG}\"\n",
            "if [ \"${tool_name}\" = whitaker ] && [ -n \"${WHITAKER_RUSTFLAGS_LOG:-}\" ]; then\n",
            "  printf '%s\\n' \"${RUSTFLAGS-<unset>}\" > \"${WHITAKER_RUSTFLAGS_LOG}\"\n",
            "fi\n",
            "if [ -n \"${GATE_LOCK_DIR:-}\" ]; then\n",
            "  acquired=false\n",
            "  if mkdir \"${GATE_LOCK_DIR}/active\" 2>/dev/null; then\n",
            "    acquired=true\n",
            "  else\n",
            "    printf '%s\\n' \"${tool_name}\" >> \"${GATE_LOCK_DIR}/overlap\"\n",
            "  fi\n",
            "  sleep 0.02\n",
            "  if [ \"${acquired}\" = true ]; then rmdir \"${GATE_LOCK_DIR}/active\"; fi\n",
            "fi\n",
            "if [ \"${tool_name}\" = \"${FAILING_TOOL:-}\" ]; then\n",
            "  exit 23\n",
            "fi\n",
        ),
    )?;
    let mut permissions = directory.metadata(tool)?.permissions();
    permissions.set_mode(0o755);
    directory.set_permissions(tool, permissions)?;
    Ok(())
}
