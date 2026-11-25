#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
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
#   (g)      push event: new file + new ledger entry, bound = previous commit   must fail
#   (g2)     push event with an all-zero previous commit (new branch)           must fail closed
#   (g3)     push event whose previous commit cannot be fetched                 must fail closed
#   (h)      STATE.deed with a wrong :of-chora (profile only, plain lint passes) must fail --state
#
# Each refusal is also matched against the reason the gate prints, so a plant
# that fails for some unrelated cause does not count as a pass.
#
# probes (f), (i), (j): known upstream holes, run and printed, not asserted.
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

SPDX='SPDX-License-Identifier: MPL-2.0'
failures=0
OUT="$WORK/out.txt"

reset_tree() {
  git -C "$T" reset -q --hard "$BASE"
  git -C "$T" clean -q -fdx
}

run_lane() {
  # The hook exits non-zero on a refusal; keep going and report the status.
  set +e
  (cd "$T" && A2ML_BASE="${LANE_BASE:-$BASE}" STANDARDS_DIR="$STANDARDS_DIR" bash .github/hooks/validate-deed.sh) >"$OUT" 2>&1
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

# (d) a .deed with a valid ";;" SPDX header and unbalanced parentheses, so the
# linter gets past the header and refuses it for the parse fault itself.
# (Must be tracked for the hook to lint it.)
reset_tree
printf ';; %s\n(repo-deed :schema-version "1.0.0" (status :present #t)\n' "$SPDX" >"$T/planted_chora.deed"
git -C "$T" add -A
expect_refuse "(d) .deed with unbalanced parentheses" "unbalanced parens"

# (e) an .a2ml file renamed in place; the ledger still names the old path.
reset_tree
git -C "$T" mv -- "$VICTIM" "${VICTIM}.renamed.a2ml"
expect_refuse "(e) .a2ml renamed in place" "A2ML file not in $LEDGER"

# (e2) an .a2ml file moved into 9-archive/.
reset_tree
mkdir -p "$T/9-archive"
git -C "$T" mv -- "$VICTIM" "9-archive/$(basename "$VICTIM")"
expect_refuse "(e2) .a2ml moved into 9-archive/" "A2ML file not in $LEDGER"

# (g) push event. The workflow bounds the ledger on push with the previous
# commit of the branch (scripts/fetch-a2ml-base.sh), fetched from origin; here
# origin is a local bare clone of the base. A new file plus a new ledger entry
# must be refused against that bound (with no bound at all it would pass).
ORIGIN_DIR="$WORK/origin.git"
git clone -q --bare "$T" "$ORIGIN_DIR"
reset_tree
git -C "$T" remote remove origin 2>/dev/null || true
git -C "$T" remote add origin "file://$ORIGIN_DIR"
set +e
PUSH_BASE=$(cd "$T" && EVENT_NAME=push BEFORE_SHA="$BASE" bash "$ROOT/scripts/fetch-a2ml-base.sh" 2>"$WORK/fetch.err")
fetch_rc=$?
set -e
if [ "$fetch_rc" -ne 0 ] || [ "$PUSH_BASE" != "$BASE" ]; then
  echo "FAIL  (g) push base: fetch-a2ml-base.sh exit $fetch_rc, printed '$PUSH_BASE', expected $BASE"
  sed 's/^/        /' "$WORK/fetch.err"
  failures=$((failures + 1))
else
  printf '# planted\n' >"$T/planted-joins-on-push.a2ml"
  printf 'planted-joins-on-push.a2ml\n' >>"$T/$LEDGER"
  git -C "$T" add -A
  LANE_BASE="$PUSH_BASE" expect_refuse "(g) push: new file + ledger entry vs previous commit" "nothing joins it"
fi

# (g2) a push with an all-zero previous commit (a new branch): fail closed.
expect_fetch_refuses() {
  local name=$1 before=$2
  set +e
  (cd "$T" && EVENT_NAME=push BEFORE_SHA="$before" bash "$ROOT/scripts/fetch-a2ml-base.sh") >"$OUT" 2>&1
  local rc=$?
  set -e
  if [ "$rc" -ne 0 ]; then
    echo "PASS  $name: failed closed (exit $rc)"
  else
    echo "FAIL  $name: exit 0, expected the base fetch to fail closed"
    sed 's/^/        /' "$OUT"
    failures=$((failures + 1))
  fi
}
expect_fetch_refuses "(g2) push with an all-zero previous commit" 0000000000000000000000000000000000000000
# (g3) a previous commit that origin does not have.
expect_fetch_refuses "(g3) push whose previous commit cannot be fetched" 1111111111111111111111111111111111111111

# (h) STATE.deed profile. The plain deed lint accepts a STATE.deed whose
# :of-chora is not the repo's chora id; only --state --root --strict refuses it
# (scripts/check-state-profile.sh, a step of the CI job). The fixture is the
# STATE.deed of the clade-facts change, embedded here so the test needs no
# other branch; the repo's own chora id is read from its chora deed.
REPO_ID=$(sed -n 's/.*:repo-uuid[[:space:]]*#\(u8\|uuid\)"\([^"]*\)".*/\2/p' "$T"/*_chora.deed | head -n 1)
[ -n "$REPO_ID" ] || { echo "ERROR: no :repo-uuid in the chora deed under test" >&2; exit 2; }
write_state() {
  mkdir -p "$T/.machine_readable/descriptiles"
  cat >"$T/.machine_readable/descriptiles/STATE.deed" <<STATE
;; $SPDX
(descriptile-deed
  :schema-version "1.0.0"
  :canonical-name "enaction-engine"
  :of-chora #uuid"$1"
  :affirmed "2026-11-17" :affirmed-by agent
  (status
    :phase incubating
    :since "2026-07-22"
    :present #t
    :aliases ()
    :merged-into ""
    :superseded-by ""
    :successors ()
    :ended ""
    (history
      (entry :phase incubating
             :since "2026-07-22"
             :note "fixture"))))
STATE
  git -C "$T" add -A
}
run_state() {
  set +e
  (cd "$T" && STANDARDS_DIR="$STANDARDS_DIR" bash "$ROOT/scripts/check-state-profile.sh" .) >"$OUT" 2>&1
  LANE_RC=$?
  set -e
}
reset_tree
write_state "$REPO_ID"
run_state
if [ "$LANE_RC" -eq 0 ]; then
  echo "PASS  (h) control: a STATE.deed naming the repo's chora id passes --state --strict"
else
  echo "FAIL  (h) control: valid STATE.deed refused (exit $LANE_RC)"
  sed 's/^/        /' "$OUT"
  failures=$((failures + 1))
fi
reset_tree
write_state "00000000-0000-8000-8000-000000000000"
run_lane
if [ "$LANE_RC" -eq 0 ]; then
  echo "INFO  (h) plain deed lint accepts the wrong :of-chora (exit 0): the profile is checked only by the --state step"
else
  echo "INFO  (h) plain deed lint already refuses the wrong :of-chora (exit $LANE_RC)"
fi
run_state
if [ "$LANE_RC" -ne 0 ] && grep -qF -- "YD08" "$OUT"; then
  echo "PASS  (h) STATE.deed with a wrong :of-chora: refused by --state --strict (exit $LANE_RC), reason YD08"
else
  echo "FAIL  (h) STATE.deed with a wrong :of-chora: exit $LANE_RC, expected a YD08 refusal"
  sed 's/^/        /' "$OUT"
  failures=$((failures + 1))
fi

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

# probe (i) H2: delete the ledger and move every .a2ml to
# 9-archive/<flat-name>.a2ml.in. Not asserted: upstream's rules disagree.
reset_tree
git -C "$T" rm -q -- "$LEDGER"
mkdir -p "$T/9-archive"
git -C "$T" ls-files '*.a2ml' | while IFS= read -r f; do
  git -C "$T" mv -- "$f" "9-archive/$(printf '%s' "$f" | tr '/' '_').in"
done
run_lane
hook_rc=$LANE_RC
set +e
bash "$ROOT/scripts/check-no-a2ml.sh" "$T" >"$OUT" 2>&1
noa2ml_rc=$?
set -e
if [ "$hook_rc" -eq 0 ] && [ "$noa2ml_rc" -eq 0 ]; then
  echo "FINDING (i) H2: deleting the ledger and moving every .a2ml to 9-archive/<name>.a2ml.in PASSES both the hook (exit 0) and check-no-a2ml.sh (exit 0)"
else
  echo "INFO  (i) H2: hook exit $hook_rc, check-no-a2ml.sh exit $noa2ml_rc (at least one refuses)"
fi

# probe (j) H8: an unstaged deletion of the last listed .a2ml path.
reset_tree
LASTPATH=$(git -C "$T" ls-files -z --cached --others --exclude-standard -- ':(icase)*.a2ml' | tr '\0' '\n' | tail -n 1)
rm -- "$T/$LASTPATH"
set +e
bash "$ROOT/scripts/check-no-a2ml.sh" "$T" >"$OUT" 2>&1
noa2ml_rc=$?
set -e
if [ "$noa2ml_rc" -ne 0 ] && [ ! -s "$OUT" ]; then
  echo "FINDING (j) H8: check-no-a2ml.sh exits $noa2ml_rc with no output on an unstaged deletion of the last listed path ($LASTPATH)"
else
  echo "INFO  (j) H8: check-no-a2ml.sh exit $noa2ml_rc, output $(wc -c <"$OUT") bytes"
fi

echo
if [ "$failures" -ne 0 ]; then
  echo "deed lane test: $failures case(s) FAILED"
  exit 1
fi
echo "deed lane test: all asserted cases behaved (controls pass; every asserted plant refused)"
