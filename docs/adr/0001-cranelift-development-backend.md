# Architectural decision record (ADR) 001: Cranelift development backend

## Status

Accepted.

## Date

2026-10-01.

## Context and problem statement

The Rust build standard requires Cranelift as Catnap's development codegen
backend. Catnap's release workflow uses stable Cargo and rustc, and an earlier
attempt to select Cranelift with Cargo's `[profile.dev] codegen-backend` key
made stable Cargo reject `.cargo/config.toml` during the v0.1.0 release.

The unresolved question was whether a nightly rustc flag could select Cranelift
for development while the release jobs continued to use stable. A paired
experiment on commit `82221d0` tested a nightly development build and the
`aarch64-unknown-linux-gnu` Cross release route with an empty `RUSTFLAGS` and
the cross image's `cc` linker override. Both the baseline and candidate release
builds succeeded; the development candidate passed the Cranelift flag to rustc
and built Catnap.

## Decision Drivers

- Use Cranelift for ordinary development builds as required by the build
  standard.
- Preserve the stable release toolchain and platform linkers.
- Install and check the backend through the repository's pinned toolchain
  workflow.
- Keep coverage builds on LLVM so the coverage tools retain their expected
  code generation and linker behaviour.

## Requirements

### Functional requirements

- Development, test, lint, and typecheck builds select Cranelift.
- Coverage and release builds do not receive the development backend flag.
- Stable release builds continue to load the repository Cargo configuration.

### Technical requirements

- Pin the Cranelift preview component beside Catnap's nightly toolchain.
- Provide an installation route for CI's shared setup action, whose component
  list is fixed outside this repository.
- Make missing local components fail before a development build starts.
- Guard both the Cranelift development flags and the release override with
  mutation-tested contracts.

## Options considered

### Option A: Keep LLVM as the development default

This avoids preview-backend limitations and needs no additional component. It
does not meet the Rust build standard's Cranelift requirement.

### Option B: Select Cranelift with Cargo's profile configuration

This follows Cargo's profile-level backend interface, but stable Cargo rejects
that key while reading the release configuration. It repeats the failure that
broke the v0.1.0 release.

### Option C: Pass Cranelift as a nightly rustc flag

The nightly toolchain accepts `-Zcodegen-backend=cranelift` through Cargo's
`rustflags`. Assigning an empty `RUSTFLAGS` in release steps displaces those
flags before stable rustc sees them. The paired development and Cross builds
passed with this arrangement.

| Topic                          | Option A | Option B | Option C                      |
| ------------------------------ | -------- | -------- | ----------------------------- |
| Meets the development standard | No       | Yes      | Yes                           |
| Stable release compatibility   | Yes      | No       | Measured for one Cross target |
| Requires the preview component | No       | Yes      | Yes                           |

_Table 1: Cranelift selection options._

## Decision outcome / proposed direction

Use Option C. Add `-Zcodegen-backend=cranelift` to both Cargo development
`rustflags` sources and to the Makefile's standard flags. Pin
`rustc-codegen-cranelift-preview` in `rust-toolchain.toml`; install it in CI
before Rust development suites and verify it in the local build preflight.
Coverage continues to assign LLVM-specific flags. Stable release build steps
continue to assign empty `RUSTFLAGS`.

The profile-level Cargo key remains prohibited. The stable Cargo probe and
release workflow contract keep that distinction executable.

## Goals and non-goals

- Goals:
  - Select Cranelift by default for development and test builds.
  - Retain the stable release path and LLVM coverage path.
- Non-goals:
  - Change the release optimisation backend.
  - Claim a Catnap-specific compile-time improvement without a benchmark.
  - Prove every release-matrix target from the single Cross experiment.

## Migration plan

1. Pin and install the preview component through the existing toolchain
   installer and a CI-specific Make target.
2. Add the Cranelift flag to development configuration and target contracts.
3. Require stable release workflows to clear `RUSTFLAGS` and retain their
   target linker overrides.
4. Run the local unit, behavioural, lint, spelling, Markdown, and workflow
   gates on the integrated head.

## Known risks and limitations

- Cranelift is a preview backend with unsupported language features; the
  upstream
  [Cranelift documentation](https://github.com/rust-lang/rustc_codegen_cranelift#download-using-rustup)
  lists current platform support and limitations.
- The compatibility experiment measured one Linux cross target. The release
  workflow's dispatch path remains the proof for the complete target matrix.
- No benchmark measured Cranelift's compile-time effect on Catnap.

## Outstanding decisions

None. Revisit this decision if Catnap adopts a feature the backend cannot
compile or if the release toolchain changes.
