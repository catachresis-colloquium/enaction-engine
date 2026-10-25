#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>

# Contract tests for the lock-sync gate's self-protecting workflow shape.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKFLOW="$PROJECT_DIR/.github/workflows/lock-sync-gate.yml"

PASS=0
FAIL=0

pass() {
  printf 'PASS: %s\n' "$1"
  PASS=$((PASS + 1))
}

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  FAIL=$((FAIL + 1))
}

assert_present() {
  local label="$1"
  local pattern="$2"

  if grep -Eq "$pattern" "$WORKFLOW"; then
    pass "$label"
  else
    fail "$label"
  fi
}

assert_absent() {
  local label="$1"
  local pattern="$2"

  if grep -Eq "$pattern" "$WORKFLOW"; then
    fail "$label"
  else
    pass "$label"
  fi
}

if [ ! -f "$WORKFLOW" ]; then
  printf 'FAIL: workflow not found: %s\n' "$WORKFLOW" >&2
  exit 1
fi

assert_present 'runs for pull requests' '^[[:space:]]{2}pull_request:[[:space:]]*$'
assert_present 'runs for pushes' '^[[:space:]]{2}push:[[:space:]]*$'
assert_present 'can be dispatched manually after an unrerunnable startup failure' '^[[:space:]]{2}workflow_dispatch:[[:space:]]*$'
assert_present 'manual dispatch can request the official lockfile generator' '^[[:space:]]{6}generate_lockfile:[[:space:]]*$'
assert_present 'manual lockfile generation is opt-in by default' '^[[:space:]]{8}default:[[:space:]]*false[[:space:]]*$'
assert_present 'limits push runs to main' '^[[:space:]]+branches:[[:space:]]*\[main\][[:space:]]*$'
assert_present 'uses read-only repository contents permission' '^[[:space:]]+contents:[[:space:]]*read[[:space:]]*$'
assert_absent 'has no path filter that could suppress a required check' '^[[:space:]]+paths(-ignore)?:'
assert_absent 'has no action dependency that could itself require a lock entry' '^[[:space:]]*-?[[:space:]]*uses:'
assert_absent 'does not weaken failures with continue-on-error' '^[[:space:]]*continue-on-error:'
assert_present 'installs the official verifier at a fixed release' 'gh extension install github/gh-actions-lock --pin v0\.2\.0'
assert_present 'manual candidate uses the official generator without migrating refs' 'gh actions-lock --no-migrate-local-actions --no-narrow'
assert_present 'manual candidate is surfaced for review without writing to the branch' 'cat \.github/workflows/actions\.lock'
assert_present 'runs official lockfile and runner-parity verification' 'gh actions-lock --verify'
assert_present 'runs local structural verification even when official verification fails' '^[[:space:]]{8}if: always\(\)$'
assert_present 'verifies that the local checker exists and is executable' 'test -x scripts/check-lock-sync\.sh'
assert_present 'executes the repository checker directly' '^[[:space:]]*\./scripts/check-lock-sync\.sh[[:space:]]*$'

printf '\nlock-sync-gate tests: %d passed, %d failed\n' "$PASS" "$FAIL"
exit "$FAIL"
