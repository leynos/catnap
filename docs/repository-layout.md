# Repository layout

This document describes the generated vsleep repository layout. It is the
canonical reference for where source code, tests, configuration, automation,
and long-lived documentation belong.

## Top-level tree

The tree below shows the repository structure. It is intentionally compact and
omits build output such as `target/`.

```plaintext
.
├── .cargo/
│   └── config.toml
├── .github/
│   ├── dependabot.yml
│   └── workflows/
│       ├── ci.yml

│       └── release.yml

├── docs/
│   ├── contents.md
│   ├── developers-guide.md
│   ├── repository-layout.md
│   ├── users-guide.md
│   └── ...
├── src/
│   ├── cli.rs
│   ├── clock.rs
│   ├── duration.rs
│   ├── format.rs
│   ├── lib.rs
│   ├── main.rs
│   └── runner.rs
├── tests/
│   ├── behaviour.rs
│   ├── e2e.rs
│   ├── features/
│   │   └── sleep_cli.feature
│   └── snapshots.rs
├── AGENTS.md
├── Cargo.toml
├── LICENSE
├── Makefile
├── README.md
├── clippy.toml
├── codecov.yml
└── rust-toolchain.toml
```

## Path responsibilities

- `.cargo/config.toml`: Configures Cargo defaults for local development,
  including Linux linker and code-generation settings.
- `.github/dependabot.yml`: Configures automated dependency update checks.
- `.github/workflows/ci.yml`: Runs the generated project's continuous
  integration checks.

- `.github/workflows/release.yml`: Builds and publishes binary release
  artefacts for the application flavour.

- `docs/`: Holds long-lived reference documentation, guides, style rules, and
  design material.
- `docs/contents.md`: Indexes the documentation set and should be updated when
  documentation files are added, renamed, or removed.
- `docs/users-guide.md`: Explains how to use the generated project and its
  public build and test commands.
- `docs/developers-guide.md`: Explains the contributor workflow and local
  tooling used to work on the generated project.
- `docs/repository-layout.md`: Documents the repository tree and path
  responsibilities.

- `src/lib.rs`: Exposes the library boundary used by the binary, unit tests,
  behavioural tests, and documentation examples.
- `src/main.rs`: Contains the application entrypoint and top-level executable
  wiring.
- `src/cli.rs`: Parses GNU-like sleep operands, public options, and the hidden
  e2e-only logical-second argument.
- `src/clock.rs`: Defines the injectable monotonic clock trait and the real
  `Instant`-backed implementation.
- `src/duration.rs`: Parses duration operands and selects progress-reporting
  intervals.
- `src/format.rs`: Formats remaining-time progress lines for the selected
  locale.
- `src/runner.rs`: Coordinates monotonic sleeping and progress output.

- `tests/`: Holds integration and behavioural tests that exercise public
  behaviour.
- `tests/behaviour.rs`: Binds `rstest-bdd` scenarios to GNU-like operand
  behaviour.
- `tests/e2e.rs`: Builds and runs the compiled binary through accelerated
  logical seconds.
- `tests/features/sleep_cli.feature`: Describes behaviour-driven parsing
  scenarios.
- `tests/snapshots.rs`: Pins representative locale-aware remaining-time
  output.
- `AGENTS.md`: Provides repository-specific working instructions for agents and
  contributors.
- `Cargo.toml`: Defines package metadata, dependencies, lint policy, and Cargo
  configuration.
- `LICENSE`: Records the project licence text.
- `Makefile`: Provides the public build, lint, test, coverage, and
  documentation validation commands.
- `README.md`: Introduces the project and gives the shortest useful
  getting-started path.
- `clippy.toml`: Configures Clippy lint behaviour that is not expressed
  directly in `Cargo.toml`.
- `codecov.yml`: Configures coverage reporting behaviour.
- `rust-toolchain.toml`: Pins the Rust toolchain channel and required
  components.

## Ownership boundaries

- Keep source code under `src/`. Keep `src/main.rs` thin and put reusable
  command logic behind the library boundary in `src/lib.rs` and sibling modules.
- Keep black-box integration tests and externally observable workflow tests
  under `tests/`.
- Keep reusable documentation under `docs/`. Update `docs/contents.md` whenever
  a documentation file is added, renamed, or removed.
- Keep build and validation entrypoints in `Makefile`; prefer adding or
  extending a Make target over documenting an ad hoc command.
- Keep continuous integration workflow changes under `.github/workflows/` and
  dependency-update policy under `.github/dependabot.yml`.
- Do not commit generated build output such as `target/`, coverage artefacts,
  or local editor state.

## Updating this document

Update this document when the repository gains a new top-level directory, a new
long-lived documentation category, a new workflow file, or a changed ownership
boundary that would otherwise make the tree misleading.
