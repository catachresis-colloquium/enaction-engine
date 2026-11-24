#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>

# Unit tests for scripts/check-lock-sync.sh. Each case builds an isolated,
# minimal workflow directory so failures identify one lock-sync rule at a time.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$PROJECT_DIR/scripts/check-lock-sync.sh"
TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/check-lock-sync-test.XXXXXX")"
trap 'rm -rf "$TMP_ROOT"' EXIT

PASS=0
FAIL=0
STATUS=0
OUTPUT=""
WORKFLOWS_DIR=""

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

new_fixture() {
  WORKFLOWS_DIR="$TMP_ROOT/$1/.github/workflows"
  mkdir -p "$WORKFLOWS_DIR"
}

write_workflow() {
  local filename="$1"
  shift

  {
    printf '%s\n' 'name: Fixture' 'on: push' 'jobs:' '  check:' '    runs-on: ubuntu-latest' '    steps:'
    local ref
    for ref in "$@"; do
      printf '      - uses: %s\n' "$ref"
    done
  } >"$WORKFLOWS_DIR/$filename"
}

write_closed_lock() {
  local filename="$1"
  shift

  {
    printf '%s\n' "version: 'v0.0.2'" 'workflows:'
    if [ "$#" -eq 0 ]; then
      printf "    '.github/workflows/%s': []\n" "$filename"
    else
      printf "    '.github/workflows/%s':\n" "$filename"
      local ref
      for ref in "$@"; do
        printf "        - '%s'\n" "$ref"
      done
    fi
    printf '%s\n' 'dependencies:'
    local ref
    for ref in "$@"; do
      printf "    '%s':\n" "$ref"
      printf "        ref: '%s'\n" "${ref##*@}"
    done
  } >"$WORKFLOWS_DIR/actions.lock"
}

run_checker() {
  if OUTPUT="$(bash "$CHECKER" "$WORKFLOWS_DIR" 2>&1)"; then
    STATUS=0
  else
    STATUS=$?
  fi
}

run_checker_with_path() {
  local path="$1"
  if OUTPUT="$(PATH="$path" bash "$CHECKER" "$WORKFLOWS_DIR" 2>&1)"; then
    STATUS=0
  else
    STATUS=$?
  fi
}

assert_success() {
  local label="$1"
  local expected="${2:-}"

  if [ "$STATUS" -ne 0 ]; then
    fail "$label" "expected exit 0, got $STATUS: $OUTPUT"
  elif [ -n "$expected" ] && [[ "$OUTPUT" != *"$expected"* ]]; then
    fail "$label" "output did not contain '$expected': $OUTPUT"
  else
    pass "$label"
  fi
}

assert_failure() {
  local label="$1"
  local expected="${2:-}"

  if [ "$STATUS" -eq 0 ]; then
    fail "$label" "expected a non-zero exit: $OUTPUT"
  elif [ -n "$expected" ] && [[ "$OUTPUT" != *"$expected"* ]]; then
    fail "$label" "output did not contain '$expected': $OUTPUT"
  else
    pass "$label"
  fi
}

assert_output_contains() {
  local label="$1"
  local expected="$2"

  if [[ "$OUTPUT" == *"$expected"* ]]; then
    pass "$label"
  else
    fail "$label" "output did not contain '$expected': $OUTPUT"
  fi
}

test_accepts_synchronised_lock() {
  new_fixture synchronised
  write_workflow build.yml 'actions/checkout@abc123'
  write_closed_lock build.yml 'actions/checkout@abc123'

  run_checker
  assert_success 'accepts a synchronised, transitively closed lock' '0 dangling edges'
}

test_accepts_listed_zero_uses_workflow() {
  new_fixture listed-zero-uses
  write_workflow housekeeping.yml
  write_closed_lock housekeeping.yml

  run_checker
  assert_success 'accepts an empty lock entry for a workflow with no external uses' 'zero-uses: workflows included'
}

test_accepts_yaml_extension_and_normalises_subpath() {
  new_fixture action-subpath
  {
    printf '%s\n' \
      'name: Action subpath fixture' \
      'on: push' \
      'jobs:' \
      '  build:' \
      '    runs-on: ubuntu-latest' \
      '    steps:' \
      '      - uses: "Acme/Tools/sub-action@Main" # pinned action in a subdirectory'
  } >"$WORKFLOWS_DIR/subpath.yaml"
  write_closed_lock subpath.yaml 'acme/tools@Main'

  run_checker
  assert_success 'discovers .yaml workflows, strips quotes/comments, normalises subpaths, and folds repository case'
}

