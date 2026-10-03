# User Guide

This guide explains how to use `catnap`, a GNU-like sleep command that reports
remaining time while it waits.

## Command Syntax

Run `catnap` with one or more duration operands:

```sh
catnap NUMBER[SUFFIX]...
```

Each operand is a non-negative decimal number with an optional suffix:

- `s` for seconds, which is also the default when no suffix is supplied.
- `m` for minutes.
- `h` for hours.
- `d` for days.

Multiple operands are summed, matching GNU `sleep` style:

```sh
catnap 1m 5s
```

Durations must remain separate operands: write `catnap 5h 20m`, not
`catnap 5h20m`. When otherwise valid operands are accidentally concatenated,
the diagnostic suggests the whitespace-separated form.

The command accepts `--help` and `--version`. Invalid operands, missing
operands, unsupported suffixes, and unknown options are reported to standard
error with a non-zero exit status.

## Progress Output

`catnap` uses a monotonic stopwatch, so changes to the system wall clock do not
alter the requested wait. Progress is written to standard error; standard
output stays empty.

The progress interval depends on the full requested duration:

- Durations greater than one minute report every thirty seconds.
- Durations of one minute or less report every five seconds.
- Durations of twenty seconds or less report every second.

Remaining time is formatted for the current environment locale where a
translation is available, with English used as the fallback locale.

## Development Tooling

The project uses Rust 2024, a pinned nightly toolchain, strict lint settings,
and documented source code. Development, test, lint, and typecheck builds use
the Cranelift backend. Release builds use stable Rust and clear the
nightly-only development flags before compiling. On Linux targets,
`.cargo/config.toml` configures clang to link with `mold` so local debug builds
link quickly. Coverage generation stays on LLVM and uses `lld` because LLVM
coverage tools expect LLVM-compatible linker behaviour. The
[build orchestration migration guide](migrations/v0-1-0.md) explains the
available Netsuke commands, their former Make equivalents, and local setup.

## Netsukefile actions

The repository's `Netsukefile` exposes these public actions. Use
`netsuke help targets` to list all actions and their descriptions. Commonly
used actions include:

- `netsuke` runs the default `all` action for formatting checks, linting, tests,
  spelling, and workflow-contract checks.
- `netsuke build check-fmt` verifies Rust and Markdown formatting.
- `netsuke build lint` runs rustdoc, Clippy, Whitaker, yamllint, and actionlint
  with warnings denied where supported.
- `netsuke build github-actions-lint` validates GitHub Actions workflows.
- `netsuke build test` runs `cargo nextest run` when cargo-nextest is installed
  and then runs workspace doctests. When cargo-nextest is unavailable, it falls
  back to `cargo test`, which includes its normal doctest run.
- `netsuke build test-ui` runs the focused Rust UI tests.
- `netsuke build test-workflow-contracts` checks the repository's workflow
  contracts, including the shared coverage policy.
- `netsuke build typecheck` checks all targets and features without producing
  binaries.
- `netsuke build install-build-tools` installs the pinned nightly toolchain and
  local linker; `check-build-tools`, `check-rust-toolchain`, and
  `check-coverage-tools` verify the corresponding prerequisites.
- `netsuke build build` builds the debug target.
- `netsuke build release` builds the release target.
- `netsuke build coverage` writes `lcov.info` using `cargo llvm-cov` and `lld`.
- `netsuke build markdownlint` checks Markdown files.
- `netsuke build nixie` validates Mermaid diagrams.

Install `clang`, `lld`, `mold`, Ninja, and the `netsuke-build` crate before
running the full generated workflow locally on Linux. The developer's guide
documents the current Cargo installation command.
