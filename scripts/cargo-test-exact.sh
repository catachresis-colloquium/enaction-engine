#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# cargo-test-exact.sh <package> <test-name>
#
# Run exactly one named test and fail unless exactly one test ran and passed.
#
# Why: `cargo test <name>` is a substring filter, and a filter that matches
# nothing exits 0 with "running 0 tests". A done_when that names a test which
# was misspelled, renamed or deleted therefore reads as green. This script
# passes `--exact` and then counts, across every test target cargo runs (unit,
# integration, doc), the tests run and the tests passed. It requires:
#
#   * cargo exits 0;
#   * the summed "running N tests" is exactly 1;
#   * the summed "N passed" is exactly 1, and no target reports a failure.
#
# Unit tests need their module path (`store::tests::name`); a top-level test
# in tests/<file>.rs is just its function name.
#
# Exit: 0 = exactly one test ran and passed; 1 = otherwise; 2 = misuse.
# Tested both ways by tests/check-cargo-test-exact.sh.

set -euo pipefail

if [ "$#" -ne 2 ] || [ -z "$1" ] || [ -z "$2" ]; then
  echo "usage: $(basename "$0") <package> <test-name>" >&2
  exit 2
fi
package="$1"
name="$2"

cd "$(dirname "${BASH_SOURCE[0]}")/.."

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

rc=0
# pipefail (above) makes the pipeline report cargo's failure, not tee's success.
cargo test --locked -p "$package" -- --exact "$name" 2>&1 | tee "$log" || rc=$?

ran="$(sed -nE 's/^running ([0-9]+) tests?$/\1/p' "$log" | awk '{s += $1} END {print s + 0}')"
passed="$(sed -nE 's/^test result: [A-Za-z]+\. ([0-9]+) passed;.*/\1/p' "$log" | awk '{s += $1} END {print s + 0}')"
failed_targets="$(grep -cE '^test result: FAILED' "$log" || true)"

echo
echo "cargo-test-exact: package=$package test=$name cargo-exit=$rc ran=$ran passed=$passed failed-targets=$failed_targets"
if [ "$rc" -ne 0 ] || [ "$ran" -ne 1 ] || [ "$passed" -ne 1 ] || [ "$failed_targets" -ne 0 ]; then
  echo "cargo-test-exact: FAIL -- wanted exactly 1 test run and passed" >&2
  exit 1
fi
echo "cargo-test-exact: PASS -- exactly 1 test run and passed"
