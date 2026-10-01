//! Contracts for installing the pinned linker and Rust toolchain.

use std::error::Error;

use rstest::rstest;

use super::support::{BuildToolsSandbox, build_tools_sandbox};

const PINNED_TOOLCHAIN: &str = "nightly-2026-05-28";
const FIXTURE_ARCHIVE_SHA256: &str =
    "b82d14bd3717287c78a2e1351107a49a925192cae59c0f844437eed8a0d6caef";

#[cfg(target_os = "linux")]
#[rstest]
fn installer_passes_the_pinned_toolchain_components_to_rustup(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_uname("Darwin")
        .expect("write non-Linux uname");
    sandbox
        .write_recording_rustup()
        .expect("write recording rustup");

    let output = sandbox
        .run_make(&["install-build-tools"])
        .expect("run make install-build-tools");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "installer failed: {stderr}");
    assert_eq!(
        sandbox.rustup_invocations().expect("read rustup log"),
        format!(
            concat!(
                "toolchain\n",
                "install\n",
                "{}\n",
                "--profile\n",
                "minimal\n",
                "--component\n",
                "clippy,llvm-tools-preview,rust-analyzer,rustc-codegen-cranelift-preview,rustfmt\n"
            ),
            PINNED_TOOLCHAIN
        )
    );
}

#[rstest]
fn make_installs_only_the_pinned_cranelift_component(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_recording_rustup()
        .expect("write recording rustup");

    let output = sandbox
        .run_make(&["install-cranelift"])
        .expect("run make install-cranelift");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "component installation failed: {stderr}"
    );
    assert_eq!(
        sandbox.rustup_invocations().expect("read rustup log"),
        format!(
            "component\nadd\nrustc-codegen-cranelift-preview\n--toolchain\n{PINNED_TOOLCHAIN}\n"
        )
    );
    assert_eq!(
        sandbox.install_order().expect("read install order"),
        "rustup\n",
        "the CI component route should not download or replace mold"
    );
}

#[cfg(target_os = "linux")]
#[rstest]
fn installer_verifies_the_mold_archive_before_unpacking(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_fake_mold_download_tools()
        .expect("write fake download tools");
    sandbox
        .write_recording_rustup()
        .expect("write recording rustup");

    let output = sandbox
        .run_installer_with_checksum(FIXTURE_ARCHIVE_SHA256)
        .expect("run installer with valid checksum");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "installer failed: {stderr}");
    assert_eq!(
        sandbox.install_order().expect("read install order"),
        "curl\ntar\nrustup\n",
        "the installer should verify the download before unpacking it"
    );
}

#[cfg(target_os = "linux")]
#[rstest]
fn installer_refuses_a_mold_archive_with_the_wrong_checksum(
    build_tools_sandbox: Result<BuildToolsSandbox, Box<dyn Error>>,
) {
    let sandbox = build_tools_sandbox.expect("create build-tools sandbox");
    sandbox
        .write_fake_mold_download_tools()
        .expect("write fake download tools");
    sandbox
        .write_recording_rustup()
        .expect("write recording rustup");

    let output = sandbox
        .run_installer_with_checksum(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .expect("run installer with wrong checksum");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "installer accepted a wrong checksum"
    );
    assert!(
        stderr.contains("checksum mismatch"),
        "failure did not identify the mismatch: {stderr}"
    );
    assert_eq!(
        sandbox.install_order().expect("read install order"),
        "curl\n",
        "a failed checksum must stop unpacking and toolchain installation"
    );
    assert_eq!(
        sandbox.rustup_invocations().expect("read rustup log"),
        "",
        "rustup ran after the archive checksum failed"
    );
}
