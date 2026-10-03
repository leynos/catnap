//! Integration checks for the public Netsuke build actions.

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
use rstest::rstest;
use tempfile::TempDir;

const MANIFEST: &str = include_str!("../Netsukefile");
const FAKE_TOOLS: [&str; 8] = [
    "cargo",
    "whitaker",
    "yamllint",
    "actionlint",
    "mdtablefix",
    "bunx",
    "uv",
    "nixie",
];

/// Keep each generated Ninja graph and command log private to one test.
struct ActionSandbox {
    directory: Dir,
    temporary_directory: TempDir,
    tools_directory: TempDir,
    netsuke: PathBuf,
}

impl ActionSandbox {
    /// Prepare fake commands while retaining the real Netsuke executable.
    fn new(with_nextest: bool) -> Result<Self, Box<dyn Error>> {
        let temporary_directory = tempfile::tempdir()?;
        let directory = Dir::open_ambient_dir(temporary_directory.path(), ambient_authority())?;
        let tools_directory = tempfile::tempdir()?;
        let tools = Dir::open_ambient_dir(tools_directory.path(), ambient_authority())?;
        directory.write("Netsukefile", MANIFEST)?;
        directory.write("README.md", "# Sandbox\n")?;
        directory.create_dir_all("scripts")?;
        for script in ["check-build-tools.sh", "install-build-tools.sh"] {
            write_fake_tool(&directory, &format!("scripts/{script}"))?;
        }
        for tool in FAKE_TOOLS {
            write_fake_tool(&tools, tool)?;
        }
        if with_nextest {
            write_fake_tool(&tools, "cargo-nextest")?;
        }
        Ok(Self {
            directory,
            temporary_directory,
            tools_directory,
            netsuke: netsuke_binary()?,
        })
    }

    /// Run a public action without using the repository's shared Ninja state.
    fn run(
        &self,
        action: Option<&str>,
        failing_tool: Option<&str>,
    ) -> Result<Output, Box<dyn Error>> {
        let mut command = Command::new(&self.netsuke);
        if let Some(action_name) = action {
            command.args(["build", action_name]);
        }
        command
            .current_dir(self.temporary_directory.path())
            .env(
                "PATH",
                env::join_paths([
                    self.tools_directory.path(),
                    std::path::Path::new("/usr/bin"),
                    std::path::Path::new("/bin"),
                ])?,
            )
            .env_remove("BASH_ENV")
            .env_remove("RUSTFLAGS")
            .env_remove("RUSTDOCFLAGS")
            .env_remove("CFLAGS")
            .env_remove("LDFLAGS")
            .env_remove("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER")
            .env(
                "ACTION_LOG",
                self.temporary_directory.path().join("commands.log"),
            )
            .env(
                "ENV_LOG",
                self.temporary_directory.path().join("environment.log"),
            );
        if let Some(tool) = failing_tool {
            command.env("FAILING_TOOL", tool);
        }
        Ok(command.output()?)
    }

    /// Return exact tool-and-argument records in execution order.
    fn commands(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self
            .directory
            .read_to_string("commands.log")?
            .lines()
            .map(str::to_owned)
            .collect())
    }

    /// Return the warning and coverage environment seen by each fake tool.
    fn environment(&self) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(self
            .directory
            .read_to_string("environment.log")?
            .lines()
            .map(str::to_owned)
            .collect())
    }
}

/// Find the installed runner before constraining the sandbox's command path.
fn netsuke_binary() -> Result<PathBuf, Box<dyn Error>> {
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path)
        .map(|directory| directory.join("netsuke"))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "netsuke is required for integration tests",
            )
            .into()
        })
}

