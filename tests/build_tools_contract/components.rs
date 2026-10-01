//! Covers Cranelift component checking and the LLVM coverage exception.

use std::error::Error;

use rstest::rstest;

use super::{
    PINNED_MOLD_VERSION,
    PINNED_TOOLCHAIN,
    support::{BuildToolsSandbox, build_tools_sandbox},
};

#[rstest]
fn missing_cranelift_component_reports_install_hint(
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
                "rustfmt-x86_64-unknown-linux-gnu\n"
            ),
        )
        .expect("write fake rustup without Cranelift");

    let output = sandbox
        .run_make(&["check-build-tools"])
        .expect("run make check-build-tools");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "check unexpectedly succeeded");
    assert!(
        stderr.contains(&format!(
            "component rustc-codegen-cranelift-preview is not installed for {PINNED_TOOLCHAIN}"
        )),
        "checker did not identify the missing Cranelift component: {stderr}"
    );
    assert!(
        stderr.contains("install it with: make install-build-tools"),
        "failure did not explain how to install the pinned toolchain: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[rstest]
fn coverage_prerequisites_do_not_require_the_development_backend(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox.write_clang().expect("write fake clang");
    sandbox.write_usable_lld().expect("write fake lld");
    sandbox
        .write_rustup_with_components(
            PINNED_TOOLCHAIN,
            concat!(
                "clippy-x86_64-unknown-linux-gnu\n",
                "llvm-tools-x86_64-unknown-linux-gnu\n",
                "rustfmt-x86_64-unknown-linux-gnu\n"
            ),
        )
        .expect("write fake rustup without Cranelift");

    let output = sandbox
        .run_make(&["check-coverage-tools"])
        .expect("run make check-coverage-tools");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "coverage check failed: {stderr}");
}
