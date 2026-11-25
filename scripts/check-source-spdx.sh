#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Check SPDX headers on all implementation and proof source files.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

ROOTS=(crates src/interface verification tests examples)
for root in "${ROOTS[@]}"; do
    if [ ! -d "$root" ]; then
        printf 'FAIL: required source root is missing: %s\n' "$root" >&2
        exit 1
    fi
done

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
if ! find "${ROOTS[@]}" -type f \
    \( -name '*.rs' -o -name '*.zig' -o -name '*.idr' -o \
       -name '*.lean' -o -name '*.agda' -o -name '*.v' -o -name '*.hs' \) \
    -print0 >"$TMP_DIR/sources" 2>"$TMP_DIR/find-errors"; then
    printf 'FAIL: unable to inventory source files:\n' >&2
    cat "$TMP_DIR/find-errors" >&2
    exit 1
fi

mapfile -d '' SOURCE_FILES <"$TMP_DIR/sources"
if [ "${#SOURCE_FILES[@]}" -eq 0 ]; then
    echo 'FAIL: no implementation or proof source files were found' >&2
    exit 1
fi

missing=0
for file in "${SOURCE_FILES[@]}"; do
    if ! head -n 5 "$file" | grep -qF 'SPDX-License-Identifier:'; then
        printf 'FAIL: missing SPDX header: %s\n' "$file" >&2
        missing=1
    fi
done

if [ "$missing" -ne 0 ]; then
    exit 1
fi
printf 'PASS: SPDX headers present on all %d implementation and proof source files\n' "${#SOURCE_FILES[@]}"
