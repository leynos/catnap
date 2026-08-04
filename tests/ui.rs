//! Executable UI checks for the public error types.

#[test]
fn public_error_display_output() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/*_display.rs");
}
