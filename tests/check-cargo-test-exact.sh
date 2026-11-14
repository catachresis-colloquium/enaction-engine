#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Both-ways test for scripts/cargo-test-exact.sh.
#
# `cargo test <name>` is a substring filter that exits 0 when it matches
# nothing, so a misspelled test name in a done_when reads as green. The script
# exists to close that hole; this test proves it does, in both directions:
#
#   real cargo:
#     1. a real, fully qualified test name PASSES;
#     2. a misspelled name FAILS (0 tests run);
#     3. a bare prefix of a real name FAILS (`--exact` is in force, so a
#        filter cannot quietly match a different or larger set);
#   stub cargo (a fake `cargo` first on PATH, so the parser is tested on
#   outputs real cargo would only give with a planted broken test):
#     4. one test run that FAILED fails;
#     5. two tests run and passed fails (exactly one, not at least one);
#     6. output claiming 1 passed while cargo exits non-zero fails;
#     7. one test run but ignored (0 passed) fails;
#     8. a missing argument is a usage error (exit 2).

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$here/scripts/cargo-test-exact.sh"
[ -f "$script" ] || { echo "FAIL: $script does not exist" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

expect() {
  # expect <want-exit> <label> -- <command...>
  local want="$1" label="$2" rc=0
  shift 3
  "$@" >"$work/$label.log" 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ]; then
    echo "ok   $label (exit $rc)"
  else
    echo "FAIL $label: wanted exit $want, got $rc" >&2
    sed 's/^/     /' "$work/$label.log" >&2
    exit 1
  fi
}

real=store::tests::no_belief_moves_without_evidence

expect 0 real-name-passes -- bash "$script" enaction-trace "$real"
expect 1 misspelled-name-fails -- bash "$script" enaction-trace "${real}_misspelt"
expect 1 prefix-name-fails -- bash "$script" enaction-trace store::tests::no_belief

# stub_cargo <dir> <exit-code> <<< "<output>": a fake cargo that ignores its
# arguments, prints the given output and exits with the given code.
stub_cargo() {
  local dir="$work/$1"
  mkdir -p "$dir"
  cat >"$dir/out.txt"
  printf '#!/usr/bin/env bash\ncat "%s"\nexit %s\n' "$dir/out.txt" "$2" >"$dir/cargo"
  chmod +x "$dir/cargo"
  echo "$dir"
}

d="$(stub_cargo failed 101 <<'OUT'
running 1 test
test planted ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 40 filtered out; finished in 0.00s
OUT
)"
expect 1 stub-one-failed-fails -- env PATH="$d:$PATH" bash "$script" pkg planted

d="$(stub_cargo two 0 <<'OUT'
running 1 test
test planted ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out; finished in 0.00s
running 1 test
test planted ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s
OUT
)"
expect 1 stub-two-passed-fails -- env PATH="$d:$PATH" bash "$script" pkg planted

d="$(stub_cargo lying 101 <<'OUT'
running 1 test
test planted ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out; finished in 0.00s
OUT
)"
expect 1 stub-nonzero-exit-fails -- env PATH="$d:$PATH" bash "$script" pkg planted

d="$(stub_cargo ignored 0 <<'OUT'
running 1 test
test planted ... ignored
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 40 filtered out; finished in 0.00s
OUT
)"
expect 1 stub-ignored-fails -- env PATH="$d:$PATH" bash "$script" pkg planted

expect 2 missing-argument-is-usage-error -- bash "$script" enaction-trace

echo "PASS: cargo-test-exact.sh passes exactly one real test and fails otherwise"
