#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Negative control for the native accelerator ABI's 64-bit pointer contract.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
zig_cache="$(mktemp -d)"
trap 'rm -rf -- "$zig_cache"' EXIT

if output="$(cd "$repo_root/src/interface/ffi" && \
    ZIG_GLOBAL_CACHE_DIR="$zig_cache" \
      zig build -Dtarget=wasm32-freestanding 2>&1)"; then
  echo "FAIL: the native accelerator ABI unexpectedly admitted wasm32-freestanding." >&2
  exit 1
fi

expected="Enaction accelerator ABI v1 requires 64-bit pointers"
if ! grep -Fq "$expected" <<<"$output"; then
  echo "FAIL: wasm32-freestanding failed, but not at the 64-bit pointer guard." >&2
  printf '%s\n' "$output" >&2
  exit 1
fi

echo "PASS: wasm32-freestanding was rejected by the 64-bit pointer guard."
