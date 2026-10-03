#!/usr/bin/env bash
# Install the pinned linker and Rust toolchain used by the development build.

set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/build-tools-common.sh
. "$script_dir/build-tools-common.sh"

MOLD_RELEASE_BASE_URL=${MOLD_RELEASE_BASE_URL:-https://github.com/rui314/mold/releases/download}
CURL_CONNECT_TIMEOUT=${CURL_CONNECT_TIMEOUT:-15}
CURL_MIN_BYTES_PER_SECOND=${CURL_MIN_BYTES_PER_SECOND:-1024}
CURL_STALL_SECONDS=${CURL_STALL_SECONDS:-60}
CRANELIFT_COMPONENT=rustc-codegen-cranelift-preview

BUILD_TOOLS_WORKDIR=

remove_workdir() {
  [ -n "$BUILD_TOOLS_WORKDIR" ] || return 0
  rm -rf -- "$BUILD_TOOLS_WORKDIR"
  BUILD_TOOLS_WORKDIR=
}

trap remove_workdir EXIT

# Require exactly one checksum row for the selected architecture before checking
# the downloaded archive; duplicate rows make the pin ambiguous.
verify_mold_archive() {
  local archive=$1 name=$2 expected recorded
  expected=$(awk -v name="$name" '$2 == name { print $1 }' "$MOLD_SHA256SUMS_FILE")
  [ -n "$expected" ] || fail "no checksum recorded for $name in $MOLD_SHA256SUMS_FILE"
  recorded=$(printf '%s\n' "$expected" | grep -c .)
  [ "$recorded" -eq 1 ] ||
    fail "$recorded checksums recorded for $name in $MOLD_SHA256SUMS_FILE; refusing to guess"
  printf '%s  %s\n' "$expected" "$archive" | sha256sum --check --status ||
    fail "checksum mismatch for $name; refusing to install"
  note "verified $name against $MOLD_SHA256SUMS_FILE"
}

install_mold() {
  local version=$1 arch name url workdir
  if ! is_linux; then
    note "mold is Linux-only; skipping on $(uname -s), the platform linker is used instead"
    return 0
  fi
  arch=$(mold_arch)
  name="mold-$version-$arch-linux.tar.gz"
  url="$MOLD_RELEASE_BASE_URL/v$version/$name"

  BUILD_TOOLS_WORKDIR=$(mktemp -d)
  workdir=$BUILD_TOOLS_WORKDIR

  note "downloading $url"
  curl --fail --silent --show-error --location \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --speed-limit "$CURL_MIN_BYTES_PER_SECOND" --speed-time "$CURL_STALL_SECONDS" \
    --output "$workdir/$name" "$url" ||
    fail "failed to download $name; retry with: netsuke build install-build-tools"
  verify_mold_archive "$workdir/$name" "$name"

  # The verified release archive has a versioned top-level directory; stripping
  # it places its bin, lib, and libexec directories under the chosen prefix.
  mkdir -p "$BUILD_TOOLS_PREFIX"
  tar --extract --gzip --strip-components=1 --directory "$BUILD_TOOLS_PREFIX" --file "$workdir/$name" ||
    fail "failed to unpack $name into $BUILD_TOOLS_PREFIX"
  note "installed mold $version into $BUILD_TOOLS_PREFIX"
  note "put $BUILD_TOOLS_PREFIX/bin first on PATH when not using Netsuke"
}

install_toolchain() {
  local toolchain=$1 components
  local -a arguments
  if ! command -v rustup >/dev/null 2>&1; then
    fail 'rustup not found on PATH; install it from https://rustup.rs'
  fi
  components=$(pinned_toolchain_components) || return 1
  arguments=(toolchain install "$toolchain" --profile minimal)
  if [ -n "$components" ]; then
    arguments+=(--component "$components")
  fi
  note "installing toolchain $toolchain"
  rustup "${arguments[@]}" ||
    fail "failed to install toolchain $toolchain; retry with: netsuke build install-build-tools"
}

install_cranelift_component() {
  local toolchain
  if ! command -v rustup >/dev/null 2>&1; then
    fail 'rustup not found on PATH; install it from https://rustup.rs'
  fi
  toolchain=$(pinned_toolchain) || return 1
  note "installing $CRANELIFT_COMPONENT for $toolchain"
  rustup component add "$CRANELIFT_COMPONENT" --toolchain "$toolchain" ||
    fail "failed to install $CRANELIFT_COMPONENT for $toolchain; retry with: netsuke build install-cranelift"
}

main() {
  local mode=${1:-} mold_pin toolchain_pin
  case "$mode" in
    --cranelift-only)
      [ "$#" -eq 1 ] || fail "usage: $0 [--cranelift-only]"
      install_cranelift_component
      return
      ;;
    '') [ "$#" -eq 0 ] || fail "usage: $0 [--cranelift-only]" ;;
    *) fail "usage: $0 [--cranelift-only]" ;;
  esac
  mold_pin=$(mold_version) || return 1
  toolchain_pin=$(pinned_toolchain) || return 1
  install_mold "$mold_pin"
  install_toolchain "$toolchain_pin"
  note 'ready; verify with: netsuke build check-build-tools'
}

main "$@"
