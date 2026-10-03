#!/usr/bin/env bash
# Shared helpers for the build tools used by the repository's standard.
#
# Sourced by the installer and checker so both read the same pins and report
# failures consistently.

set -euo pipefail

BUILD_TOOLS_HELPER_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
BUILD_TOOLS_REPO_ROOT=$(cd -- "$BUILD_TOOLS_HELPER_DIR/.." && pwd)

MOLD_VERSION_FILE="${MOLD_VERSION_FILE:-$BUILD_TOOLS_REPO_ROOT/tools/mold/VERSION}"
MOLD_SHA256SUMS_FILE="${MOLD_SHA256SUMS_FILE:-$BUILD_TOOLS_REPO_ROOT/tools/mold/SHA256SUMS}"
RUST_TOOLCHAIN_FILE="${RUST_TOOLCHAIN_FILE:-$BUILD_TOOLS_REPO_ROOT/rust-toolchain.toml}"
BUILD_TOOLS_PREFIX="${BUILD_TOOLS_PREFIX:-$HOME/.local}"

note() { printf 'build-tools: %s\n' "$*" >&2; }

fail() {
  printf 'build-tools: %s\n' "$*" >&2
  exit 1
}

# Read a one-line pin without silently concatenating malformed values.
read_pin() {
  local file=$1 value lines
  [ -f "$file" ] || fail "missing version pin: $file"
  lines=$(grep -c '' <"$file")
  [ "$lines" -le 1 ] || fail "expected one line in version pin: $file, found $lines"
  value=$(cat -- "$file")
  value=${value#"${value%%[![:space:]]*}"}
  value=${value%"${value##*[![:space:]]}"}
  [ -n "$value" ] || fail "empty version pin: $file"
  case $value in
    *[[:space:]]*) fail "version pin contains whitespace: $file" ;;
  esac
  printf '%s' "$value"
}

mold_version() { read_pin "$MOLD_VERSION_FILE"; }

pinned_toolchain() {
  local file=$RUST_TOOLCHAIN_FILE value
  [ -f "$file" ] || fail "missing version pin: $file"
  value=$(awk -F'"' '/^[[:space:]]*channel[[:space:]]*=/ { print $2; exit }' "$file")
  [ -n "$value" ] || fail "no channel found in: $file"
  printf '%s' "$value"
}

pinned_toolchain_components() {
  local file=$RUST_TOOLCHAIN_FILE
  [ -f "$file" ] || fail "missing version pin: $file"
  awk '
    /^\[toolchain\][[:space:]]*$/ { in_toolchain=1; next }
    /^\[/ { in_toolchain=0 }
    in_toolchain && /^[[:space:]]*components[[:space:]]*=/ { in_components=1 }
    in_components {
      line=$0
      sub(/#.*/, "", line)
      while (match(line, /"[^"]+"/)) {
        component=substr(line, RSTART + 1, RLENGTH - 2)
        if (component ~ /[[:space:],]/) exit 2
        printf "%s%s", separator, component
        separator=","
        line=substr(line, RSTART + RLENGTH)
      }
      if (line ~ /\]/) { found=1; exit }
    }
    END { if (!found) exit 1 }
  ' "$file" || fail "could not read components from: $file"
}

is_linux() { [ "$(uname -s)" = 'Linux' ]; }

mold_arch() {
  local machine
  machine=$(uname -m)
  case "$machine" in
    x86_64 | amd64) printf 'x86_64' ;;
    aarch64 | arm64) printf 'aarch64' ;;
    *) fail "unsupported architecture for the pinned mold release: $machine" ;;
  esac
}

installed_mold_version() {
  local output
  output=$(mold --version 2>/dev/null) || return 1
  printf '%s' "$output" | awk 'NR == 1 && $1 == "mold" { print $2 }'
}
