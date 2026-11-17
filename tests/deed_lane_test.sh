#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# deed_lane_test.sh -- prove the deed lane refuses what it must refuse.
#
# The lane is .github/hooks/validate-deed.sh run with the canonical linter and
# gate from a hyperpolymath/standards checkout (STANDARDS_DIR). This script
# copies the tree into a temporary git repository, commits it as the base, and
# then plants one defect at a time. The hook runs with A2ML_BASE pointing at
# the base commit, as CI runs it against the pull-request base.
#
#   control  nothing planted                         must pass (exit 0)
#   (a)      a new .a2ml file the ledger lacks       must fail
#   (b)      a listed file deleted, entry kept       must fail
#   (c)      a ledger entry the base did not list    must fail (shrink-only)
#   (d)      a .deed file with a syntax error        must fail
#   (e)      an .a2ml file renamed in place          must fail
#   (e2)     an .a2ml file moved into 9-archive/     must fail
#
# Each refusal is also matched against the reason the gate prints, so a plant
# that fails for some unrelated cause does not count as a pass.
#
# probe (f): a rename that also moves its ledger entry. The canonical gate
# allows this one carry-over (git R100, "a move adds no fact"). It is run and
# its result is printed as a FINDING; it is not asserted, because the
# owner-stated rule (a move fails) and the gate's carry-over disagree and that
# is for the owner to settle, not for this test to paper over.
#
# Needs: bash, git, tar, bun, and STANDARDS_DIR (no fallback: a missing linter
# would let the hook skip its deed check with a notice, which is a silent
# green, so this script refuses to run without one).
#
# Usage: STANDARDS_DIR=/path/to/standards bash tests/deed_lane_test.sh
# Exit:  0 all asserted cases behaved; 1 a case did not; 2 setup error

set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
: "${STANDARDS_DIR:?set STANDARDS_DIR to a hyperpolymath/standards checkout}"
STANDARDS_DIR=$(cd "$STANDARDS_DIR" && pwd)
[ -f "$STANDARDS_DIR/1-formats/deed/tools/deed_lint.js" ] \
  || { echo "ERROR: no deed_lint.js under $STANDARDS_DIR" >&2; exit 2; }
[ -f "$STANDARDS_DIR/scripts/check-a2ml-archived.sh" ] \
  || { echo "ERROR: no check-a2ml-archived.sh under $STANDARDS_DIR" >&2; exit 2; }
