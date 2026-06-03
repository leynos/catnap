# Developer Guide

This guide explains the contributor workflow for the `catnap` command.

## Local Workflow

Use `make all` as the public entrypoint for formatting, linting, and tests.
`make lint` runs rustdoc, Clippy, and Whitaker. `make test` prefers
`cargo nextest run` and falls back to `cargo test` when cargo-nextest is not
available. `make coverage` uses `cargo llvm-cov` with `lld`.

## Tooling

Development builds use Cranelift for debug code generation. On Linux targets,
`.cargo/config.toml` configures clang to link with `mold` so debug builds link
quickly. Coverage generation uses `lld` because LLVM coverage tooling expects
LLVM-compatible linker behaviour.

Install `clang`, `lld`, and `mold` before running the full generated workflow
locally on Linux.

## Implementation Boundaries

The binary entry point in `src/main.rs` only wires process streams and command
arguments into the library. Command parsing, duration parsing, locale-aware
remaining-time formatting, monotonic clock handling, and sleep orchestration
live in `src/lib.rs` and its sibling modules.

The runner depends on the `MonotonicClock` trait rather than calling
`std::time::Instant` directly. Production code uses `RealMonotonicClock`; tests
use `mockall` to verify runner behaviour with deterministic monotonic time.

End-to-end tests use the hidden `--logical-second-ms` argument to shorten one
logical second to a small real duration. This argument is private test support:
it is intentionally omitted from normal help output and must not be documented
as a user-facing option.

## Test Layout

The test suite covers the same behaviour from several angles:

- Unit tests in `src/duration.rs`, `src/format.rs`, and `src/runner.rs` cover
  parsing, cadence selection, locale formatting, and mock-clock orchestration.
- Behavioural tests in `tests/behaviour.rs` use `rstest-bdd` scenarios from
  `tests/features/sleep_cli.feature`.
- Snapshot tests in `tests/snapshots.rs` pin representative remaining-time
  output.
- End-to-end tests in `tests/e2e.rs` build and run the compiled binary with
  accelerated logical seconds.