# gh actions-lock v0.2.0 (the official verifier the gate runs) does not lock
# job-level reusable-workflow calls: it generates '[]' for a workflow whose only
# uses: is such a call, and `--verify` rejects an entry for one as "Unused
# lockfile entry". Measured on this repository, 2026-11-14 (Lock Sync Gate
# run 43650652741). The local checker must agree, or the two gates can never
# both pass.
write_reusable_caller() {
  {
    printf '%s\n' \
      'name: Reusable workflow caller' \
      'on: push' \
      'jobs:' \
      '  call:' \
      '    uses: "Acme/Reusable/.github/workflows/build.yml@Main" # pinned reusable workflow'
  } >"$WORKFLOWS_DIR/$1"
}

test_accepts_reusable_workflow_call_with_empty_entry() {
  new_fixture reusable-workflow-empty
  write_reusable_caller reuse.yaml
  write_closed_lock reuse.yaml

  run_checker
  assert_success 'accepts an empty lock entry for a job-level reusable-workflow call' '0 dangling edges'
}

test_rejects_lock_entry_for_reusable_workflow_call() {
  new_fixture reusable-workflow-locked
  write_reusable_caller reuse.yaml
  write_closed_lock reuse.yaml 'acme/reusable@Main'

  run_checker
  assert_failure 'rejects a lock entry for a job-level reusable-workflow call, as gh actions-lock --verify does' \
    'stale lockfile entries, no uses: references them: acme/reusable@Main'
}

test_ignores_local_and_non_action_uses() {
  new_fixture local-action
  write_workflow build.yml './local-action' 'docker://alpine:3.22'
  write_closed_lock build.yml

  run_checker
  assert_success 'does not require lock entries for local or docker actions'
}

test_accepts_duplicate_uses_once() {
  new_fixture duplicate
  write_workflow build.yml 'actions/checkout@abc123' 'actions/checkout@abc123'
  write_closed_lock build.yml 'actions/checkout@abc123'

  run_checker
  assert_success 'deduplicates repeated uses references in one workflow'
}

test_requires_lockfile() {
  new_fixture missing-lock
  write_workflow build.yml 'actions/checkout@abc123'

  run_checker
  assert_failure 'rejects a missing lockfile' 'FATAL: no lockfile'
}

