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

`make test-workflow-contracts` enforces the split by running
`cv005-contracts check`, the shared contract library in `leynos/shared-actions`
(`packages/cv005-contracts`), from a full commit named by `CV005_CONTRACTS_REF`
in the Makefile; CI runs it in a "Check the CV-005 contracts" step. A fix to
the rules is therefore a pin bump. The repository's parameters are in
`.github/cv005.toml`: its `repository` name and the `[selection]` the baseline
measures, which the publisher's generator must carry and every pull-request
lane must match. The library's own suite proves each rule refuses the shape it
exists to refuse, so this repository keeps no copy of the readers or the
refusal cases.

The rule covers every workflow a pull request can reach, following local
reusable-workflow calls transitively, and every other workflow too: only the
publisher may hold the token, reach a secret by a computed name, name the
CodeScene host, run the CLI or the uploader, or touch the retired
`CODESCENE_CLI_SHA256` variable. Workflows are read strictly: a duplicate key,
or a workflow declaring both `on` and `true`, is refused rather than silently
resolved, and a reading failure exits 2 rather than passing. The pull-request
surface is seeded by every event that runs a workflow for a pull request, and a
workflow a push starts, or one it calls, may run a ratcheted coverage step only
behind `if: github.event_name == 'pull_request'`, so the publisher stays the
baseline's only writer. When adding a workflow, keep CodeScene, `cs-coverage`,
and the token out of it unless it is the publisher; the library names the
clause a change breaks.

## Tooling

On Linux targets, `.cargo/config.toml` configures clang to link with `mold` so
debug builds link quickly. Coverage generation uses `lld` because LLVM coverage
tooling expects LLVM-compatible linker behaviour.

Install `clang`, `lld`, and `mold` before running the full generated workflow
locally on Linux.

### The build standard

Development, test, lint, and typecheck builds use the parallel `rustc` frontend
(`-Zthreads=8`) and, on Linux, the `mold` linker (`-Clink-arg=-fuse-ld=mold`).
These are defaults in `.cargo/config.toml`, which Cargo discovers on its own,
so a bare `cargo build` gets them. `mold` ships for Linux only, so the linker
flag lives in a Linux-only table and macOS and Windows keep their platform
linker. Cargo selects one `rustflags` source rather than merging them, so every
source repeats the same flags apart from the linker.

An assigned `RUSTFLAGS` replaces the configuration's flags, so the Makefile
recipes that set it compose the standard's flags onto any inherited value (CI's
`setup-rust` exports one). Two builds are deliberately excluded: coverage
assigns `RUSTFLAGS` without the fast flags, because a measurement should not
depend on them, and the release recipe and workflow keep the platform linker,
because they assign `RUSTFLAGS` (even an empty value displaces the
configuration). Cargo has no per-profile `rustflags`, so a direct
`cargo build --release` takes the configuration's flags unless `RUSTFLAGS` is
assigned too.

On Linux, install `mold` before building: the configuration names it, so a
build without it fails at link time. CI installs it through `setup-rust`'s
`install-mold` input. `tests/build_standard_contract.rs` holds the standard. It
reads the configuration sources, the commands `make -n` prints for each
development target on a Linux host and a macOS host (each keeping the caller's
own `RUSTFLAGS`) and for each coverage and release target on a Linux host, and
the `setup-rust` steps of the CI workflows (each must pass `install-mold`), so
a flag lost through a recipe or workflow edit fails there.

### Cold-cache allowance for the trybuild tests

`.config/nextest.toml` keeps the 180 s per-test allowance that the coverage
action writes when a repository has no file of its own, and gives the `ui`
tests three times that. They run a nested Cargo that compiles the whole
dependency graph, and a pull request has no warm sccache directory until the
default branch has written one (`setup-rust` owns that directory and only a
push to the default branch writes it). Before that cache exists, the nested
build alone can exceed 180 s on a GitHub-hosted runner.

### Cranelift exception

