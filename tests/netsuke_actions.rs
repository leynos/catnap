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
    "markdownlint-cli2",
    "uv",
    "nixie",
];

/// Keep each generated Ninja graph and command log private to one test.
struct ActionSandbox {
    directory: Dir,
    temporary_directory: TempDir,
    netsuke: PathBuf,
}

impl ActionSandbox {
    /// Prepare fake commands while retaining the real Netsuke executable.
    fn new(with_nextest: bool) -> Result<Self, Box<dyn Error>> {
        let temporary_directory = tempfile::tempdir()?;
        let directory = Dir::open_ambient_dir(temporary_directory.path(), ambient_authority())?;
        directory.write("Netsukefile", MANIFEST)?;
        directory.write("README.md", "# Sandbox\n")?;
        for tool in FAKE_TOOLS {
            write_fake_tool(&directory, tool)?;
        }
        if with_nextest {
            write_fake_tool(&directory, "cargo-nextest")?;
        }
        Ok(Self {
            directory,
            temporary_directory,
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
                    self.temporary_directory.path(),
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
#[case::build("build", &["cargo\tbuild\t--bin\tcatnap"])]
#[case::release("release", &["cargo\tbuild\t--release\t--bin\tcatnap"])]
#[case::coverage("coverage", &["cargo\tllvm-cov\t--lcov\t--output-path\tlcov.info\t--all-targets\t--all-features"])]
#[case::typecheck("typecheck", &["cargo\tcheck\t--all-targets\t--all-features"])]
#[case::fmt("fmt", &[
    "cargo\t+nightly\tfmt\t--all",
    "mdtablefix\t--in-place\t--git\t--include-untracked\t--wrap\t--renumber\t--breaks\t--ellipsis\t--fences",
    "markdownlint-cli2\t--fix\t**/*.md",
])]
#[case::check_fmt("check-fmt", &[
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
    "cargo\tdoc\t--no-deps",
    "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
    "whitaker\t--all\t--\t--all-targets\t--all-features",
    "yamllint\t.github/workflows",
    "actionlint",
])]
#[case::markdownlint("markdownlint", &[
    "uv\ttool\trun\t--python\t3.14\t--from\tgit+https://github.com/leynos/typos-config-builder.git@v0.1.1\ttypos-config-builder\tgate\t--repository\t.",
    "markdownlint-cli2\t./README.md",
])]
#[case::spelling("spelling", &[
    "uv\ttool\trun\t--python\t3.14\t--from\tgit+https://github.com/leynos/typos-config-builder.git@v0.1.1\ttypos-config-builder\tgate\t--repository\t.",
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
#[case::nixie("nixie", "nixie")]
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
#[case::nextest(true, "cargo\tnextest\trun\t--all-targets\t--all-features")]
#[case::cargo(false, "cargo\ttest\t--all-targets\t--all-features")]
fn test_action_selects_available_runner(#[case] with_nextest: bool, #[case] expected: &str) {
    let sandbox = ActionSandbox::new(with_nextest).expect("create isolated Netsuke sandbox");
    let output = sandbox.run(Some("test"), None).expect("run test action");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(sandbox.commands().expect("read fake-tool log"), [expected]);
    assert_eq!(
        sandbox.environment().expect("read environment log"),
        ["cargo\t-D warnings\t\t\t\t"]
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
        "cargo\tfmt\t--all\t--\t--check",
        "mdtablefix\t--check\t--git\t--include-untracked\t--wrap\t--renumber\t--breaks\t--ellipsis\t--fences",
        "cargo\tdoc\t--no-deps",
        "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
        "whitaker\t--all\t--\t--all-targets\t--all-features",
        "yamllint\t.github/workflows",
        "actionlint",
        "cargo\tnextest\trun\t--all-targets\t--all-features",
        "uv\ttool\trun\t--python\t3.14\t--from\tgit+https://github.com/leynos/typos-config-builder.git@v0.1.1\ttypos-config-builder\tgate\t--repository\t.",
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
            "cargo\tfmt\t--all\t--\t--check",
            concat!(
                "mdtablefix\t--check\t--git\t--include-untracked\t--wrap\t--renumber\t--breaks",
                "\t--ellipsis\t--fences",
            ),
            "cargo\tdoc\t--no-deps",
            "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
            "whitaker\t--all\t--\t--all-targets\t--all-features",
        ]
    );
}

/// Check warning policy and linker flags at their command boundaries.
#[rstest]
#[case::typecheck("typecheck", "cargo\t-D warnings\t\t\t\t")]
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
    assert_eq!(
        sandbox.environment().expect("read environment log"),
        [expected]
    );
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
            "cargo\t\t-D warnings\t\t\t",
            "cargo\t\t\t\t\t",
            "whitaker\t-D warnings\t\t\t\t",
        ]
    );
}