test_requires_workflow_files() {
  new_fixture missing-workflows
  {
    printf '%s\n' "version: 'v0.0.2'" 'workflows:' 'dependencies:'
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects a lockfile when no workflow files exist'
}

test_works_without_gnu_awk() {
  new_fixture posix-awk
  write_workflow build.yml 'actions/checkout@abc123'
  write_closed_lock build.yml 'actions/checkout@abc123'
  mkdir -p "$TMP_ROOT/failing-gawk-bin"
  {
    printf '%s\n' '#!/usr/bin/env bash' 'exit 1'
  } >"$TMP_ROOT/failing-gawk-bin/gawk"
  chmod +x "$TMP_ROOT/failing-gawk-bin/gawk"

  run_checker_with_path "$TMP_ROOT/failing-gawk-bin:$PATH"
  assert_success 'validates the lock with POSIX awk when GNU awk is unavailable' '0 dangling edges'
}

test_reports_unonboarded_workflow() {
  new_fixture unonboarded
  write_workflow build.yml 'actions/checkout@abc123'
  {
    printf '%s\n' "version: 'v0.0.2'" 'workflows:' 'dependencies:' \
      "    'actions/checkout@abc123':" "        ref: 'abc123'"
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects a workflow with no path entry in the lockfile' 'not onboarded'
  assert_output_contains 'also reports an action-bearing workflow as unlisted' 'FAIL actions.lock: UNLISTED WORKFLOWS'
}

test_reports_unlisted_zero_uses_workflow() {
  new_fixture unlisted-zero-uses
  write_workflow housekeeping.yml
  {
    printf '%s\n' "version: 'v0.0.2'" 'workflows:' 'dependencies:'
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects an unlisted workflow even when it has no external uses' 'FAIL actions.lock: UNLISTED WORKFLOWS'
  assert_output_contains 'counts a single unlisted workflow' '1 workflow file(s) have no key in the lockfile'
  assert_output_contains 'names the unlisted workflow by its canonical lock path' '.github/workflows/housekeeping.yml'
  assert_output_contains 'explains the empty-list remediation for zero-use workflows' "'.github/workflows/x.yml': []"
}

test_reports_unlisted_local_only_workflow() {
  new_fixture unlisted-local-only
  write_workflow local.yml './local-action' 'docker://alpine:3.22'
  {
    printf '%s\n' "version: 'v0.0.2'" 'workflows:' 'dependencies:'
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects an unlisted workflow whose uses do not require dependency locks' 'FAIL actions.lock: UNLISTED WORKFLOWS'
  assert_output_contains 'names the uncovered local-only workflow' '.github/workflows/local.yml'
}

test_reports_every_unlisted_workflow() {
  new_fixture multiple-unlisted
  write_workflow listed.yml
  write_workflow first-missing.yml
  write_workflow second-missing.yaml
  write_closed_lock listed.yml

  run_checker
  assert_failure 'rejects multiple unlisted workflows together' '2 workflow file(s) have no key in the lockfile'
  assert_output_contains 'names an unlisted .yml workflow' '.github/workflows/first-missing.yml'
  assert_output_contains 'names an unlisted .yaml workflow' '.github/workflows/second-missing.yaml'
}

test_requires_exact_workflow_path_coverage() {
  new_fixture exact-path
  write_workflow build.yml
  write_workflow build-extra.yml
  write_closed_lock build-extra.yml

  run_checker
  assert_failure 'does not let a similarly named lock key cover another workflow' '1 workflow file(s) have no key in the lockfile'
  assert_output_contains 'reports the exact uncovered path when lock keys share a prefix' '.github/workflows/build.yml'
}

test_reports_ref_missing_from_existing_path() {
  new_fixture missing-ref
  write_workflow build.yml 'actions/checkout@abc123' 'actions/cache@def456'
  write_closed_lock build.yml 'actions/checkout@abc123'
  {
    printf "    'actions/cache@def456':\n        ref: 'def456'\n"
  } >>"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects an unlocked ref under an existing workflow path' 'refs missing from the lockfile: actions/cache@def456'
}

test_reports_stale_lock_entry() {
  new_fixture stale-ref
  write_workflow build.yml 'actions/checkout@abc123'
  write_closed_lock build.yml 'actions/checkout@abc123' 'actions/cache@def456'

  run_checker
  assert_failure 'rejects a lock entry no longer used by its workflow' 'stale lockfile entries, no uses: references them: actions/cache@def456'
}

test_requires_each_workflow_to_own_its_refs() {
  new_fixture per-workflow
  write_workflow first.yml 'actions/checkout@abc123'
  write_workflow second.yml 'actions/checkout@abc123'
  write_closed_lock first.yml 'actions/checkout@abc123'

  run_checker
  assert_failure 'requires a shared action to be locked under every workflow that uses it' 'FAIL .github/workflows/second.yml'
}

test_reports_deleted_workflow_entry() {
  new_fixture deleted-workflow
  write_workflow current.yml 'actions/checkout@abc123'
  {
    printf '%s\n' \
      "version: 'v0.0.2'" \
      'workflows:' \
      "    '.github/workflows/current.yml':" \
      "        - 'actions/checkout@abc123'" \
      "    '.github/workflows/deleted.yml': []" \
      'dependencies:' \
      "    'actions/checkout@abc123':" \
      "        ref: 'abc123'"
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects lock entries for deleted workflow files' 'lockfile entry for a workflow file that does not exist'
}

test_ref_case_remains_sensitive() {
  new_fixture ref-case
  write_workflow build.yml 'actions/checkout@main'
  write_closed_lock build.yml 'actions/checkout@Main'
  {
    printf "    'actions/checkout@main':\n        ref: 'main'\n"
  } >>"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'treats git ref case as significant' 'refs missing from the lockfile: actions/checkout@main'
}

test_rejects_corrupted_local_action_rewrite() {
  new_fixture dollar-local
  write_workflow build.yml '$/local-action'
  write_closed_lock build.yml

  run_checker
  assert_failure 'rejects the known $/ local-action corruption' 'invalid local-action rewrite (uses: $/...)'
}

test_reports_direct_dangling_edge() {
  new_fixture direct-dangling
  write_workflow build.yml 'actions/checkout@abc123'
  {
    printf '%s\n' \
      "version: 'v0.0.2'" \
      'workflows:' \
      "    '.github/workflows/build.yml':" \
      "        - 'actions/checkout@abc123'" \
      'dependencies:'
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects a workflow ref without a dependency record' 'DANGLING EDGES'
}

test_reports_nested_dangling_edge() {
  new_fixture nested-dangling
  write_workflow build.yml 'acme/parent@v1'
  {
    printf '%s\n' \
      "version: 'v0.0.2'" \
      'workflows:' \
      "    '.github/workflows/build.yml':" \
      "        - 'acme/parent@v1'" \
      'dependencies:' \
      "    'acme/parent@v1':" \
      "        ref: 'v1'" \
      '        uses:' \
      "            - 'acme/child@v2'"
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects a dependency whose nested action is unresolved' 'named by: dependencies:acme/parent@v1'
}

test_accepts_transitive_closure() {
  new_fixture transitive-closure
  write_workflow build.yml 'acme/parent/path@v1'
  {
    printf '%s\n' \
      "version: 'v0.0.2'" \
      'workflows:' \
      "    '.github/workflows/build.yml':" \
      "        - 'acme/parent@v1'" \
      'dependencies:' \
      "    'acme/parent@v1':" \
      "        ref: 'v1'" \
      '        uses:' \
      "            - 'acme/child@v2'" \
      "    'acme/child@v2':" \
      "        ref: 'v2'" \
      '        uses:' \
      "            - 'acme/leaf@v3'" \
      "    'acme/leaf@v3':" \
      "        ref: 'v3'"
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_success 'accepts a fully closed multi-level dependency graph'
}

test_nested_subpath_does_not_satisfy_schema_key() {
  new_fixture nested-subpath
  write_workflow build.yml 'acme/parent@v1'
  {
    printf '%s\n' \
      "version: 'v0.0.2'" \
      'workflows:' \
      "    '.github/workflows/build.yml':" \
      "        - 'acme/parent@v1'" \
      'dependencies:' \
      "    'acme/parent@v1':" \
      "        ref: 'v1'" \
      '        uses:' \
      "            - 'acme/child/sub-action@v2'" \
      "    'acme/child@v2':" \
      "        ref: 'v2'"
  } >"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_failure 'rejects nested uses with a subpath instead of a schema-valid dependency key' 'acme/child/sub-action@v2'
}

test_allows_unreferenced_dependency_record() {
  new_fixture unreferenced-dependency
  write_workflow build.yml 'actions/checkout@abc123'
  write_closed_lock build.yml 'actions/checkout@abc123'
  {
    printf "    'actions/cache@def456':\n        ref: 'def456'\n"
  } >>"$WORKFLOWS_DIR/actions.lock"

  run_checker
  assert_success 'allows but reports an unreferenced dependency record' '1 dependencies: record(s) are unreferenced'
}

test_accepts_synchronised_lock
test_accepts_listed_zero_uses_workflow
test_accepts_yaml_extension_and_normalises_subpath
test_accepts_reusable_workflow_call_with_empty_entry
test_rejects_lock_entry_for_reusable_workflow_call
test_ignores_local_and_non_action_uses
test_accepts_duplicate_uses_once
test_requires_lockfile
test_requires_workflow_files
test_works_without_gnu_awk
test_reports_unonboarded_workflow
test_reports_unlisted_zero_uses_workflow
test_reports_unlisted_local_only_workflow
test_reports_every_unlisted_workflow
test_requires_exact_workflow_path_coverage
test_reports_ref_missing_from_existing_path
test_reports_stale_lock_entry
test_requires_each_workflow_to_own_its_refs
test_reports_deleted_workflow_entry
test_ref_case_remains_sensitive
test_rejects_corrupted_local_action_rewrite
test_reports_direct_dangling_edge
test_reports_nested_dangling_edge
test_accepts_transitive_closure
test_nested_subpath_does_not_satisfy_schema_key
test_allows_unreferenced_dependency_record

printf '\ncheck-lock-sync tests: %d passed, %d failed\n' "$PASS" "$FAIL"
exit "$FAIL"
