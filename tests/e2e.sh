#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Cross-implementation accelerator conformance over the committed v1 corpus.
# The scalar Rust reference and Zig-backed native adapter must consume the same
# integrity-checked fixtures; missing toolchains are failures, never skips.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

PASS=0
FAIL=0

green() { printf '\033[32m%s\033[0m\n' "$*"; }
red() { printf '\033[31m%s\033[0m\n' "$*" >&2; }

printf '%s\n' '=== Enaction Engine accelerator conformance ==='

if (cd "$PROJECT_DIR/conformance/accelerator/v1" && sha256sum --check --quiet SHA256SUMS); then
    green 'PASS: conformance corpus integrity (SHA256SUMS)'
    PASS=$((PASS + 1))
else
    red 'FAIL: conformance corpus integrity (SHA256SUMS)'
    FAIL=$((FAIL + 1))
fi

if ! command -v cargo >/dev/null 2>&1; then
    red 'FAIL: cargo is required; scalar and native conformance tests were not run'
    FAIL=$((FAIL + 1))
else
    if cargo test --locked -p enaction-accelerator --test conformance; then
        green 'PASS: scalar reference vs locked corpus'
        PASS=$((PASS + 1))
    else
        red 'FAIL: scalar reference vs locked corpus'
        FAIL=$((FAIL + 1))
    fi

    # build.rs links the Zig library built from the pinned Zig toolchain.
    if cargo test --locked -p enaction-accelerator-native --test conformance; then
        green 'PASS: Zig-native backend vs locked corpus (parity)'
        PASS=$((PASS + 1))
    else
        red 'FAIL: Zig-native backend vs locked corpus (parity)'
        FAIL=$((FAIL + 1))
    fi
fi

printf '\nConformance summary: %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
