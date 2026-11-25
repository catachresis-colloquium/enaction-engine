#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Guard against generated maintenance recipes that erase unrelated working-tree edits.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
GENERATED="$PROJECT_DIR/build/contractile.just"
FAILURES=0

pass() { printf 'PASS: %s\n' "$1"; }
fail() {
    printf 'FAIL: %s\n' "$1" >&2
    FAILURES=$((FAILURES + 1))
}

if [ ! -f "$GENERATED" ]; then
    printf 'FAIL: generated contractile recipes are missing: %s\n' "$GENERATED" >&2
    exit 1
fi

expect_absent() {
    local pattern="$1"
    local description="$2"
    local status
    if grep -nE "$pattern" "$GENERATED"; then
        fail "$description"
    else
        status=$?
        if [ "$status" -eq 1 ]; then
            pass "$description"
        else
            fail "unable to scan generated contractile recipes for: $description (grep exit $status)"
        fi
    fi
}

expect_absent '^dust-source-rollback:' 'no whole-tree rollback recipe is exposed'
expect_absent 'git[[:space:]]+checkout[[:space:]]+HEAD[[:space:]]+--[[:space:]]+\.' 'no recipe discards every tracked working-tree edit'

if grep -qF 'No automatic source rollback is configured' "$GENERATED"; then
    pass 'dust-status clearly states that automatic source rollback is unavailable'
else
    fail 'dust-status must clearly state that automatic source rollback is unavailable'
fi

if grep -Eq '^must-check:.*must-project-ci' "$GENERATED"; then
    pass 'must-check includes the project CI gate'
else
    fail 'must-check must include the project CI gate'
fi

if grep -Eq '^must-project-ci:' "$GENERATED" && grep -qF 'just validate && just ci' "$GENERATED"; then
    pass 'project CI gate runs repository validation and the full local CI recipe'
else
    fail 'project CI gate must run repository validation and the full local CI recipe'
fi

if [ "$FAILURES" -ne 0 ]; then
    printf '\nContractile safety tests: %d failure(s)\n' "$FAILURES" >&2
    exit 1
fi
printf '\nAll contractile safety tests passed.\n'
