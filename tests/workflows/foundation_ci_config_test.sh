#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>

# Contract tests for the foundation CI configuration managed in this repository.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEPENDABOT="$PROJECT_DIR/.github/dependabot.yml"
CODEQL="$PROJECT_DIR/.github/workflows/codeql.yml"
GOVERNANCE="$PROJECT_DIR/.github/workflows/governance.yml"
HYPATIA="$PROJECT_DIR/.github/workflows/hypatia-scan.yml"
SCORECARD="$PROJECT_DIR/.github/workflows/scorecard.yml"
ACTION_LOCK="$PROJECT_DIR/.github/workflows/actions.lock"
WORKFLOW_LINTER="$PROJECT_DIR/.github/workflows/workflow-linter.yml"

PASS=0
FAIL=0

pass() {
  printf 'PASS: %s\n' "$1"
  PASS=$((PASS + 1))
}

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  if [ -n "${2:-}" ]; then
    printf '      %s\n' "$2" >&2
  fi
  FAIL=$((FAIL + 1))
}

assert_eq() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  if [ "$actual" = "$expected" ]; then
    pass "$label"
  else
    fail "$label" "expected '$expected', got '${actual:-<empty>}'"
  fi
}

assert_sha() {
  local label="$1"
  local value="$2"

  if [[ "$value" =~ ^[0-9a-f]{40}$ ]]; then
    pass "$label"
  else
    fail "$label" "expected a lowercase 40-character commit SHA, got '${value:-<empty>}'"
  fi
}

