//! Consumer contracts for the pinned local build-tool checks and Netsuke routes.

use std::error::Error;

use rstest::rstest;

#[path = "build_tools_contract/components.rs"]
mod components;
#[path = "build_tools_contract/installer.rs"]
mod installer;
#[path = "build_tools_contract/support.rs"]
mod support;

use support::{BuildToolsSandbox, build_tools_sandbox};

const PINNED_MOLD_VERSION: &str = "2.41.0";
const PINNED_TOOLCHAIN: &str = "nightly-2026-05-28";

#[cfg(target_os = "linux")]
#[rstest]
fn pinned_prefix_mold_wins_over_an_earlier_wrong_version(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_mold("prefix/bin/mold", PINNED_MOLD_VERSION)
        .expect("write pinned prefix mold");
    sandbox
        .write_mold("incoming-bin/mold", "0.1.0")
        .expect("write earlier wrong-version mold");
    sandbox.write_clang().expect("write fake clang");
    sandbox
        .write_rustup(PINNED_TOOLCHAIN)
        .expect("write fake rustup");

    let output = sandbox
        .run_netsuke(&["check-build-tools"])
        .expect("run netsuke build check-build-tools");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(
        output.status.success(),
        "build-tools check failed: {stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "mold {PINNED_MOLD_VERSION} at {}",
            sandbox.prefix_bin.join("mold").display()
        )),
        "checker did not report the pinned prefix mold: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[rstest]
fn wrong_prefix_mold_stops_build_with_install_hint_before_cargo(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_mold("prefix/bin/mold", "0.1.0")
        .expect("write wrong-version prefix mold");
    sandbox
        .write_mold("incoming-bin/mold", PINNED_MOLD_VERSION)
        .expect("write later pinned mold");
    sandbox.write_clang().expect("write fake clang");
    sandbox
        .write_rustup(PINNED_TOOLCHAIN)
        .expect("write fake rustup");

    let output = sandbox
        .run_netsuke(&["build"])
        .expect("run netsuke build build");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(!output.status.success(), "build unexpectedly succeeded");
    assert!(
        stderr.contains(&format!(
            "mold 0.1.0 at {} does not match the pin {PINNED_MOLD_VERSION}",
            sandbox.prefix_bin.join("mold").display()
        )),
        "checker did not reject the prefix mold: {stderr}"
    );
    assert!(
        stderr.contains("run netsuke build install-build-tools to match"),
        "failure did not explain how to install the pinned tools: {stderr}"
    );
    assert_eq!(
        sandbox.cargo_invocations().expect("read fake Cargo log"),
        "",
        "Cargo ran despite the failed prerequisite"
    );
}

#[rstest]
fn missing_pinned_toolchain_reports_install_hint(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_mold("prefix/bin/mold", PINNED_MOLD_VERSION)
        .expect("write pinned prefix mold");
    sandbox.write_clang().expect("write fake clang");
    sandbox
        .write_rustup("stable-x86_64-unknown-linux-gnu")
        .expect("write fake rustup without the pinned nightly");

    let output = sandbox
        .run_netsuke(&["check-build-tools"])
        .expect("run netsuke build check-build-tools");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(!output.status.success(), "check unexpectedly succeeded");
    assert!(
        stderr.contains(&format!("toolchain {PINNED_TOOLCHAIN} is not installed")),
        "checker did not identify the missing nightly: {stderr}"
    );
    assert!(
        stderr.contains("install it with: netsuke build install-build-tools"),
        "failure did not explain how to install the pinned toolchain: {stderr}"
    );
}

#[rstest]
fn missing_pinned_toolchain_component_reports_install_hint(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_mold("prefix/bin/mold", PINNED_MOLD_VERSION)
        .expect("write pinned prefix mold");
    sandbox.write_clang().expect("write fake clang");
    sandbox
        .write_rustup_with_components(
            PINNED_TOOLCHAIN,
            concat!(
                "clippy-x86_64-unknown-linux-gnu\n",
                "llvm-tools-x86_64-unknown-linux-gnu\n",
                "rust-analyzer-x86_64-unknown-linux-gnu\n"
            ),
        )
        .expect("write fake rustup without rustfmt");

    let output = sandbox
        .run_netsuke(&["check-build-tools"])
        .expect("run netsuke build check-build-tools");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(!output.status.success(), "check unexpectedly succeeded");
    assert!(
        stderr.contains(&format!(
            "component rustfmt is not installed for {PINNED_TOOLCHAIN}"
        )),
        "checker did not identify the missing rustfmt component: {stderr}"
    );
    assert!(
        stderr.contains("install it with: netsuke build install-build-tools"),
        "failure did not explain how to install the pinned tools: {stderr}"
    );
}

#[rstest]
fn toolchain_check_does_not_require_local_linkers(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_rustup(PINNED_TOOLCHAIN)
        .expect("write fake rustup");

    let output = sandbox
        .run_netsuke(&["check-rust-toolchain"])
        .expect("run netsuke build check-rust-toolchain");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(output.status.success(), "toolchain check failed: {stderr}");
    assert!(stderr.contains(&format!("toolchain {PINNED_TOOLCHAIN} available")));
    assert!(
        !stderr.contains("mold at"),
        "toolchain check looked for mold"
    );
    assert!(
        !stderr.contains("clang driver"),
        "toolchain check looked for clang"
    );
}

