# Developer Guide

This guide explains the contributor workflow for the `catnap` command.

## Local Workflow

Use `make all` as the public entrypoint for formatting, linting, and tests.
`make lint` runs rustdoc, Clippy, and Whitaker. `make test` prefers
`cargo nextest run` and falls back to `cargo test` when cargo-nextest is not
available. Because `cargo nextest run` does not execute doctests, a
nextest-backed `make test` run skips them; run `cargo test --doc` separately as
a required additional step when nextest is present. `make coverage` uses
`cargo llvm-cov` with `lld`.

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
