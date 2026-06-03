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
and documented source code. Development builds use Cranelift for debug code
generation. On Linux targets, `.cargo/config.toml` configures clang to link with
 `mold` so local debug builds link quickly. Coverage generation uses `lld`
instead because LLVM coverage tools expect LLVM-compatible linker behaviour.

## Makefile Targets

The generated `Makefile` exposes these public targets:

- `make all` runs formatting checks, linting, and tests.
- `make check-fmt` verifies Rust formatting.
- `make lint` runs rustdoc, Clippy, and Whitaker with warnings denied.
- `make test` runs `cargo nextest run` when cargo-nextest is installed and
  falls back to `cargo test` otherwise. `cargo nextest run` does not execute
  doctests, so they run only through the `cargo test` fallback; run
  `cargo test --doc` separately to exercise doctests when nextest is present.
- `make build` builds the debug target.
- `make release` builds the release target.
- `make coverage` writes `lcov.info` using `cargo llvm-cov` and `lld`.
- `make markdownlint` checks Markdown files.
- `make nixie` validates Mermaid diagrams.

Install `clang`, `lld`, and `mold` before running the full generated workflow
locally on Linux.
