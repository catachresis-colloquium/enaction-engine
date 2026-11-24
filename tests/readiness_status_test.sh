#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Ensure the public readiness record is an honest status, not an unfilled grade template.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPORT="$PROJECT_DIR/docs/status/READINESS.adoc"
FAILURES=0

pass() { printf 'PASS: %s\n' "$1"; }
fail() {
    printf 'FAIL: %s\n' "$1" >&2
    FAILURES=$((FAILURES + 1))
}

if [ ! -f "$REPORT" ]; then
    printf 'FAIL: readiness status report is missing: %s\n' "$REPORT" >&2
    exit 1
fi

expect_absent() {
    local pattern="$1"
    local description="$2"
    local status
    if grep -Eiq -- "$pattern" "$REPORT"; then
        fail "$description"
    else
        status=$?
        if [ "$status" -eq 1 ]; then
            pass "$description"
        else
            fail "unable to scan readiness report for $description (grep exit $status)"
        fi
    fi
}

expect_present() {
    local pattern="$1"
    local description="$2"
    local status
    if grep -Eiq -- "$pattern" "$REPORT"; then
        pass "$description"
    else
        status=$?
        if [ "$status" -eq 1 ]; then
            fail "$description"
        else
            fail "unable to scan readiness report for $description (grep exit $status)"
        fi
    fi
}

expect_absent '(<component[^>]*>|_<[^>]*>_|Current Grade:[[:space:]]*X|just crg-grade|just crg-badge|CRG-AUDIT-<)' \
    'readiness report has no unfilled grade fields or badge-generation claims'
expect_present '^\*Formal assessment:\*[[:space:]]*\*not conducted\*' \
    'readiness report explicitly records that no formal assessment was conducted'
expect_present '^\*Aggregate grade:\*[[:space:]]*\*none assigned\*' \
    'readiness report explicitly records that no aggregate grade is assigned'
expect_absent '^image:' 'readiness report publishes no badge'

if ((FAILURES > 0)); then
    printf '%d readiness status test(s) failed.\n' "$FAILURES" >&2
    exit 1
fi

printf 'All readiness status tests passed.\n'