Cranelift is not the development-profile codegen backend. The repository pins
`nightly-2026-05-28`, but the release workflow builds with
`cross +stable build --release`, which reads `.cargo/config.toml` on a stable
toolchain. Stable Cargo refuses a `[profile.dev] codegen-backend` key ("config
profile `dev` is not valid") and stops, so selecting the backend there breaks
every release build; it broke the v0.1.0 release (recorded 2026-09-29).
`tests/build_backend_contract.rs` fails if a `codegen-backend` key returns to
the configuration while the release still builds on `+stable`.
`tests/stable_cargo_config.rs` asks stable Cargo itself, through
`rustup run stable cargo build --release --bin no-such-bin`: stable Cargo
resolves every configured profile before it looks up the target, so a refused
configuration and an accepted one differ in the message, and nothing compiles.
The probe must run on stable, because a nightly Cargo accepts a backend that
stable refuses. The test therefore needs the stable toolchain installed
(`rustup toolchain install stable --profile minimal`); CI installs it before
the tests run. Revisit if the release moves to the pinned nightly.

### Release builds

`release.yml` runs on a `v*.*.*` tag push and on `workflow_dispatch`, and
builds six targets in one matrix, each leg with a `builder`.

- **Native macOS.** `x86_64-apple-darwin` builds on `macos-15-intel` and
  `aarch64-apple-darwin` on `macos-latest`, with
  `cargo +stable build --release --target <target>`. `cross` has no Docker
  image for Apple targets; on a Linux runner it falls back to host cargo, which
  lacks the target and stops with E0463. That, and not only the Cranelift key
  above, is why the v0.1.0 macOS legs failed.
- **Cross for the rest.** The Linux (`x86_64`, `aarch64`), Windows GNU and
  FreeBSD legs run `cross +stable build --release --target <target>` on
  `ubuntu-latest`.
- **Linker for the x86_64 Linux leg.** `.cargo/config.toml` names `clang` as
  that triple's linker for the development build (with mold). The `cross` image
  has gcc and no clang, so the cross step sets
  `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc`. An environment value beats
  the configuration file and `cross` forwards `CARGO_*` variables into its
  container. The development configuration is untouched.
- **No cancellation.** `fail-fast` is off, so one failing leg cannot hide
  whether the others build.
- **Dry run.** A `workflow_dispatch` builds every leg and uploads the
  artefacts, then stops: the `release` job runs only for a tag push, or for a
  dispatch on a tag ref that sets `dry-run` to `false`. A branch dispatch
  therefore never publishes. Run the dispatch on a branch before tagging; it is
  the proof that every leg builds.

`tests/release_workflow.rs` holds these clauses: each is proved by a mutation
of a copy of the real workflow that the test must refuse.

### Compiler cache (sccache)

The shared `setup-rust` action gives sccache a local-disk directory under
`runner.temp` on a GitHub-hosted runner. The directory is restored with
`actions/cache` on every event and saved only on a push to `main`, so a pull
request reads the cache and never writes one.

- **One shared lane.** `ci.yml`'s `build-test` and `coverage-main.yml`'s
  `coverage-upload` both set `sccache-cache-discriminator: coverage`. The
  action's default discriminator is the job ID, which would give the two jobs
  different lanes and leave the pull-request lane without a writer. Only
  `coverage-upload` runs on a push to `main`, so it is the writer and
  `build-test` is the reader. Keep the two values equal.
- **What it warms.** The lane holds the artefacts of the coverage build, so the
  coverage step of `build-test` restores from it. The lint step compiles a
  different graph and gains almost nothing from it; measured on frankie, lint
  hit 2.5 % and the coverage step hit 100 %.
- **`expect-cache: any`.** A GitHub-hosted job accepts whichever cache backend
  the runner offers, so the input is set explicitly.
- **`release.yml` disables it.** A release build never saves the cache, and
  the `cross` legs build inside a container that receives neither
  `RUSTC_WRAPPER` nor `SCCACHE_PATH`, so sccache is switched off there with
  `use-sccache: 'false'`.

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

`TYPOS_CONFIG_BUILDER_VERSION` in the `Makefile` pins the
`typos-config-builder` release the gate runs (currently `v0.1.3`). Raise it
together with the regenerated `typos.toml`, never on its own.

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