/// Log command arguments and selected environment fields without running tools.
fn write_fake_tool(directory: &Dir, name: &str) -> Result<(), Box<dyn Error>> {
    directory.write(
        name,
        concat!(
            "#!/bin/sh\n",
            "tool_name=${0##*/}\n",
            "printf '%s' \"${tool_name}\" >> \"${ACTION_LOG}\"\n",
            "for argument in \"$@\"; do printf '\\t%s' \"${argument}\" >> \"${ACTION_LOG}\"; \
             done\n",
            "printf '\\n' >> \"${ACTION_LOG}\"\n",
            "printf '%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n' \\\n",
            "  \"${tool_name}\" \"${RUSTFLAGS:-}\" \"${RUSTDOCFLAGS:-}\" \\\n",
            "  \"${CFLAGS:-}\" \"${LDFLAGS:-}\" \\\n",
            "  \"${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER:-}\" >> \"${ENV_LOG}\"\n",
            "if [ \"${tool_name}\" = \"${FAILING_TOOL:-}\" ]; then exit 23; fi\n",
        ),
    )?;
    let mut permissions = directory.metadata(name)?.permissions();
    permissions.set_mode(0o755);
    directory.set_permissions(name, permissions)?;
    Ok(())
}

/// Verify each action's command contract with the real manifest and fake tools.
#[rstest]
#[case::clean("clean", &["cargo\tclean"])]
#[case::build("build", &["check-build-tools.sh", "cargo\tbuild\t--bin\tcatnap"])]
#[case::release("release", &["cargo\t+stable\tbuild\t--release\t--bin\tcatnap"])]
#[case::coverage("coverage", &["check-build-tools.sh\t--coverage-only", "cargo\tllvm-cov\t--lcov\t--output-path\tlcov.info\t--all-targets\t--all-features"])]
#[case::typecheck("typecheck", &["check-build-tools.sh", "cargo\tcheck\t--all-targets\t--all-features"])]
#[case::check_build_tools("check-build-tools", &["check-build-tools.sh"])]
#[case::check_rust_toolchain("check-rust-toolchain", &["check-build-tools.sh\t--toolchain-only"])]
#[case::check_coverage_tools("check-coverage-tools", &["check-build-tools.sh\t--coverage-only"])]
#[case::install_build_tools("install-build-tools", &["install-build-tools.sh"])]
#[case::install_cranelift("install-cranelift", &["install-build-tools.sh\t--cranelift-only"])]
#[case::test_ui("test-ui", &["check-build-tools.sh", "cargo\ttest\t--test\tui"])]
#[case::fmt("fmt", &[
    "check-build-tools.sh\t--toolchain-only",
    "cargo\tfmt\t--all",
    "mdtablefix\t--in-place\t--git\t--include-untracked\t--wrap\t--renumber\t--breaks\t--ellipsis\t--fences",
    "bunx\t--silent\tmarkdownlint-cli2@0.23.3\t--fix\t**/*.md",
])]
#[case::check_fmt("check-fmt", &[
    "check-build-tools.sh\t--toolchain-only",
    "cargo\tfmt\t--all\t--\t--check",
    "mdtablefix\t--check\t--git\t--include-untracked\t--wrap\t--renumber\t--breaks\t--ellipsis\t--fences",
])]
#[case::rust_lint("rust-lint", &[
    "cargo\tdoc\t--no-deps",
    "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
    "whitaker\t--all\t--\t--all-targets\t--all-features",
])]
#[case::github_actions_lint("github-actions-lint", &[
    "yamllint\t.github/workflows",
    "actionlint",
])]
#[case::lint("lint", &[
    "check-build-tools.sh",
    "cargo\tdoc\t--no-deps",
    "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
    "whitaker\t--all\t--\t--all-targets\t--all-features",
    "yamllint\t.github/workflows",
    "actionlint",
])]
#[case::markdownlint("markdownlint", &[
    "uv\ttool\trun\t--python\t3.14\t--from\tgit+https://github.com/leynos/typos-config-builder.git@v0.1.3\ttypos-config-builder\tgate\t--repository\t.",
    "bunx\t--silent\tmarkdownlint-cli2@0.23.3\t./README.md",
])]
#[case::spelling("spelling", &[
    "uv\ttool\trun\t--python\t3.14\t--from\tgit+https://github.com/leynos/typos-config-builder.git@v0.1.3\ttypos-config-builder\tgate\t--repository\t.",
])]
#[case::workflow_contracts("test-workflow-contracts", &[
    "uv\ttool\trun\t--python\t3.13\t--from\tgit+https://github.com/leynos/shared-actions@a38feb9be25755c30eca5bda96bd3786a5b89c6b#subdirectory=packages/cv005-contracts\tcv005-contracts\tcheck\t--repository\t.",
])]
#[case::nixie("nixie", &["nixie\t--no-sandbox"])]
fn public_action_runs_expected_commands(#[case] action: &str, #[case] expected: &[&str]) {
    let sandbox = ActionSandbox::new(false).expect("create isolated Netsuke sandbox");
    let output = sandbox.run(Some(action), None).expect("run Netsuke action");
    assert!(
        output.status.success(),
        "{action}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(sandbox.commands().expect("read fake-tool log"), expected);
}

/// A failing tool must fail its action and prevent subsequent commands.
#[rstest]
#[case::clean("clean", "cargo")]
#[case::test("test", "cargo")]
#[case::build("build", "cargo")]
#[case::release("release", "cargo")]
#[case::coverage("coverage", "cargo")]
#[case::lint("lint", "yamllint")]
#[case::rust_lint("rust-lint", "whitaker")]
#[case::github_actions_lint("github-actions-lint", "yamllint")]
#[case::typecheck("typecheck", "cargo")]
#[case::fmt("fmt", "mdtablefix")]
#[case::check_fmt("check-fmt", "mdtablefix")]
#[case::markdownlint("markdownlint", "uv")]
#[case::spelling("spelling", "uv")]
#[case::workflow_contracts("test-workflow-contracts", "uv")]
#[case::nixie("nixie", "nixie")]
#[case::check_build_tools("check-build-tools", "check-build-tools.sh")]
#[case::check_rust_toolchain("check-rust-toolchain", "check-build-tools.sh")]
#[case::check_coverage_tools("check-coverage-tools", "check-build-tools.sh")]
#[case::install_build_tools("install-build-tools", "install-build-tools.sh")]
#[case::install_cranelift("install-cranelift", "install-build-tools.sh")]
#[case::test_ui("test-ui", "cargo")]
fn public_action_propagates_tool_failure(#[case] action: &str, #[case] failing_tool: &str) {
    let sandbox = ActionSandbox::new(false).expect("create isolated Netsuke sandbox");
    let output = sandbox
        .run(Some(action), Some(failing_tool))
        .expect("run failing Netsuke action");
    assert!(!output.status.success(), "{action} unexpectedly succeeded");
    let commands = sandbox.commands().expect("read fake-tool log");
    assert!(
        commands
            .last()
            .is_some_and(|command| command.starts_with(failing_tool)),
        "{action} ran another command after {failing_tool}: {commands:?}"
    );
}

/// Ensure nextest is selected only when its executable is available.
#[rstest]
#[case::nextest(true, &["cargo\tnextest\trun\t--all-targets\t--all-features", "cargo\ttest\t--workspace\t--doc\t--all-features"])]
#[case::cargo(false, &["cargo\ttest\t--all-targets\t--all-features"])]
fn test_action_selects_available_runner(#[case] with_nextest: bool, #[case] expected: &[&str]) {
    let sandbox = ActionSandbox::new(with_nextest).expect("create isolated Netsuke sandbox");
    let output = sandbox.run(Some("test"), None).expect("run test action");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = sandbox.commands().expect("read fake-tool log");
    let test_calls: Vec<&str> = commands.iter().skip(1).map(String::as_str).collect();
    assert_eq!(test_calls, expected);
    assert!(
        sandbox
            .environment()
            .expect("read environment log")
            .iter()
            .filter(|record| record.starts_with("cargo\t"))
            .all(|record| record.contains("-D warnings -Zthreads=8 -Zcodegen-backend=cranelift"))
    );
}

/// Run the default graph to guard serial dependencies and their order.
#[test]
fn default_action_runs_checks_in_declared_order() {
    let sandbox = ActionSandbox::new(true).expect("create isolated Netsuke sandbox");
    let output = sandbox.run(None, None).expect("run default action");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(sandbox.commands().expect("read fake-tool log"), [
        "check-build-tools.sh\t--toolchain-only",
        "cargo\tfmt\t--all\t--\t--check",
        "mdtablefix\t--check\t--git\t--include-untracked\t--wrap\t--renumber\t--breaks\t--ellipsis\t--fences",
        "check-build-tools.sh",
        "cargo\tdoc\t--no-deps",
        "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
        "whitaker\t--all\t--\t--all-targets\t--all-features",
        "yamllint\t.github/workflows",
        "actionlint",
        "cargo\tnextest\trun\t--all-targets\t--all-features",
        "cargo\ttest\t--workspace\t--doc\t--all-features",
        "uv\ttool\trun\t--python\t3.14\t--from\tgit+https://github.com/leynos/typos-config-builder.git@v0.1.3\ttypos-config-builder\tgate\t--repository\t.",
        "uv\ttool\trun\t--python\t3.13\t--from\tgit+https://github.com/leynos/shared-actions@a38feb9be25755c30eca5bda96bd3786a5b89c6b#subdirectory=packages/cv005-contracts\tcv005-contracts\tcheck\t--repository\t.",
    ]);
}

/// A failed dependency must prevent later checks from running.
#[test]
fn default_action_stops_at_first_failure() {
    let sandbox = ActionSandbox::new(true).expect("create isolated Netsuke sandbox");
    let output = sandbox
        .run(Some("all"), Some("whitaker"))
        .expect("run failing action");
    assert!(!output.status.success(), "all unexpectedly succeeded");
    assert_eq!(
        sandbox.commands().expect("read fake-tool log"),
        [
            "check-build-tools.sh\t--toolchain-only",
            "cargo\tfmt\t--all\t--\t--check",
            concat!(
                "mdtablefix\t--check\t--git\t--include-untracked\t--wrap\t--renumber\t--breaks",
                "\t--ellipsis\t--fences",
            ),
            "check-build-tools.sh",
            "cargo\tdoc\t--no-deps",
            "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
            "whitaker\t--all\t--\t--all-targets\t--all-features",
        ]
    );
}

/// Check warning policy and linker flags at their command boundaries.
#[rstest]
#[case::typecheck(
    "typecheck",
    "cargo\t-D warnings -Zthreads=8 -Zcodegen-backend=cranelift -Clink-arg=-fuse-ld=mold\t\t\t\t"
)]
#[case::coverage(
    "coverage",
    "cargo\t-D warnings -C link-arg=-fuse-ld=lld\t\t-fuse-ld=lld\t-fuse-ld=lld\tclang"
)]
#[case::spelling("spelling", "uv\t\t\t\t\t")]
fn action_sets_expected_environment(#[case] action: &str, #[case] expected: &str) {
    let sandbox = ActionSandbox::new(false).expect("create isolated Netsuke sandbox");
    let output = sandbox.run(Some(action), None).expect("run Netsuke action");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let environment = sandbox.environment().expect("read environment log");
    assert_eq!(environment.last().map(String::as_str), Some(expected));
}

/// Rust linting must deny warnings in both rustdoc and Whitaker.
#[test]
fn rust_lint_sets_warning_policy() {
    let sandbox = ActionSandbox::new(false).expect("create isolated Netsuke sandbox");
    let output = sandbox
        .run(Some("rust-lint"), None)
        .expect("run Rust lint action");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        sandbox.environment().expect("read environment log"),
        [
            "cargo\t-Zthreads=8 -Zcodegen-backend=cranelift -Clink-arg=-fuse-ld=mold\t-D \
             warnings\t\t\t",
            "cargo\t-Zthreads=8 -Zcodegen-backend=cranelift -Clink-arg=-fuse-ld=mold\t\t\t\t",
            "whitaker\t-D warnings\t\t\t\t",
        ]
    );
}
