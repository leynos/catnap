#!/usr/bin/env bash
# Check that the development linker and pinned Rust toolchain are available.

set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/build-tools-common.sh
. "$script_dir/build-tools-common.sh"

check_mold() {
  local pinned=$1 installed resolved
  if ! is_linux; then
    note "mold is Linux-only; falling back to the default $(uname -s) linker"
    return 0
  fi
  if ! resolved=$(command -v mold 2>/dev/null); then
    note "mold not found on PATH (pinned $pinned)"
    note 'install it with: make install-build-tools'
    return 1
  fi
  if ! installed=$(installed_mold_version) || [ -z "$installed" ]; then
    note "mold at $resolved is on PATH but cannot report its version"
    note 'reinstall it with: make install-build-tools'
    return 1
  fi
  if [ "$installed" != "$pinned" ]; then
    note "mold $installed at $resolved does not match the pin $pinned"
    note 'run make install-build-tools to match'
    return 1
  fi
  note "mold $installed at $resolved"
}

check_clang() {
  local resolved version
  if ! is_linux; then
    note "clang is only required for the Linux build; skipping on $(uname -s)"
    return 0
  fi
  if ! resolved=$(command -v clang 2>/dev/null); then
    note 'clang driver not found on PATH; install clang using your OS package manager'
    note 'for example: dnf install clang (Fedora/Rocky) or apt install clang (Debian/Ubuntu)'
    note 'after installing clang, run: make check-build-tools'
    return 1
  fi
  if ! version=$(clang --version 2>/dev/null) || [ -z "$version" ]; then
    note "clang driver at $resolved cannot report its version"
    note 'install clang using your OS package manager, then run: make check-build-tools'
    return 1
  fi
  note "clang driver at $resolved"
}

check_lld() {
  local resolved version
  if ! is_linux; then
    note "lld is only required for Linux coverage; skipping on $(uname -s)"
    return 0
  fi
  if ! resolved=$(command -v ld.lld 2>/dev/null); then
    note 'lld linker not found on PATH; install it using your OS package manager'
    note 'for example: dnf install lld (Fedora/Rocky) or apt install lld (Debian/Ubuntu)'
    note 'after installing lld, run: make check-coverage-tools'
    return 1
  fi
  if ! version=$(ld.lld --version 2>/dev/null) || [ -z "$version" ]; then
    note "lld linker at $resolved cannot report its version"
    note 'install lld using your OS package manager, then run: make check-coverage-tools'
    return 1
  fi
  note "lld linker at $resolved: $version"
}

check_coverage_linkers() {
  local status=0
  check_clang || status=1
  check_lld || status=1
  [ "$status" -eq 0 ]
}

check_toolchain() {
  local toolchain=$1 mode=$2 installed_toolchains installed_components required_components
  local component package_name status=0
  local -a components
  if ! command -v rustup >/dev/null 2>&1; then
    note 'rustup not found on PATH; install it from https://rustup.rs'
    note 'after installing rustup, run: make install-build-tools'
    return 1
  fi
  if ! installed_toolchains=$(rustup toolchain list) ||
    ! printf '%s\n' "$installed_toolchains" | grep -Eq "^${toolchain}(-|[[:space:]]|$)"; then
    note "toolchain $toolchain is not installed"
    note 'install it with: make install-build-tools'
    return 1
  fi
  note "toolchain $toolchain available"

  if ! required_components=$(pinned_toolchain_components); then
    return 1
  fi
  if [ -z "$required_components" ]; then
    return 0
  fi
  if ! installed_components=$(rustup component list --toolchain "$toolchain" --installed); then
    note "could not list installed components for $toolchain"
    return 1
  fi
  IFS=',' read -r -a components <<< "$required_components"
  for component in "${components[@]}"; do
    if [ "$mode" = '--coverage-only' ] &&
      [ "$component" = 'rustc-codegen-cranelift-preview' ]; then
      continue
    fi
    case "$component" in
      *-preview) package_name=${component%-preview} ;;
      *) package_name=$component ;;
    esac
    if ! printf '%s\n' "$installed_components" | awk -v wanted="$package_name" \
      '$1 == wanted || index($1, wanted "-") == 1 { found=1 } END { exit !found }'; then
      note "component $component is not installed for $toolchain"
      note 'install it with: make install-build-tools'
      status=1
    fi
  done
  [ "$status" -eq 0 ]
}

main() {
  local status=0 mold_pin toolchain_pin mode=${1:-}
  case "$mode" in
    '' | --toolchain-only | --coverage-only) ;;
    *) fail "usage: $0 [--toolchain-only|--coverage-only]" ;;
  esac
  [ "$#" -le 1 ] || fail "usage: $0 [--toolchain-only|--coverage-only]"
  toolchain_pin=$(pinned_toolchain) || return 1
  case "$mode" in
    '')
      mold_pin=$(mold_version) || return 1
      check_mold "$mold_pin" || status=1
      check_clang || status=1
      ;;
    --coverage-only) check_coverage_linkers || status=1 ;;
    --toolchain-only) ;;
  esac
  check_toolchain "$toolchain_pin" "$mode" || status=1
  [ "$status" -eq 0 ] || note 'capability check failed; see the messages above'
  return "$status"
}

main "$@"
