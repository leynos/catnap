# Developer Guide

This guide explains the contributor workflow for the `catnap` command.

## Local Workflow

Use `make all` as the public entrypoint for formatting, linting, and tests.
`make lint` runs rustdoc, Clippy, Whitaker, yamllint, and actionlint.
`make test` prefers `cargo nextest run` and falls back to `cargo test` when
cargo-nextest is not available. Because `cargo nextest run` does not execute
doctests, a nextest-backed `make test` run skips them; run `cargo test --doc`
separately as a required additional step when nextest is present.
`make coverage` uses `cargo llvm-cov` with `lld`.

### GitHub Actions workflow linting

`make lint` runs `yamllint .github/workflows` and `actionlint`, so every
workflow receives YAML style, syntax, and GitHub Actions semantic validation.
The `.yamllint.yml` policy accepts GitHub's unquoted `on` trigger key while
requiring `true` and `false` for boolean values.

Install `yamllint` with the version configured by `YAMLLINT_VERSION`, then
install `actionlint` using its
[upstream instructions](https://github.com/rhysd/actionlint/blob/main/README.md#installation).
Make both linters available on `PATH` before running the target:

```sh
uv tool install "yamllint==${YAMLLINT_VERSION}"
make lint
```

CI caches the uv cache, tool environment, and executable directory, then
installs `yamllint` with `uv tool`. It separately caches actionlint v1.7.12
and, on a cache miss, uses the upstream download script pinned to commit
`914e7df21a07ef503a81201c76d2b11c789d3fca`, verifying the release archive's
SHA-256 checksum
(`8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8`) before
use. The CI lint step prefixes the repository root to `PATH` so the cached or
downloaded actionlint executable is available to `make lint`.

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

Duration suffix metadata is owned by `UNITS` in `src/duration.rs`. All duration
parsing and compound-operand boundary detection must use this table rather than
maintaining separate suffix lists. Keep suffix composition inside the duration
module; callers supply complete operands and consume typed parse results.
`src/duration_number.rs` handles only the numeric part of an operand: it
receives a nanosecond multiplier from the duration module and never inspects
suffix spelling.

`DurationParseError` variants describe the domain fault alone. Advisory text
that names the command or its syntax — such as the compound-operand "did you
mean" line — belongs in `write_cli_error` in `src/lib.rs`, which reads the
structured `suggestion` field. Adding command wording to an error's `#[error]`
display string would leak the command-line layer into the domain.

End-to-end tests use the hidden `--logical-second-ms` argument to shorten one
logical second to a small real duration. This argument is private test support:
it is intentionally omitted from normal help output and must not be documented
as a user-facing option.

## Test Layout

The test suite covers the same behaviour from several angles:

- Unit tests in `src/duration_tests.rs`, `src/format.rs`, and `src/runner.rs`
  cover parsing, cadence selection, locale formatting, and mock-clock
  orchestration.
- Property tests in `src/duration_tests.rs` use `proptest` to build compound
  operands from generated components and check that the suggested rewrite
  splits back into those components and parses to the same duration.
- Behavioural tests in `tests/behaviour.rs` use `rstest-bdd` scenarios from
  `tests/features/sleep_cli.feature`.
- Snapshot tests in `tests/snapshots.rs` pin representative remaining-time
  output.
- End-to-end tests in `tests/e2e.rs` build and run the compiled binary with
  accelerated logical seconds.
- UI tests in `tests/ui/` compile against the public crate boundary, pin the
  user-facing `Display` output of public error types, and pin the compiler
  diagnostics that keep those error enums non-exhaustive.

### Public error UI tests

The `tests/ui.rs` harness uses `trybuild` in two complementary modes, each
covering what the other cannot.

Pass fixtures, `tests/ui/*_display.rs`, are compiled and executed as external
crates. Pass mode is required for message text: Rust evaluates `Display`
implementations at runtime, so a compile-fail fixture can snapshot compiler
diagnostics but never observes an error value's formatted output.

Compile-fail fixtures, `tests/ui/*_non_exhaustive.rs`, match every public
variant of an error enum without a wildcard arm. Each is expected to fail with
`E0004`, which pins `#[non_exhaustive]` on `CliError`, `DurationParseError`,
and `ClockConfigError`. That contract is what keeps adding an error variant a
non-breaking change for downstream crates.

Run the focused harness with:

```sh
cargo test --test ui
```

`make test` also discovers the harness and is the required pre-commit and CI
entrypoint.

#### Updating display fixtures

Treat each expected string literal in a display fixture as a UI snapshot. When
adding a public error type or variant, add an assertion with representative
field values to the corresponding fixture, or add a new `*_display.rs` file. If
an intentional wording change alters a message, update the expected literal in
the same commit and review the string diff deliberately. Display fixtures have
no adjacent `.stderr` file, so `TRYBUILD=overwrite` does not maintain them.

#### Updating compile-fail snapshots

Each `*_non_exhaustive.rs` fixture has an adjacent `.stderr` file holding the
expected diagnostic. Add every new variant to the fixture's `match`, then
regenerate the snapshot with:

```sh
TRYBUILD=overwrite cargo test --test ui
```

Review the regenerated diagnostic before committing. Because the snapshots
capture compiler output, they are tied to the toolchain pinned in
`rust-toolchain.toml`; a toolchain bump that rewords `E0004` requires the same
regeneration step. A fixture that starts *passing* means the enum has lost
`#[non_exhaustive]`, which is a breaking change rather than a snapshot to
refresh.

## Spelling gate

Run `make spelling` to enforce en-GB-oxendict spelling in tracked Markdown
prose. The target checks `typos.toml` for drift, runs the consumer phrase
scanner, then runs the pinned `typos` release over tracked Markdown files.
`make markdownlint` depends on this gate, and `make all` runs it with the
repository's other checks.

The generated configuration combines the shared estate dictionary with the
repository-specific `typos.local.toml` overlay. Do not edit `typos.toml` by
hand. Add only narrow identifier, API, proper-name, or immutable-fixture
exceptions to the local overlay; ordinary prose belongs in Oxford spelling.

The configuration builder is pinned to commit
`d6da92f02240a79a945c835f69bdd08a888da1d0`. Regenerate the configuration with:

```sh
TYPOS_CONFIG_BUILDER_COMMIT=d6da92f02240a79a945c835f69bdd08a888da1d0
uvx --python 3.14 \
  --from "git+https://github.com/leynos/typos-config-builder.git@${TYPOS_CONFIG_BUILDER_COMMIT}" \
  typos-config-builder
```

Use the same command with `--check` in quality gates to detect drift without
rewriting `typos.toml`. The builder refreshes the shared dictionary into the
untracked `.typos-oxendict-base.toml` cache only when the authority is newer,
records refresh metadata in `.typos-oxendict-base.json`, and reuses a valid
local cache when the authority is unavailable.

Typos splits hyphenated phrases into separate words. The consumer-owned
`scripts/typos_rollout_check.py` therefore reads phrase corrections from the
shared cache and local overlay, while taking ignore patterns and file
exclusions from generated `typos.toml`. It reports prohibited phrases without
duplicating the builder's validation, cache, merge or rendering behaviour.
