//! UI checks for the public error types.
//!
//! Two `trybuild` modes cover complementary guarantees. Pass fixtures compile
//! and run as external crates so they can observe formatted `Display` output,
//! which Rust only evaluates at runtime. Compile-fail fixtures snapshot the
//! compiler diagnostics that keep the public error enums non-exhaustive.

// Coverage instruments the parent test crate with LLVM. trybuild starts
// nested Cargo builds that reload Cranelift from `.cargo/config.toml`; that
// backend cannot accept LLVM coverage flags. CI runs these tests separately
// with `make test-ui` before its coverage step.
/// Compiles and runs every display fixture, pinning public error message text.
#[cfg_attr(
    coverage,
    ignore = "trybuild subprocesses cannot use LLVM coverage with Cranelift"
)]
#[test]
fn public_error_display_output() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/*_display.rs");
}

/// Compiles every non-exhaustive fixture, pinning the public matching contract.
#[cfg_attr(
    coverage,
    ignore = "trybuild subprocesses cannot use LLVM coverage with Cranelift"
)]
#[test]
fn public_error_non_exhaustive_matching() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*_non_exhaustive.rs");
}
