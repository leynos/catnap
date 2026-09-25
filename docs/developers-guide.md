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

### Coverage publication

Coverage has two workflows, and the split is a contract (concordat's CV-005,
`main-owned-codescene-coverage`), not a convention.

- `ci.yml` measures lld-linked lcov coverage on every pull request with the
  shared `generate-coverage` action, `with-ratchet: 'true'` and
  `publish-artefact: 'false'`. A drop against the ratchet baseline fails the
  pull request. The lane holds no CodeScene credential, has no upload step, and
  never contacts CodeScene.
- `coverage-main.yml` runs on every push to `main` and on dispatch. It measures
  the same source with the same action, format, output path, and default
  baseline files. A push to `main` writes the ratchet baseline every pull
  request compares against; a dispatch reads it without advancing it. The lane
  then uploads the report to CodeScene in explicit upload mode. A check step
  reports whether the secret is set by evaluating
  `${{ secrets.CS_ACCESS_TOKEN != '' }}` into its output, and no step holds the
  token in its `env`, because the composite upload action would hand a step
  `env` to its nested `upload-artifact` and cache steps; the upload step passes
  the secret as its `access-token` input. The check runs earlier in the
  upload's own job, under no default shell, since a step's output is readable
  only there. The upload's `if:` is exactly
  `steps.codescene-token.outputs.available == 'true'` joined by `&&` to
  `github.ref == 'refs/heads/main'` (a dispatch can name any branch, and any
  further conjunct could only narrow, defeat, or invert the upload), and the
  workflow's concurrency group, exactly
  `${{ github.workflow }}-${{ github.ref }}` at every level, never cancels a
  run in progress and never overlaps two runs, so triggered runs (push and
  dispatch) upload in commit order and a burst of merges cannot abandon a
  baseline write. A manual re-run of an older `main` run is an operator action
  that republishes that commit's coverage and baseline until the next push
  supersedes it. The workflow answers exactly a push to `main` and
  `workflow_dispatch`, and the coverage selection both lanes run is pinned in
  the contract.

One known exception: a Dependabot pull request merged by the automerge workflow
with `GITHUB_TOKEN` fires no push event, so that merge is neither measured nor
uploaded until the next push to `main`; shared-actions #518 tracks the fix.
There is deliberately no `schedule` trigger to paper over it. Likewise, a
dispatch that replaces a pending push uploads the same or a newer commit, but
the ratchet baseline is saved only on a push, so it stays one commit behind
until the next push; shared-actions #518 covers that too.

The reasons are both quiet failures: a pull request from a fork cannot read the
secret, so an upload there is silently skipped, and CodeScene accepts an upload
only for a branch it analyses, which a pull request head is not.

`tests/coverage_workflows.rs` enforces the split over every workflow a pull
request can reach, following local reusable-workflow calls transitively, and
over every other workflow too: only the publisher may hold the token, name the
CodeScene host, run the CLI or the uploader, or touch the retired
`CODESCENE_CLI_SHA256` variable. It drives each rule against breaching fixtures
under `tests/coverage_workflows/`. The pull-request surface is seeded by every
event that runs a workflow for a pull request (`pull_request`,
`pull_request_target`, `merge_group`, the two review events, `issue_comment`,
`workflow_run`, and any push not limited to exactly `branches: [main]` or to
tags), and the push side is followed the same way: a workflow a push starts, or
one it calls, may run a ratcheted coverage step only behind
`if: github.event_name == 'pull_request'`, so the publisher stays the
baseline's only writer. When adding a workflow, keep CodeScene, `cs-coverage`,
and the token out of it unless it is the publisher; the contract names the
clause a change breaks.

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
export YAMLLINT_VERSION=1.38.0
uv tool install "yamllint==${YAMLLINT_VERSION}"
export PATH="$(uv tool dir --bin):${PATH}"
make lint
```

CI caches the uv cache, tool environment, and executable directory, then
installs `yamllint` with `uv tool`. It separately caches actionlint v1.7.12
and, on a cache miss, uses the upstream download script pinned to commit
`914e7df21a07ef503a81201c76d2b11c789d3fca`, verifying the release archive's
SHA-256 checksum
(`8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8`) before
use. The CI lint step passes the cached or downloaded actionlint executable via
an absolute `ACTIONLINT` path while invoking trusted `/usr/bin/make`, so
checkout contents cannot shadow `make`.

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

The runner depends on Monotony's `MonotonicClock` trait rather than calling
`std::time::Instant` directly. Production code combines `StdMonotonicClock`
with Catnap's `ThreadLogicalSleeper`. The clock owns only monotonic
observation; the sleeper owns logical-time scaling and blocking sleep.

`LogicalSleeper` is Catnap's narrowly scoped adapter seam. It may be
implemented only by sleep orchestration callers that must supply an alternative
blocking strategy; command parsing and unrelated application code use
`ThreadLogicalSleeper`. Runner tests pair Monotony's
`SharedManualMonotonicClock` with a local advancing sleeper, keeping time
observation and sleep progression deterministic without mocks.

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
  cover parsing, cadence selection, locale formatting, and manual-clock
  orchestration with an advancing sleeper.
- Property tests in `src/duration_tests.rs` and `src/clock.rs` use `proptest`
  to build compound operands and logical-time durations, checking parser
  rewrites, zero preservation, monotonicity, bounded truncation, and saturation.
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
`E0004`, which pins `#[non_exhaustive]` on `CliError`, `DurationParseError`, and
`ClockConfigError`. That contract is what keeps adding an error variant a
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

Run the spelling gate with:

```bash
make spelling
```

The gate enforces en-GB-oxendict spelling in tracked Markdown prose.
`make markdownlint` depends on it, and `make all` runs it with the repository's
other checks.

The tracked `typos.toml` is regenerated on every run from the live shared
dictionary and the repository-specific `typos.local.toml` overlay. Never edit
generated entries by hand. Add only narrow identifier, API, proper-name, or
immutable-fixture exceptions to the overlay; ordinary prose belongs in Oxford
spelling. Because the dictionary is live, `typos.toml` must never be drift
checked in continuous integration.

The shared `typos-config-builder` CLI refreshes the estate dictionary into the
untracked `.typos-oxendict-base.toml` cache only when the authoritative copy is
newer, records refresh metadata in `.typos-oxendict-base.json`, and reuses a
valid cache when the network is unavailable. The gate also enforces exact
phrase corrections, such as those Typos cannot match because it splits
hyphenated phrases into separate words.