command -v bun >/dev/null 2>&1 || { echo "ERROR: bun is required" >&2; exit 2; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
T="$WORK/tree"
mkdir -p "$T"

# Copy the tracked and untracked (not ignored) files of the working tree.
(cd "$ROOT" && git ls-files -z -co --exclude-standard \
  | while IFS= read -r -d '' f; do [ -e "$f" ] && printf '%s\0' "$f"; done \
  | tar --null -T - -cf -) | tar -C "$T" -xf -

git -C "$T" init -q
git -C "$T" config user.email test@example.invalid
git -C "$T" config user.name deed-lane-test
git -C "$T" config commit.gpgsign false
git -C "$T" add -A
git -C "$T" commit -q -m base
BASE=$(git -C "$T" rev-parse HEAD)

LEDGER=.machine_readable/a2ml-legacy-allow.txt
[ -f "$T/$LEDGER" ] || { echo "ERROR: $LEDGER missing from the tree under test" >&2; exit 2; }
VICTIM=$(git -C "$T" ls-files '*.a2ml' | LC_ALL=C sort | head -n 1)
[ -n "$VICTIM" ] || { echo "ERROR: no tracked .a2ml file to plant against" >&2; exit 2; }

SPDX='SPDX-License-Identifier: AGPL-3.0-or-later'
failures=0
OUT="$WORK/out.txt"

reset_tree() {
  git -C "$T" reset -q --hard "$BASE"
  git -C "$T" clean -q -fdx
}

run_lane() {
  # The hook exits non-zero on a refusal; keep going and report the status.
  set +e
  (cd "$T" && A2ML_BASE="$BASE" STANDARDS_DIR="$STANDARDS_DIR" bash .github/hooks/validate-deed.sh) >"$OUT" 2>&1
  LANE_RC=$?
  set -e
}

expect_pass() {
  local name=$1
  run_lane
  if [ "$LANE_RC" -eq 0 ]; then
    echo "PASS  $name: lane exit 0"
  else
    echo "FAIL  $name: lane exit $LANE_RC, expected 0"
    sed 's/^/        /' "$OUT"
    failures=$((failures + 1))
  fi
}

expect_refuse() {
  local name=$1 reason=$2
  run_lane
  if [ "$LANE_RC" -eq 0 ]; then
    echo "FAIL  $name: lane exit 0, expected a refusal"
    sed 's/^/        /' "$OUT"
    failures=$((failures + 1))
  elif ! grep -qF -- "$reason" "$OUT"; then
    echo "FAIL  $name: refused (exit $LANE_RC) but not for the expected reason '$reason'"
    sed 's/^/        /' "$OUT"
    failures=$((failures + 1))
  else
    echo "PASS  $name: refused (exit $LANE_RC), reason '$reason'"
  fi
}

# control: the unplanted tree must pass, so a refusal below is the plant's.
reset_tree
expect_pass "control (no plant)"

# (a) a new .a2ml file the ledger does not list.
reset_tree
printf '# planted\n' >"$T/planted-unlisted.a2ml"
git -C "$T" add -A
expect_refuse "(a) new .a2ml not in the ledger" "A2ML file not in $LEDGER"

# (b) a listed file deleted while its ledger entry remains.
reset_tree
git -C "$T" rm -q -- "$VICTIM"
expect_refuse "(b) listed file deleted, entry kept" "names no A2ML file"

# (c) a ledger entry the base ledger did not list. The file exists so that
# (a) and (b) are not what trips: only the shrink-only comparison can.
reset_tree
printf '# planted\n' >"$T/planted-joins-ledger.a2ml"
printf 'planted-joins-ledger.a2ml\n' >>"$T/$LEDGER"
git -C "$T" add -A
expect_refuse "(c) ledger entry the base did not list" "nothing joins it"

# (d) a .deed file with a syntax error (must be tracked for the hook to lint it).
reset_tree
printf '// %s\n(this is not a deed (((\n' "$SPDX" >"$T/planted_chora.deed"
git -C "$T" add -A
expect_refuse "(d) .deed with a syntax error" "DEED validation failed"

# (e) an .a2ml file renamed in place; the ledger still names the old path.
reset_tree
git -C "$T" mv -- "$VICTIM" "${VICTIM}.renamed.a2ml"
expect_refuse "(e) .a2ml renamed in place" "A2ML file not in $LEDGER"

# (e2) an .a2ml file moved into 9-archive/.
reset_tree
mkdir -p "$T/9-archive"
git -C "$T" mv -- "$VICTIM" "9-archive/$(basename "$VICTIM")"
expect_refuse "(e2) .a2ml moved into 9-archive/" "A2ML file not in $LEDGER"

# probe (f): rename plus the ledger entry renamed with it (identical content).
reset_tree
git -C "$T" mv -- "$VICTIM" "${VICTIM}.renamed.a2ml"
tmp="$WORK/ledger.new"
awk -v old="$VICTIM" -v new="${VICTIM}.renamed.a2ml" '$0 == old { print new; next } { print }' \
  "$T/$LEDGER" >"$tmp"
cp "$tmp" "$T/$LEDGER"
git -C "$T" add -A
run_lane
if [ "$LANE_RC" -eq 0 ]; then
  echo "FINDING (f): rename + ledger-entry rename PASSES the lane (exit 0): the gate's R100 carry-over lets a move through when the entry moves with it"
else
  echo "INFO  (f): rename + ledger-entry rename refused (exit $LANE_RC)"
fi

echo
if [ "$failures" -ne 0 ]; then
  echo "deed lane test: $failures case(s) FAILED"
  exit 1
fi
echo "deed lane test: all asserted cases behaved (control passes; plants a-e2 refused)"
