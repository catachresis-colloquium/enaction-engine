#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# check-observer-mutant.sh
#
# Proves the observer tests guard the observer boundary (ADR-0022, Proposed):
# they must FAIL against a planted omniscience mutant.
#
#   1. Copy the working tree (tracked and untracked-but-not-ignored files;
#      no target/, no .git) to a temporary directory. The real crate is never
#      modified.
#   2. Control run: the UNPATCHED copy's observer tests must pass. Otherwise a
#      failure in step 4 could be the copy being broken, not the mutant being
#      caught.
#   3. Apply tests/mutants/omniscience.patch to the copy. A patch that no
#      longer applies is a FAIL (regenerate it), never a pass.
#   4. Run the copy's enaction-trace tests. Exit 0 only when cargo fails, the
#      output reports `test result: FAILED`, BOTH observer tests are reported
#      FAILED, and nothing failed to compile (a mutant that does not build has
#      not been caught by any test).
#
# The mutant is a patch, not a Cargo feature: features unify through
# `--all-features` and dependents, so no build of the real crate can enable it.
#
# Exit: 0 = the observer tests catch the mutant; 1 = otherwise.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PATCH="$ROOT/tests/mutants/omniscience.patch"
TESTS=(observer_sally_anne_two_run_probe observer_proptest_perturbs_outside_closure)

[ -f "$PATCH" ] || { echo "FAIL: $PATCH not found" >&2; exit 1; }
command -v patch >/dev/null 2>&1 || { echo "FAIL: patch(1) not on PATH" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
copy="$work/enaction-engine"
mkdir -p "$copy"

# A fresh target dir for every run, inside the throwaway copy. Do NOT share
# one across runs to save build time: tar keeps the sources' mtimes, Cargo
# hashes path packages relative to the workspace root, and so a later run's
# control build silently reused the previous run's MUTANT artefacts (observed
# while writing this script). Within one run, patch(1) gives store.rs a fresh
# mtime, so the mutant build cannot reuse the control build.
export CARGO_TARGET_DIR="$work/target"

echo "=== copy working tree to $copy ==="
# One tar reading the NUL-separated list (xargs could split it into several
# archives, and the extracting tar would stop at the first).
(cd "$ROOT" && git ls-files -z --cached --others --exclude-standard \
  | tar --null --no-recursion -T - -cf -) \
  | tar -xf - -C "$copy"

echo
echo "=== control: unpatched copy, observer tests must pass ==="
control_rc=0
(cd "$copy" && cargo test --locked -p enaction-trace --test observer) \
  >"$work/control.log" 2>&1 || control_rc=$?
cat "$work/control.log"
if [ "$control_rc" -ne 0 ] || ! grep -qE '^test result: ok\. 2 passed; 0 failed' "$work/control.log"; then
  echo "FAIL: the unpatched copy does not pass its observer tests (exit $control_rc);" >&2
  echo "      a mutant failure there would prove nothing." >&2
  exit 1
fi

echo
echo "=== apply $(basename "$PATCH") to the copy ==="
if ! (cd "$copy" && patch -p1 --forward --batch <"$PATCH"); then
  echo "FAIL: the mutant no longer applies; regenerate tests/mutants/omniscience.patch" >&2
  echo "      against the current crates/enaction-trace/src/store.rs." >&2
  exit 1
fi

echo
echo "=== mutant: patched copy, observer tests must FAIL ==="
mutant_rc=0
(cd "$copy" && cargo test --locked -p enaction-trace --no-fail-fast) \
  >"$work/mutant.log" 2>&1 || mutant_rc=$?
cat "$work/mutant.log"

echo
echo "=== verdict ==="
ok=1
if grep -qE '^error(\[E[0-9]+\])?:' "$work/mutant.log" && ! grep -q '^test result: FAILED' "$work/mutant.log"; then
  echo "FAIL: the mutant did not compile; no test caught it" >&2
  ok=0
fi
[ "$mutant_rc" -ne 0 ] || { echo "FAIL: cargo test passed against the mutant" >&2; ok=0; }
grep -q '^test result: FAILED' "$work/mutant.log" \
  || { echo "FAIL: no 'test result: FAILED' in the mutant run" >&2; ok=0; }
for t in "${TESTS[@]}"; do
  if grep -qE "^test $t \.\.\. FAILED$" "$work/mutant.log"; then
    echo "caught: $t FAILED against the mutant"
  else
    echo "FAIL: $t did not fail against the mutant" >&2
    ok=0
  fi
done

if [ "$ok" -ne 1 ]; then
  echo "RESULT: FAIL -- the observer tests do not catch the omniscience mutant" >&2
  exit 1
fi
echo "RESULT: PASS -- the patched copy reports test result: FAILED; both observer tests catch the mutant"
