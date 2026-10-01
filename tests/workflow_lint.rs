//! Integration tests for the repository's GitHub Actions linting boundary.

use std::{
    env,
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
const NETSUKEFILE: &str = "Netsukefile";
const WORKFLOW_FILES: [&str; 5] = [
    ".github/workflows/ci.yml",
    ".github/workflows/coverage-main.yml",
    ".github/workflows/delayed-pr-comment.yml",
    ".github/workflows/dependabot-automerge.yml",
    ".github/workflows/release.yml",
];
const YAML_POLICY: &str = ".yamllint.yml";

#[path = "workflow_lint/codescene_token.rs"]
mod codescene_token;
#[path = "workflow_lint/whitaker.rs"]
mod whitaker;

/// Confirm the aggregate lint action reaches both workflow linters in order.
#[rstest]
fn lint_target_invokes_the_workflow_linters(lint_sandbox: Result<LintSandbox, Box<dyn Error>>) {
    let sandbox = lint_sandbox.expect("create lint sandbox");

    let output = sandbox.run_lint(None).expect("run netsuke build lint");

    assert!(
        output.status.success(),
        "netsuke build lint failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations"),
        ["yamllint\t.github/workflows", "actionlint"]
    );
}

/// A failing workflow linter must make the aggregate action fail.
#[rstest]
fn lint_target_propagates_a_workflow_linter_failure(
    lint_sandbox: Result<LintSandbox, Box<dyn Error>>,
) {
    let sandbox = lint_sandbox.expect("create lint sandbox");

    let output = sandbox
        .run_lint(Some("actionlint"))
        .expect("run netsuke build lint");

    assert!(
        !output.status.success(),
        "netsuke build lint unexpectedly succeeded"
    );
    assert_eq!(
        sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations"),
        ["yamllint\t.github/workflows", "actionlint"]
    );
}

/// A missing linter must not silently turn the workflow gate into a no-op.
#[rstest]
fn lint_target_fails_when_a_workflow_linter_is_missing(
    lint_sandbox: Result<LintSandbox, Box<dyn Error>>,
) {
    let sandbox = lint_sandbox.expect("create lint sandbox");

    let output = sandbox
        .run_with_missing_yamllint()
        .expect("run netsuke build lint");

    assert!(
        !output.status.success(),
        "netsuke build lint unexpectedly succeeded"
    );
    assert!(
        sandbox
            .workflow_linter_invocations()
            .expect("read workflow linter invocations")
            .is_empty()
    );
}

