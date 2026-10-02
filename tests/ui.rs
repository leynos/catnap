//! UI checks for the public error types.
//!
//! Two `trybuild` modes cover complementary guarantees. Pass fixtures compile
//! and run as external crates so they can observe formatted `Display` output,
//! which Rust only evaluates at runtime. Compile-fail fixtures snapshot the
//! compiler diagnostics that keep the public error enums non-exhaustive.

/// Compiles and runs every display fixture, pinning public error message text.
#[test]
fn public_error_display_output() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/*_display.rs");
}

/// Compiles the public runner contract from an external crate.
#[test]
fn public_runner_contract() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/runner_contract.rs");
}

/// Compiles every non-exhaustive fixture, pinning the public matching contract.
#[test]
fn public_error_non_exhaustive_matching() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*_non_exhaustive.rs");
}