#[rstest]
fn toolchain_checker_rejects_extra_arguments(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    let output = sandbox
        .run_checker(&["--toolchain-only", "unexpected"])
        .expect("run build-tools checker");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(
        !output.status.success(),
        "checker accepted an extra argument"
    );
    assert!(
        stderr.contains("usage:"),
        "checker omitted usage on failure: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[rstest]
fn coverage_check_rejects_unusable_lld_before_cargo(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox.write_clang().expect("write fake clang");
    sandbox.write_lld().expect("write fake lld");
    sandbox
        .write_rustup(PINNED_TOOLCHAIN)
        .expect("write fake rustup");

    let output = sandbox
        .run_netsuke(&["coverage"])
        .expect("run netsuke build coverage");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(
        !output.status.success(),
        "coverage unexpectedly passed with a broken lld"
    );
    assert!(
        stderr.contains("lld linker at") && stderr.contains("cannot report its version"),
        "broken lld was not reported: {stderr}"
    );
    assert_eq!(
        sandbox.cargo_invocations().expect("read fake Cargo log"),
        "",
        "Cargo ran despite the missing coverage linker"
    );
}

#[cfg(target_os = "linux")]
#[rstest]
fn coverage_check_rejects_unusable_clang_before_cargo(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox.write_broken_clang().expect("write broken clang");
    sandbox.write_usable_lld().expect("write usable lld");
    sandbox
        .write_rustup(PINNED_TOOLCHAIN)
        .expect("write fake rustup");

    let output = sandbox
        .run_netsuke(&["coverage"])
        .expect("run netsuke build coverage");
    let stderr = BuildToolsSandbox::diagnostics(&output);

    assert!(
        !output.status.success(),
        "coverage unexpectedly passed with a broken clang"
    );
    assert!(
        stderr.contains("clang driver at") && stderr.contains("cannot report its version"),
        "broken clang was not reported: {stderr}"
    );
    assert_eq!(
        sandbox.cargo_invocations().expect("read fake Cargo log"),
        "",
        "Cargo ran despite the missing coverage driver"
    );
}

#[rstest]
fn netsuke_build_routes_check_tools_before_cargo_and_exclude_release_routes(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");

    for target in ["build", "test", "typecheck"] {
        let output = sandbox.run_plan(target).expect("generate Netsuke plan");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "Netsuke plan for {target} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let check_position = stdout
            .find("scripts/check-build-tools.sh")
            .unwrap_or_else(|| panic!("Netsuke {target} omitted the tool check: {stdout}"));
        let cargo_position = stdout
            .find("cargo ")
            .unwrap_or_else(|| panic!("Netsuke {target} omitted Cargo: {stdout}"));
        assert!(
            check_position < cargo_position,
            "Netsuke {target} places Cargo before the tool check: {stdout}"
        );
    }

    for target in ["fmt", "check-fmt"] {
        let output = sandbox.run_plan(target).expect("generate Netsuke plan");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "Netsuke plan for {target} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let check_position = stdout
            .find("scripts/check-build-tools.sh --toolchain-only")
            .unwrap_or_else(|| panic!("Netsuke {target} omitted the toolchain check: {stdout}"));
        let cargo_position = stdout
            .find("cargo ")
            .unwrap_or_else(|| panic!("Netsuke {target} omitted Cargo: {stdout}"));
        assert!(
            check_position < cargo_position,
            "Netsuke {target} places Cargo before the toolchain check: {stdout}"
        );
    }

    let target = "release";
    let output = sandbox.run_plan(target).expect("generate Netsuke plan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Netsuke plan for {target} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !stdout.contains("scripts/check-build-tools.sh"),
        "Netsuke {target} unexpectedly depends on the local tool check: {stdout}"
    );
    assert!(
        stdout.contains("cargo +stable build"),
        "Netsuke {target} omitted stable Cargo: {stdout}"
    );
    assert_eq!(
        sandbox.cargo_invocations().expect("read fake Cargo log"),
        "",
        "Netsuke plan inspection executed a Cargo command"
    );
}

#[rstest]
fn netsuke_coverage_route_checks_linkers_before_cargo(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    let output = sandbox.run_plan("coverage").expect("generate Netsuke plan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Netsuke coverage plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let check_position = stdout
        .find("scripts/check-build-tools.sh --coverage-only")
        .unwrap_or_else(|| panic!("Netsuke coverage omitted the linker check: {stdout}"));
    let cargo_position = stdout
        .find("cargo llvm-cov")
        .unwrap_or_else(|| panic!("Netsuke coverage omitted Cargo: {stdout}"));
    assert!(
        check_position < cargo_position,
        "Netsuke coverage places Cargo before the linker check: {stdout}"
    );
}