/// Keep the workflow and its pinned CI tools aligned with the build graph.
#[rstest]
fn workflow_lint_policy_supports_github_actions_and_pinned_ci_tools(
    lint_sandbox: Result<LintSandbox, Box<dyn Error>>,
) {
    let _sandbox = lint_sandbox.expect("create lint sandbox");
    let manifest = read_repository_file(NETSUKEFILE).expect("read Netsukefile");
    assert!(manifest.contains("      - rust-lint\n      - github-actions-lint\n"));
    assert!(manifest.contains("  - name: github-actions-lint\n"));
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
    assert_eq!(
        workflow_environment_value(&ci_workflow, "YAMLLINT_VERSION")
            .expect("find YAMLLINT_VERSION"),
        "1.38.0"
    );

    let yamllint_cache =
        workflow_step(&ci_workflow, "Cache yamllint").expect("find yamllint cache");
    assert_eq!(
        workflow_step_field(yamllint_cache, "uses").expect("find yamllint cache action"),
        "actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9"
    );
    assert!(yamllint_cache.contains(
        "          path: |\n            .uv-cache\n            .uv-tools\n            .uv-bin"
    ));
    assert!(yamllint_cache.contains(
        "          key: yamllint-${{ runner.os }}-${{ runner.arch }}-${{ env.YAMLLINT_VERSION }}"
    ));

    let yamllint_install =
        workflow_step(&ci_workflow, "Install yamllint").expect("find yamllint installation");
    assert!(yamllint_install.contains("uv tool install \"yamllint==${YAMLLINT_VERSION}\""));
    assert!(yamllint_install.contains("echo \"${UV_TOOL_BIN_DIR}\" >> \"$GITHUB_PATH\""));

    let actionlint_cache =
        workflow_step(&ci_workflow, "Cache actionlint").expect("find actionlint cache");
    assert_eq!(
        workflow_step_field(actionlint_cache, "id").expect("find actionlint cache id"),
        "cache_actionlint"
    );
    assert_eq!(
        actionlint_cache
            .lines()
            .find_map(|line| line.strip_prefix("          path: "))
            .expect("actionlint cache has a path"),
        "actionlint"
    );
    assert!(
        actionlint_cache
            .contains("          key: actionlint-${{ runner.os }}-${{ runner.arch }}-1.7.12")
    );

    let actionlint_download =
        workflow_step(&ci_workflow, "Download actionlint").expect("find actionlint download");
    assert_eq!(
        workflow_step_field(actionlint_download, "id").expect("find actionlint download id"),
        "get_actionlint"
    );
    assert_eq!(
        workflow_step_field(actionlint_download, "if").expect("find actionlint cache condition"),
        "steps.cache_actionlint.outputs.cache-hit != 'true'"
    );
    assert!(
        actionlint_download.contains("readonly ACTIONLINT_VERSION='1.7.12'"),
        "actionlint downloader does not pin its version"
    );
    assert!(
        actionlint_download.contains(
            "readonly ACTIONLINT_INSTALLER_COMMIT='914e7df21a07ef503a81201c76d2b11c789d3fca'"
        ),
        "actionlint downloader does not pin its installer"
    );
    assert!(
        actionlint_download.contains("sha256sum --check --"),
        "actionlint downloader does not verify its archive"
    );
    assert!(
        actionlint_download
            .contains("bash \"${ACTIONLINT_INSTALLER_PATH}\" \"${ACTIONLINT_VERSION}\""),
        "actionlint downloader does not pass its pinned version"
    );

    let lint_step = workflow_step(&ci_workflow, "Lint").expect("find lint step");
    assert_eq!(
        workflow_step_field(lint_step, "run").expect("find lint command"),
        "ACTIONLINT=\"$GITHUB_WORKSPACE/actionlint\" netsuke build lint"
    );
}

struct LintSandbox {
    directory: Dir,
    invocation_log: PathBuf,
    temporary_directory: TempDir,
}

/// Give each Netsuke invocation its own graph to permit parallel Rust tests.
#[fixture]
fn lint_sandbox() -> Result<LintSandbox, Box<dyn Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let directory = Dir::open_ambient_dir(temporary_directory.path(), ambient_authority())?;
    directory.write(NETSUKEFILE, read_repository_file(NETSUKEFILE)?)?;
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

    /// Exercise the real lint graph while selecting a fake failing tool.
    fn run_lint(&self, failing_tool: Option<&str>) -> Result<Output, Box<dyn Error>> {
        self.run_netsuke(failing_tool, &self.tool_command("yamllint"))
    }

    /// Model failed tool discovery independently of the host's installations.
    fn run_with_missing_yamllint(&self) -> Result<Output, Box<dyn Error>> {
        self.run_netsuke(None, "/missing/yamllint")
    }

    /// Run from the isolated manifest so tests cannot contend for Ninja state.
    fn run_netsuke(
        &self,
        failing_tool: Option<&str>,
        yamllint: &str,
    ) -> Result<Output, Box<dyn Error>> {
        let mut path = vec![self.temporary_directory.path().to_path_buf()];
        path.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
        let mut command = Command::new("netsuke");
        command
            .current_dir(self.temporary_directory.path())
            .args(["build", "lint"])
            .env("PATH", env::join_paths(path)?)
            .env("YAMLLINT", yamllint)
            .env("ACTIONLINT", self.tool_command("actionlint"))
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

fn workflow_environment_value<'workflow>(
    workflow: &'workflow str,
    name: &str,
) -> Result<&'workflow str, io::Error> {
    workflow
        .split_once("    env:\n")
        .and_then(|(_, environment)| environment.split_once("    steps:\n"))
        .and_then(|(environment, _)| {
            environment.lines().find_map(|line| {
                line.strip_prefix("      ")
                    .and_then(|value| value.strip_prefix(name))
                    .and_then(|value| value.strip_prefix(": "))
            })
        })
        .map(|value| value.trim_matches('\''))
        .ok_or_else(|| io::Error::other("CI workflow defines the required environment value"))
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
