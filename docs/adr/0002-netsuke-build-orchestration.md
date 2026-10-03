# Architectural decision record (ADR) 002: Use Netsuke for build orchestration

- Status: accepted
- Date: 2026-10-01

## Context

The repository used a Makefile to expose build, formatting, lint, test,
coverage, and documentation-validation targets. That workflow was also
duplicated in CI commands and contributor guidance. The project needed a
single, reviewable definition of those actions and their dependencies while
preserving the existing quality gates and coverage contract.

## Decision

In the context of consolidating local and CI build orchestration, facing
duplicated action definitions across the Makefile, CI workflows, and
contributor guidance, the project selected Netsuke v0.1.0-beta4 as the sole
workflow manifest, and against retaining Make or keeping a Make compatibility
shim, to achieve one dependency-aware definition with described actions and a
sequential default workflow, accepting the requirement to install Ninja and a
pinned nightly and the loss of direct compatibility with former Make
invocations.

The repository pins `netsuke-build` v0.1.0-beta4. Beta4 provides the manifest
capabilities used here: command lists, serial dependency execution, and
`netsuke help targets` output based on action descriptions. The default `all`
action declares `check-fmt`, `lint`, `test`, and `spelling` as serial
dependencies, followed by `test-workflow-contracts`. No no-op command is
required for this dependency-only action.

The Makefile is removed without a compatibility shim. The
[migration guide](migrations/v0-1-0.md) maps every former named Make target to
its Netsuke command.

## Alternatives considered

- Retain Make and keep CI and documentation synchronized with it. This would
  preserve the existing interface, but leave orchestration spread across Make,
  CI workflow steps, and contributor instructions.
- Replace Make with Netsuke while retaining a Make shim. This would ease the
  transition for old invocations but preserve a second entrypoint and require
  maintaining compatibility behaviour.
- Adopt Netsuke without a shim. This keeps the manifest as the sole workflow
  definition and makes the command transition explicit; this is the selected
  option.

## Consequences

- The Netsukefile is the source of truth for public build and validation
  actions, and descriptions power `netsuke help targets`.
- Existing Make actions remain available through documented Netsuke command
  names, with the sequential default workflow declared directly through
  dependencies.
- Rust formatting, rustdoc, Clippy, Whitaker, tests, type checking, spelling,
  Markdown linting, Mermaid validation, workflow linting, and lcov coverage
  remain available through Netsuke actions. The pull-request coverage ratchet
  and main-branch coverage publication/baseline workflow remain separate
  contracts.
- The `test` action prefers nextest where installed and then runs workspace
  doctests with `cargo test --workspace --doc --all-features`. When nextest is
  unavailable, `cargo test --all-targets --all-features` includes its normal
  doctest run.
- Local users and CI need the pinned nightly toolchain and Ninja. CI installs
  `netsuke-build` v0.1.0-beta4 and caches the executable using the version,
  runner platform, architecture, and installation toolchain.
- The Netsuke installation must continue to use a compatible pinned nightly
  until the crate no longer requires it. Workflow changes must preserve the
  existing coverage ratchet and publication boundaries.
