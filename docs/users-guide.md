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
coverage tools expect LLVM-compatible linker behaviour.

## Makefile Targets

The generated `Makefile` exposes these public targets:

- `make all` runs formatting checks, linting, and tests.
- `make check-fmt` verifies Rust formatting.
- `make lint` runs rustdoc, Clippy, and Whitaker with warnings denied.
- `make test` runs `cargo nextest run` when cargo-nextest is installed and
  follows it with workspace doctests. When cargo-nextest is unavailable, it
  falls back to `cargo test`, which runs its normal doctest suite.
- `make install-build-tools` installs the pinned nightly with its requested
  components and the checksum-verified `mold` release on Linux.
- `make check-build-tools` verifies the pinned nightly and its components,
  `clang`, and `mold` before development, test, lint, and typecheck builds.
- `make check-rust-toolchain` verifies the pinned channel and components before
  formatting commands.
- `make check-coverage-tools` verifies the pinned toolchain, `clang`, and `lld`
  before coverage generation.
- `make build` builds the debug target.
- `make release` builds the release target.
- `make coverage` writes `lcov.info` using `cargo llvm-cov` and `lld`.
- `make markdownlint` checks Markdown files.
- `make nixie` validates Mermaid diagrams.

On Linux, install `clang` and `lld` with the operating system's package
manager, then run `make install-build-tools` to install the pinned nightly and
`mold`.
