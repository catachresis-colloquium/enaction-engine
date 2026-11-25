#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Reject Idris2 trust escapes in the production accelerator ABI authority.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

SOURCES=(
    src/interface/Abi/Accelerator.idr
    src/interface/Abi/Generate.idr
)
for source in "${SOURCES[@]}"; do
    if [ ! -f "$source" ]; then
        printf 'FAIL: required Idris2 ABI source is missing: %s\n' "$source" >&2
        exit 1
    fi
done

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
if grep -nE 'believe_me|really_believe_me|assert_total|postulate' "${SOURCES[@]}" \
    >"$TMP_DIR/findings" 2>"$TMP_DIR/errors"; then
    printf 'FAIL: unsound proof escape found in the production ABI authority:\n' >&2
    cat "$TMP_DIR/findings" >&2
    exit 1
else
    status=$?
    if [ "$status" -ne 1 ]; then
        printf 'FAIL: Idris2 ABI safety scan failed (grep exit %s):\n' "$status" >&2
        cat "$TMP_DIR/errors" >&2
        exit 1
    fi
fi

printf 'PASS: no Idris2 trust escapes in the production accelerator ABI sources\n'
