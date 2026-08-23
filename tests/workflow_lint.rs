//! Integration tests for the repository's GitHub Actions linting boundary.

use std::{
    error::Error,
    path::PathBuf,
    process::{Command, Output},
};

use cap_std::{
    ambient_authority,
    fs::{Dir, PermissionsExt},
};
use tempfile::TempDir;

const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
const YAML_POLICY: &str = ".yamllint.yml";

#[test]
fn lint_target_invokes_the_workflow_linters() {
    let sandbox = LintSandbox::new().expect("create lint sandbox");

    let output = sandbox.run_lint(None).expect("run make lint");

    assert!(
        output.status.success(),
        "make lint failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations"),
        ["yamllint", "actionlint"]
    );
}

#[test]
fn lint_target_propagates_a_workflow_linter_failure() {
    let sandbox = LintSandbox::new().expect("create lint sandbox");

    let output = sandbox.run_lint(Some("actionlint")).expect("run make lint");

    assert!(!output.status.success(), "make lint unexpectedly succeeded");
    assert_eq!(
        sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations"),
        ["yamllint", "actionlint"]
    );
}

#[test]
fn lint_target_fails_when_a_workflow_linter_is_missing() {
    let sandbox = LintSandbox::new().expect("create lint sandbox");

    let output = sandbox.run_with_missing_yamllint().expect("run make lint");

    assert!(!output.status.success(), "make lint unexpectedly succeeded");
    assert!(
        sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations")
            .is_empty()
    );
}

#[test]
fn workflow_lint_policy_supports_github_actions_and_pinned_ci_tools() {
    let yamllint_policy = read_repository_file(YAML_POLICY).expect("read yamllint policy");
    assert!(yamllint_policy.contains("check-keys: false"));
    assert!(yamllint_policy.contains("allowed-values: ['true', 'false']"));

    let ci_workflow = read_repository_file(CI_WORKFLOW).expect("read CI workflow");
    assert!(ci_workflow.contains("actionlint-${{ runner.os }}-${{ runner.arch }}-1.7.12"));
    assert!(ci_workflow.contains("readonly ACTIONLINT_VERSION='1.7.12'"));
    assert!(ci_workflow.contains("914e7df21a07ef503a81201c76d2b11c789d3fca"));
    assert!(ci_workflow.contains("sha256sum --check --"));
    assert!(
        ci_workflow.contains("bash \"${ACTIONLINT_INSTALLER_PATH}\" \"${ACTIONLINT_VERSION}\"")
    );
}

struct LintSandbox {
    directory: Dir,
    invocation_log: PathBuf,
    temporary_directory: TempDir,
}

impl LintSandbox {
    fn new() -> Result<Self, Box<dyn Error>> {
        let temporary_directory = tempfile::tempdir()?;
        let directory = Dir::open_ambient_dir(temporary_directory.path(), ambient_authority())?;
        let invocation_log = temporary_directory.path().join("invocations.log");
        for tool in ["cargo", "whitaker", "yamllint", "actionlint"] {
            write_fake_tool(&directory, tool)?;
        }
        Ok(Self {
            directory,
            invocation_log,
            temporary_directory,
        })
    }

    fn invocations(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let invocations = self.directory.read_to_string("invocations.log")?;
        Ok(invocations.lines().map(str::to_owned).collect())
    }

    fn workflow_linter_invocations(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self
            .invocations()?
            .into_iter()
            .filter(|invocation| matches!(invocation.as_str(), "yamllint" | "actionlint"))
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
            .env("LINT_INVOCATION_LOG", &self.invocation_log);
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

fn write_fake_tool(directory: &Dir, tool: &str) -> Result<(), Box<dyn Error>> {
    directory.write(
        tool,
        concat!(
            "#!/bin/sh\n",
            "tool_name=${0##*/}\n",
            "printf '%s\\n' \"${tool_name}\" >> \"${LINT_INVOCATION_LOG}\"\n",
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
