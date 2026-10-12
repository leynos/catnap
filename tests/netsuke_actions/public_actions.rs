//! Exercises every public Netsuke action and its failure contract.

use rstest::rstest;

use super::ActionSandbox;

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
#[case::yamllint("yamllint", &["yamllint\t--strict\t--\tfixture.yaml\tfixture.yml"])]
#[case::actionlint("actionlint", &["actionlint"])]
#[case::github_actions_lint("github-actions-lint", &[
    "yamllint\t--strict\t--\tfixture.yaml\tfixture.yml",
    "actionlint",
])]
#[case::lint("lint", &[
    "check-build-tools.sh",
    "cargo\tdoc\t--no-deps",
    "cargo\tclippy\t--all-targets\t--all-features\t--\t-D\twarnings",
    "whitaker\t--all\t--\t--all-targets\t--all-features",
    "yamllint\t--strict\t--\tfixture.yaml\tfixture.yml",
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
    "uv\ttool\trun\t--python\t3.13\t--from\tgit+https://github.com/leynos/shared-actions@88977798a5c3bae1549afb99642529488c665276#subdirectory=packages/cv005-contracts\tcv005-contracts\tcheck\t--repository\t.",
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
#[case::yamllint("yamllint", "yamllint")]
#[case::actionlint("actionlint", "actionlint")]
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
