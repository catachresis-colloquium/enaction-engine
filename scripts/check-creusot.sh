#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Fail-closed Creusot gate for enaction-version, the proved leaf crate: the only
# crate that depends on creusot-std, proved as a whole. Tool presence and
# version are part of the gate.

set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  echo "FAIL: cargo is required for the Creusot proof gate." >&2
  exit 1
fi

if ! cargo creusot --help >/dev/null 2>&1; then
  echo "FAIL: cargo-creusot is not installed or cannot start." >&2
  exit 1
fi

version_output="$(cargo creusot version)"
if ! grep -Fxq 'cargo-creusot 0.13.0' <<<"$version_output"; then
  echo "FAIL: expected cargo-creusot 0.13.0." >&2
  printf '%s\n' "$version_output" >&2
  exit 1
fi
if ! grep -Fxq 'Rust toolchain nightly-2026-06-22' <<<"$version_output"; then
  echo "FAIL: expected Creusot's nightly-2026-06-22 toolchain." >&2
  printf '%s\n' "$version_output" >&2
  exit 1
fi
if grep -Fq 'not found' <<<"$version_output"; then
  echo "FAIL: one or more configured Creusot proof tools are absent." >&2
  printf '%s\n' "$version_output" >&2
  exit 1
fi

cargo creusot -p enaction-version --no-cache