dependabot_limit() {
  local ecosystem="$1"

  awk -v wanted="$ecosystem" '
    /^[[:space:]]*-[[:space:]]*package-ecosystem:/ {
      current = $0
      sub(/^[^:]+:[[:space:]]*/, "", current)
      gsub(/["[:space:]]/, "", current)
      next
    }
    current == wanted && /^[[:space:]]*open-pull-requests-limit:/ {
      value = $0
      sub(/^[^:]+:[[:space:]]*/, "", value)
      gsub(/[[:space:]]/, "", value)
      print value
    }
  ' "$DEPENDABOT"
}

action_refs() {
  local action="$1"
  local workflow="$2"

  awk -v action="$action" '
    /^[[:space:]]*uses:[[:space:]]*/ {
      value = $0
      sub(/^[[:space:]]*uses:[[:space:]]*/, "", value)
      sub(/[[:space:]]*#.*/, "", value)
      separator = substr(value, length(action) + 1, 1)
      if (index(value, action) == 1 && (separator == "@" || separator == "/")) {
        ref = value
        sub(/^.*@/, "", ref)
        print ref
      }
    }
  ' "$workflow"
}

locked_commit() {
  local dependency="$1"

  awk -v wanted="    '$dependency':" '
    $0 == wanted { in_entry = 1; next }
    in_entry && substr($0, 1, 4) == "    " && substr($0, 5, 1) != " " && $0 != wanted { exit }
    in_entry && /^[[:space:]]*commit:/ {
      value = $0
      sub(/^.*commit:[[:space:]]*/, "", value)
      gsub(/[\047\"]/, "", value)
      sub(/^sha1-/, "", value)
      print value
      exit
    }
  ' "$ACTION_LOCK"
}

checkout_persist_credentials() {
  awk '
    /^[[:space:]]+-[[:space:]]+(name|uses|run):/ && checkout { exit }
    /^[[:space:]]*uses:[[:space:]]*actions\/checkout@/ { checkout = 1; next }
    checkout && /^[[:space:]]*persist-credentials:/ {
      value = $0
      sub(/^[^:]+:[[:space:]]*/, "", value)
      gsub(/[[:space:]]/, "", value)
      print value
      exit
    }
  ' "$CODEQL"
}

for required_file in "$DEPENDABOT" "$CODEQL" "$GOVERNANCE" "$HYPATIA" "$SCORECARD" "$ACTION_LOCK" "$WORKFLOW_LINTER"; do
  if [ ! -f "$required_file" ]; then
    fail "configuration file exists: ${required_file#"$PROJECT_DIR/"}"
  fi
done

assert_eq 'GitHub Actions updates allow at most two open pull requests' \
  '2' "$(dependabot_limit github-actions)"
assert_eq 'Mix updates allow at most three open pull requests' \
  '3' "$(dependabot_limit mix)"
assert_eq 'npm updates allow at most three open pull requests' \
  '3' "$(dependabot_limit npm)"
assert_eq 'pip updates allow at most three open pull requests' \
  '3' "$(dependabot_limit pip)"

# Regression boundary: Cargo deliberately remains security-only while the
# neighbouring ecosystem blocks receive non-zero routine-update limits.
assert_eq 'Cargo routine version updates remain suppressed' \
  '0' "$(dependabot_limit cargo)"

assert_eq 'Workflow linter runs for every pull request' \
  '1' "$(grep -c '^  pull_request:[[:space:]]*$' "$WORKFLOW_LINTER" || true)"
assert_eq 'Workflow linter is not path-filtered' \
  '0' "$(grep -Ec '^[[:space:]]+paths(-ignore)?:' "$WORKFLOW_LINTER" || true)"
assert_eq 'Workflow linter runs on main pushes' \
  '1' "$(grep -c '^      - main$' "$WORKFLOW_LINTER" || true)"

mapfile -t checkout_refs < <(action_refs actions/checkout "$CODEQL")
assert_eq 'CodeQL has exactly one checkout step' '1' "${#checkout_refs[@]}"
if [ "${#checkout_refs[@]}" -eq 1 ]; then
  assert_eq 'CodeQL checkout uses the locked immutable release tag' 'v7.0.1' "${checkout_refs[0]}"
  assert_eq 'CodeQL checkout tag resolves to the locked commit' \
    '3d3c42e5aac5ba805825da76410c181273ba90b1' "$(locked_commit actions/checkout@v7.0.1)"
fi
assert_eq 'CodeQL checkout does not persist repository credentials' \
  'false' "$(checkout_persist_credentials)"

mapfile -t codeql_refs < <(action_refs github/codeql-action "$CODEQL")
assert_eq 'CodeQL has one init and one analysis action step' '2' "${#codeql_refs[@]}"
codeql_init_ref="$(action_refs github/codeql-action/init "$CODEQL")"
codeql_analyze_ref="$(action_refs github/codeql-action/analyze "$CODEQL")"
assert_eq 'CodeQL init uses the locked immutable release tag' 'v4.39.0' "$codeql_init_ref"
assert_eq 'CodeQL analysis uses the locked immutable release tag' 'v4.39.0' "$codeql_analyze_ref"
assert_eq 'CodeQL release tag resolves to the locked commit' \
  '6f2ac81f5cd81fe351a197ba0b00d91d03cd2aee' "$(locked_commit github/codeql-action@v4.39.0)"

governance_ref="$(action_refs hyperpolymath/standards/.github/workflows/governance-reusable.yml "$GOVERNANCE")"
hypatia_ref="$(action_refs hyperpolymath/standards/.github/workflows/hypatia-scan-reusable.yml "$HYPATIA")"
scorecard_ref="$(action_refs hyperpolymath/standards/.github/workflows/scorecard-reusable.yml "$SCORECARD")"

assert_sha 'Governance reusable workflow is pinned to an immutable revision' "$governance_ref"
assert_eq 'Governance reusable workflow uses the approved revision' \
  'da2c748aad55c1a1dcba00b60fe4a35017bc6540' "$governance_ref"
assert_sha 'Hypatia reusable workflow is pinned to an immutable revision' "$hypatia_ref"
assert_eq 'Hypatia reusable workflow uses the approved revision' \
  'da2c748aad55c1a1dcba00b60fe4a35017bc6540' "$hypatia_ref"
assert_sha 'Scorecard reusable workflow is pinned to an immutable revision' "$scorecard_ref"
assert_eq 'Scorecard reusable workflow uses the approved revision' \
  'da2c748aad55c1a1dcba00b60fe4a35017bc6540' "$scorecard_ref"

printf '\nfoundation CI configuration tests: %d passed, %d failed\n' "$PASS" "$FAIL"
exit "$FAIL"
