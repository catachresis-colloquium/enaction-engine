#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>

# Regression tests for the README's OpenSSF badge policy. Until this project
# has its own bestpractices.dev registration, no project-specific badge or link
# may be shown: a valid URL can still certify an unrelated repository.

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
README_PATH="$PROJECT_DIR/README.adoc"
PASS=0
FAIL=0

has_project_specific_openssf_url() {
    grep -Eiq 'https?://(www\.)?bestpractices\.dev/projects/[0-9]+' "$1"
}

pass() {
    printf 'PASS: %s\n' "$1"
    PASS=$((PASS + 1))
}

fail() {
    printf 'FAIL: %s\n' "$1" >&2
    FAIL=$((FAIL + 1))
}

expect_valid() {
    local label="$1"
    local content="$2"
    local fixture="$FIXTURE_DIR/valid-$((PASS + FAIL)).adoc"

    printf '%s\n' "$content" > "$fixture"
    if has_project_specific_openssf_url "$fixture"; then
        fail "$label"
    else
        pass "$label"
    fi
}

expect_invalid() {
    local label="$1"
    local content="$2"
    local fixture="$FIXTURE_DIR/invalid-$((PASS + FAIL)).adoc"

    printf '%s\n' "$content" > "$fixture"
    if has_project_specific_openssf_url "$fixture"; then
        pass "$label"
    else
        fail "$label"
    fi
}

FIXTURE_DIR="$(mktemp -d)"
trap 'rm -rf "$FIXTURE_DIR"' EXIT

if [ ! -f "$README_PATH" ]; then
    fail "repository README exists"
elif has_project_specific_openssf_url "$README_PATH"; then
    fail "repository README has no unverified OpenSSF project URL"
else
    pass "repository README has no unverified OpenSSF project URL"
fi

expect_invalid \
    "OpenSSF badge image is rejected" \
    'image:https://www.bestpractices.dev/projects/8509/badge[OpenSSF Best Practices]'

expect_invalid \
    "a different numeric project id is also rejected" \
    'image:https://bestpractices.dev/projects/12345/badge[OpenSSF Best Practices]'

expect_invalid \
    "project URL used only as a link target is rejected" \
    'image:https://img.shields.io/badge/OpenSSF-passing-green[OpenSSF,link=https://www.bestpractices.dev/projects/8509]'

expect_invalid \
    "the original malformed image macro cannot return" \
    'nimage:https://www.bestpractices.dev/projects/8509/badge[OpenSSF Best Practices]'

expect_invalid \
    "project URL matching is case insensitive" \
    'https://WWW.BESTPRACTICES.DEV/PROJECTS/8509'

expect_valid \
    "general OpenSSF documentation links remain allowed" \
    'Learn about OpenSSF Best Practices at https://www.bestpractices.dev/'

expect_valid \
    "unrelated badges remain allowed" \
    'image:https://img.shields.io/badge/CIAQ-Compliant-9f55ff[CIAQ Compliant]'

printf '\n%d passed; %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
