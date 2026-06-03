# Implement a monotonic visual sleep command

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
 `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`,
and `Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

## Purpose / big picture

The repository currently contains only generated starter code. This plan turns
`vsleep` into a Rust command-line application modelled on GNU `sleep`, while
adding progress output: for requested sleeps longer than one minute, the
program prints the remaining time every thirty seconds; for requested sleeps up
to one minute, it prints every five seconds; and for requested sleeps up to
twenty seconds, it prints every second. The sleep timing must be driven by a
monotonic stopwatch so that wall-clock jumps do not shorten or extend a run.

A user can observe success by running commands such as `vsleep 2`, `vsleep 1m`,
and `vsleep 1 5s`. The command exits successfully after the requested duration,
reports invalid operands with GNU-like diagnostics, and prints remaining-time
progress to standard error without mixing it into standard output.

## Constraints

The implementation must obey the repository `AGENTS.md` instructions. Every
source module must start with a module-level Rustdoc comment. Public APIs must
have Rustdoc comments. The crate's strict lint policy denies panics, unchecked
indexing, direct standard output and standard error printing macros, missing
docs, and many Clippy restriction lints.

The command-line user experience must use GNU `sleep` as its model. The public
syntax is one or more `NUMBER[SUFFIX]` operands. Supported suffixes are `s`,
`m`, `h`, and `d`, matching seconds, minutes, hours, and days. Multiple
operands are summed. The application must reject missing operands, invalid
numbers, invalid suffixes, negative durations, and non-finite values.

The progress scheduler must use the full requested duration to choose the print
cadence: greater than sixty seconds prints every thirty seconds; sixty seconds
or less prints every five seconds; twenty seconds or less prints every second.
Remaining time is printed using the current environment locale. The locale must
be captured through a small formatting boundary so tests can verify the
selected language without relying on host locale state.

Timing must use dependency injection. Core sleep orchestration receives a
monotonic clock trait. Tests use `mockall` to mock that trait. The binary uses
a real monotonic clock backed by `std::time::Instant`. End-to-end tests must be
able to run quickly through a private, hidden argument that changes the real
duration of one logical second.

The requested tests are mandatory: unit tests with `rstest`, behavioural tests
with `rstest-bdd`, snapshot tests with `insta`, and end-to-end tests. The
generated stub test must be removed once real tests exist.

All gates must be run sequentially through Makefile targets with output
captured through `tee` under `/tmp`. At minimum, commit gates are
`make check-fmt`, `make lint`, and `make test`; Markdown changes additionally
require `make markdownlint` and Mermaid validation through `make nixie` when
diagrams are present.

## Tolerances

Escalate before proceeding if satisfying GNU-like parsing requires supporting
syntax beyond `NUMBER[SUFFIX]`, if locale-aware formatting requires a very
large internationalisation framework instead of a small dependency, or if the
private e2e acceleration argument would be visible in normal help output.

Escalate if any source file would exceed four hundred lines after the change,
if the full gate suite cannot run because of a missing external tool, or if a
gate failure is unrelated existing debt rather than introduced behaviour.

Escalate if any required test class cannot be implemented with the chosen crate
versions after consulting their crate documentation.

## Risks

Locale formatting can become larger than the application if it attempts to
translate every possible phrase. The mitigation is to keep a focused formatter
that localizes the language names needed by tests and uses environment locale
selection through an injected value.

Floating-point duration parsing can introduce rounding surprises. The
mitigation is to parse operands as finite `f64`, reject negative values, and
convert to nanoseconds only after multiplying by the suffix scale.

Long-running e2e tests can make the suite slow. The mitigation is to keep the
normal public interface unchanged and use a hidden second-duration argument for
black-box tests.

Strict lints can make ordinary test helpers fail. The mitigation is to keep
test helpers small, return `Result` where useful, and place assertions inside
test bodies rather than shared production paths.

## Progress

- [x] 2026-06-01: Read repository instructions, Cargo metadata, Makefile
  gates, starter code, and documentation index.
- [x] 2026-06-01: Confirmed the current branch is `initial-app`, not `main`.
- [x] 2026-06-01: Created this living ExecPlan at
  `docs/execplans/initial-app.md`.
- [x] 2026-06-01: Validated the planning change with `make markdownlint` and
  `make check-fmt`.
- [x] 2026-06-01: Established baseline validation evidence for the generated
  starter state with `make lint` and `make test`.
- [x] 2026-06-01: Added dependencies needed for typed errors, locale discovery,
  deterministic tests, behavioural tests, snapshots, and e2e tests.
- [x] 2026-06-01: Implemented the duration parser, progress cadence selection,
  locale-aware remaining-time formatter, injected monotonic clock, real clock,
  and binary wiring.
- [x] 2026-06-01: Added unit tests using `rstest` and `mockall`.
- [x] 2026-06-01: Added behavioural tests using `rstest-bdd`.
- [x] 2026-06-01: Added snapshot tests using `insta`.
- [x] 2026-06-01: Added end-to-end tests that use the hidden logical-second
  duration argument.
- [x] 2026-06-01: `make test` passed with 25 tests.
- [x] 2026-06-01: `make lint` passed, including rustdoc, Clippy, and Whitaker.
- [x] 2026-06-01: Updated user and developer documentation for the shipped
  command and test layout.
- [x] 2026-06-01: Final gates passed: `make check-fmt`, `make lint`,
  `make test`, `make markdownlint`, and `make nixie`.
- [x] 2026-06-01: Committed the validated implementation as `10273a6`.
- [x] 2026-06-01: Completion-audited the objective against the current tree.

Completed implementation checklist:

- [x] Implement the duration parser, progress cadence selection, locale-aware
  remaining-time formatter, injected monotonic clock, real clock, and binary
  wiring.
- [x] Add unit tests using `rstest` and `mockall`.
- [x] Add behavioural tests using `rstest-bdd`.
- [x] Add snapshot tests using `insta`.
- [x] Add end-to-end tests that use the hidden logical-second duration
  argument.

## Surprises & Discoveries

`grepai` had no indexed matches for the requested stopwatch behaviour, which
matches the generated starter state: only `src/main.rs` and `tests/stub.rs`
exist as Rust code.

The Makefile runs `cargo nextest run` for `make test` when `cargo-nextest` is
installed, otherwise it falls back to `cargo test`. The implementation must
therefore keep all tests compatible with Cargo's ordinary integration-test
model.

`cargo add rstest-bdd` selected the latest stable release, `0.5.0`, rather than
the published `0.6.0-beta1` pre-release. That satisfies the requirement to use
`rstest-bdd` without pinning the project to a beta API.

`cargo nextest run --all-targets` does not provide `CARGO_BIN_EXE_vsleep` or
build the binary in this configured split target/build directory. The e2e test
therefore builds `vsleep` explicitly and reads `target_directory` from
`cargo metadata` before running the compiled binary.

## Decision Log

2026-06-01: Use `src/lib.rs` for reusable command, parsing, formatting, and
clock logic, and keep `src/main.rs` as a thin process boundary. This makes
unit, behavioural, snapshot, and e2e tests easier without putting business
logic in `main`.

2026-06-01: Print progress to standard error rather than standard output. GNU
`sleep` itself has no progress output, and CLI progress belongs on standard
error so standard output remains available for scripts.

2026-06-01: Keep the e2e acceleration argument hidden and private. It is a
testing seam, not a user-facing option, and normal help output should not
advertize it.

2026-06-01: Avoid a full CLI framework for the first implementation. The public
syntax is intentionally small, and handwritten parsing keeps the GNU-like
operand contract explicit while avoiding an unnecessary abstraction.

## Implementation Plan

First, establish and record the starter-state gate evidence. Run the Makefile
targets sequentially with `tee`, using log paths under `/tmp` such as
`/tmp/check-fmt-vsleep-initial-app.out`.

Second, add the dependencies required by the design in `Cargo.toml`. The
runtime dependencies are `thiserror` for typed library errors and `sys-locale`
for current-locale discovery. The dev-dependencies are `rstest`, `mockall`,
`assert_cmd`, `predicates`, `insta`, `rstest-bdd`, `rstest-bdd-macros`, and
`serde_json`.

Third, add `src/lib.rs` and small feature modules. The parser module turns
operands into a requested logical duration. The scheduler module selects the
progress interval. The format module renders remaining time for a locale. The
clock module defines the monotonic clock trait and real implementation. The
runner module coordinates sleeping, ticking, output, and exit behaviour.

Fourth, replace `src/main.rs` with a thin binary boundary. It parses CLI
arguments, constructs the real clock using the hidden logical-second duration,
gets the environment locale, invokes the runner, writes errors to standard
error using `std::io::Write`, and exits with the appropriate code.

Fifth, replace `tests/stub.rs` with real integration coverage. Unit tests live
close to the modules they test and use `rstest` and `mockall`. Behavioural
tests use `rstest-bdd` feature files or scenarios to describe GNU-like operand
handling and progress cadence. Snapshot tests use `insta` for representative
progress output in stable locales. End-to-end tests invoke the compiled binary
with `assert_cmd` and the hidden acceleration option.

Sixth, update `docs/users-guide.md`, `docs/developers-guide.md`, and
`docs/repository-layout.md` to describe the real command, hidden test-only
surface, and new test files. Update `docs/contents.md` if a new documentation
file is added.

Seventh, run the full gate sequence. Fix issues in the code rather than
weakening lints. Commit only after `make check-fmt`, `make lint`, and
`make test` pass for code, plus Markdown gates for documentation changes.

## Validation

Before each commit, run the applicable Makefile targets sequentially through
`tee`. The expected final gate commands are:

```sh
make check-fmt 2>&1 | tee /tmp/check-fmt-vsleep-initial-app.out
make lint 2>&1 | tee /tmp/lint-vsleep-initial-app.out
make test 2>&1 | tee /tmp/test-vsleep-initial-app.out
make markdownlint 2>&1 | tee /tmp/markdownlint-vsleep-initial-app.out
make nixie 2>&1 | tee /tmp/nixie-vsleep-initial-app.out
```

The expected final manual behaviour checks are:

```sh
cargo run -- --help
cargo run -- 2
cargo run -- 1m 5s
cargo run -- --logical-second-ms 10 2
```

Normal help must describe the public GNU-like operands while omitting the
hidden e2e-only argument. The accelerated invocation must complete quickly and
print countdown progress on standard error.

## Outcomes & Retrospective

The implementation now ships a GNU-like `vsleep` binary that parses
`NUMBER[SUFFIX]...` operands, uses a monotonic clock, prints localized
remaining-time progress to standard error, keeps standard output empty, and has
the requested unit, behavioural, snapshot, and end-to-end test coverage.

Completion audit on 2026-06-01:

- Plan requested: satisfied by this ExecPlan and commits `aed5a25`, `fd94ae8`,
  and `10273a6`.
- Rust version of `sleep`: satisfied by `src/main.rs`, `src/lib.rs`, and
  `src/cli.rs`, with help and invalid-operand behaviour verified by
  `tests/e2e.rs`.
- Monotonic stopwatch: satisfied by `MonotonicClock`, `MonotonicTimestamp`,
  and `RealMonotonicClock` in `src/clock.rs`, with runner dependency injection
  in `src/runner.rs`.
- Remaining-time cadence: satisfied by `report_interval` in `src/duration.rs`
  and unit tests covering greater than one minute, one minute or less, and
  twenty seconds or less.
- Locale formatting: satisfied by `format_remaining_time` in `src/format.rs`
  and tests for English, French, and fallback locales.
- Required tests: satisfied by `rstest` unit tests, `mockall` runner tests,
  `rstest-bdd` scenarios, `insta` snapshots, and accelerated e2e tests.
- Hidden e2e second duration: satisfied by `--logical-second-ms` in
  `src/cli.rs`, verified by e2e tests and manual help output showing the option
  is omitted.
- GNU sleep UX model: satisfied by `NUMBER[SUFFIX]...`, multiple operand
  summing, `--help`, `--version`, missing-operand errors, invalid suffix
  errors, and standard-error diagnostics.
- Gates: satisfied by final cleaned-tree runs of `make check-fmt`,
  `make lint`, `make test`, `make markdownlint`, and `make nixie`.

Planning validation passed on 2026-06-01:

```plaintext
make markdownlint 2>&1 | tee /tmp/markdownlint-vsleep-initial-app.out
Summary: 0 error(s)

