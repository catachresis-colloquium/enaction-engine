#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Build immutable scanner/tool revisions used by static-analysis-gate.yml.
set -euo pipefail

readonly RUST_TOOLCHAIN="1.97.1"
readonly PANIC_ATTACK_REPO="hyperpolymath/panic-attack"
readonly PANIC_ATTACK_COMMIT="b5530834a839734d1d181bfe6edb0d43d4233cf4"
readonly PONS_REPO="hyperpolymath/pons-asinorum"
readonly PONS_COMMIT="51b8920f541782ceb9465ea3f0d33c96104fc1fb"
readonly IJSON_JCS_REPO="hyperpolymath/ijson-jcs"
readonly IJSON_JCS_COMMIT="2c880594c8b1f823a7ab2a8a830c894975a1792e"

usage() {
  echo "Usage: $0 OUTPUT_DIRECTORY" >&2
  exit 2
}

[[ "$#" -eq 1 ]] || usage
output_dir="$1"
mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"

command -v git >/dev/null 2>&1 || { echo "git is required" >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "rustup is required" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "cargo is required" >&2; exit 1; }

rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal --no-self-update

build_root="$(mktemp -d)"
trap "rm -rf -- '$build_root'" EXIT

clone_commit() {
  local slug="$1" commit="$2" destination="$3"
  [[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "invalid source pin: $slug@$commit" >&2; exit 1; }
  git init -q "$destination"
  git -C "$destination" remote add origin "https://github.com/$slug.git"
  git -C "$destination" fetch -q --depth=1 origin "$commit"
  git -C "$destination" checkout -q --detach FETCH_HEAD
  local actual
  actual="$(git -C "$destination" rev-parse HEAD)"
  [[ "$actual" == "$commit" ]] || { echo "source pin mismatch: $slug wanted $commit, got $actual" >&2; exit 1; }
}

panic_dir="$build_root/panic-attack"
pons_dir="$build_root/pons-asinorum"
ijson_dir="$build_root/ijson-jcs"
clone_commit "$PANIC_ATTACK_REPO" "$PANIC_ATTACK_COMMIT" "$panic_dir"
clone_commit "$PONS_REPO" "$PONS_COMMIT" "$pons_dir"
clone_commit "$IJSON_JCS_REPO" "$IJSON_JCS_COMMIT" "$ijson_dir"

cargo "+$RUST_TOOLCHAIN" build --locked --release --manifest-path "$panic_dir/Cargo.toml" --features http --bin panic-attack
cargo "+$RUST_TOOLCHAIN" build --locked --release --manifest-path "$pons_dir/Cargo.toml" --package pons-cli --bin pons
cargo "+$RUST_TOOLCHAIN" build --locked --release --manifest-path "$ijson_dir/Cargo.toml" --bin ijson-jcs

install -m 0755 "$panic_dir/target/release/panic-attack" "$output_dir/panic-attack"
install -m 0755 "$pons_dir/target/release/pons" "$output_dir/pons"
install -m 0755 "$ijson_dir/target/release/ijson-jcs" "$output_dir/ijson-jcs"

"$output_dir/panic-attack" --version
"$output_dir/pons" --version
"$output_dir/ijson-jcs" --version

# Fail the build if a pinned source revision changes its documented command
# surface; the report tests below exercise the real ijson-jcs implementation.
"$output_dir/panic-attack" --quiet assail --help >/dev/null
"$output_dir/panic-attack" --quiet bridge triage --help >/dev/null
"$output_dir/pons" scan --help >/dev/null
"$output_dir/pons" validate-report --help >/dev/null
"$output_dir/ijson-jcs" --help >/dev/null
