#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Contract tests for the pinned Proven-Tests CI integration and local runner.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="$ROOT/.github/workflows/proven-tests-gate.yml"
RUNNER="$ROOT/scripts/run-proven-tests.sh"
PACKAGE="$ROOT/verification/tests/enaction-engine-tests.ipkg"
TEST_SOURCE="$ROOT/verification/tests/EnactionEngineTests/AbiTests.idr"

pass_count=0
fail_count=0

assert_present() {
  local label="$1" pattern="$2" file="$3"
  if grep -Eq "$pattern" "$file"; then
    printf 'PASS: %s\n' "$label"
    pass_count=$((pass_count + 1))
  else
    printf 'FAIL: %s\n' "$label" >&2
    fail_count=$((fail_count + 1))
  fi
}

assert_absent() {
  local label="$1" pattern="$2" file="$3"
  if grep -Eq "$pattern" "$file"; then
    printf 'FAIL: %s\n' "$label" >&2
    fail_count=$((fail_count + 1))
  else
    printf 'PASS: %s\n' "$label"
    pass_count=$((pass_count + 1))
  fi
}

printf '%s\n' 'Proven-Tests integration contract tests'
assert_present 'workflow is a manual gate' '^  workflow_dispatch:' "$WORKFLOW"
assert_present 'workflow is a pull-request gate' '^  pull_request:' "$WORKFLOW"
assert_present 'workflow fetches the immutable framework commit' 'FRAMEWORK_COMMIT: eca682bcd419554f6779e72969902e6faef4fd9d' "$WORKFLOW"
assert_present 'workflow installs the pinned Idris2 compiler' 'IDRIS2_VERSION: v0\.7\.0' "$WORKFLOW"
assert_present 'workflow runs the repository test runner' 'run: scripts/run-proven-tests\.sh' "$WORKFLOW"
assert_present 'workflow checks the Idris2 proof manifest' 'scripts/check-proofs\.sh idris2' "$WORKFLOW"
assert_present 'workflow checkout uses an immutable action release tag' 'uses: actions/checkout@v7[.]0[.]1' "$WORKFLOW"
assert_present 'checkout does not persist credentials to later steps' 'persist-credentials: false' "$WORKFLOW"

assert_present 'runner verifies the exact framework revision' 'FRAMEWORK_COMMIT="eca682bcd419554f6779e72969902e6faef4fd9d"' "$RUNNER"
assert_present 'runner checks the Idris2 version' 'IDRIS2_VERSION="0\.7\.0"' "$RUNNER"
assert_present 'runner installs the upstream framework package' 'idris2 --install proven-tests\.ipkg' "$RUNNER"
assert_present 'runner builds and runs the repository test package' 'idris2 --build enaction-engine-tests\.ipkg' "$RUNNER"
assert_present 'package depends on the pinned framework and local ABI' 'depends = base, abi, proven-tests' "$PACKAGE"
assert_present 'test code uses the direct typed classifier' 'classifyProvisionallyProven' "$TEST_SOURCE"
assert_present 'test metadata records typed category and aspect axes' 'category := Just category, aspect := Just aspect' "$TEST_SOURCE"
assert_present 'test code carries typed runtime fixture pairs' 'Fixtures \(MkFixturePair' "$TEST_SOURCE"
assert_present 'compile-time negative fixture is exercised' '^failing$' "$TEST_SOURCE"
assert_absent 'new tests do not accrue FixtureDebt' 'FixtureDebt' "$TEST_SOURCE"
assert_absent 'new tests do not call the framework legacy provisionalTest helper' '(^|[^[:alnum:]_])provisionalTest[[:space:]]' "$TEST_SOURCE"

printf '\nproven-tests integration tests: %d passed, %d failed\n' "$pass_count" "$fail_count"
[[ "$fail_count" -eq 0 ]]
