# Changelog

All notable changes to `catnap` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/).

## [0.1.1] - Unreleased

### Fixed

- Release binaries are built again. The v0.1.0 tag published to crates.io but
  its GitHub release carried no binaries: a `codegen-backend` key in
  `.cargo/config.toml` made stable Cargo refuse the configuration, and, once
  that was removed, the two macOS legs still could not build because `cross`
  has no Docker image for Apple targets. The macOS legs now build natively on
  macOS runners.

- The x86_64 Linux release leg no longer depends on `clang`, which the `cross`
  image lacks: the release build links with `cc`.

### Changed

- The release workflow builds every leg without cancelling the others, and a
  manual dispatch builds all six legs as a dry run that creates no release.

## [0.1.0]

- Initial release.