make check-fmt 2>&1 | tee /tmp/check-fmt-vsleep-initial-app.out
cargo fmt --all -- --check
```

Baseline validation passed on 2026-06-01:

```plaintext
make lint 2>&1 | tee /tmp/lint-vsleep-initial-app-baseline.out
Finished `dev` profile

make test 2>&1 | tee /tmp/test-vsleep-initial-app-baseline.out
Summary [   0.006s] 1 test run: 1 passed, 0 skipped
```

Dependency validation passed on 2026-06-01:

```plaintext
make check-fmt 2>&1 | tee /tmp/check-fmt-vsleep-initial-app-deps.out
cargo fmt --all -- --check

make lint 2>&1 | tee /tmp/lint-vsleep-initial-app-deps.out
Finished `dev` profile

make test 2>&1 | tee /tmp/test-vsleep-initial-app-deps.out
Summary [   0.006s] 1 test run: 1 passed, 0 skipped
```

Implementation validation passed on 2026-06-01:

```plaintext
make test 2>&1 | tee /tmp/test-vsleep-initial-app-impl.out
Summary [   0.180s] 25 tests run: 25 passed, 0 skipped

make lint 2>&1 | tee /tmp/lint-vsleep-initial-app-impl.out
Finished `dev` profile
```

Final validation passed on 2026-06-01:

```plaintext
make check-fmt 2>&1 | tee /tmp/check-fmt-vsleep-initial-app-final2.out
cargo fmt --all -- --check

make lint 2>&1 | tee /tmp/lint-vsleep-initial-app-final2.out
Finished `dev` profile

make test 2>&1 | tee /tmp/test-vsleep-initial-app-final2.out
Summary [   2.366s] 25 tests run: 25 passed, 0 skipped

make markdownlint 2>&1 | tee /tmp/markdownlint-vsleep-initial-app-final2.out
Summary: 0 error(s)

make nixie 2>&1 | tee /tmp/nixie-vsleep-initial-app-final2.out
All diagrams validated successfully
```
