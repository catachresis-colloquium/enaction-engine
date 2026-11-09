#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Both-ways test for scripts/check-coq-assumptions.sh:
#   1. an axiom-free theorem PASSES;
#   2. a theorem whose proof uses an Axiom declared in ANOTHER module FAILS
#      (the case a lexical ban on our own file cannot see);
#   3. an empty OBLIGATIONS file FAILS (no vacuous pass);
#   4. a missing OBLIGATIONS file FAILS;
#   5. a name that does not exist FAILS.
# Fixtures live in a throwaway directory; repository proofs are untouched.

set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command -v coqc >/dev/null 2>&1 || { echo "FAIL: coqc not on PATH" >&2; exit 1; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/repo/scripts" "$work/repo/p"
cp "$here/scripts/check-coq-assumptions.sh" "$work/repo/scripts/"
cat >"$work/repo/p/Clean.v" <<'COQ'
Theorem two_plus_two : 2 + 2 = 4.
Proof. reflexivity. Qed.
COQ
cat >"$work/repo/p/Lib.v" <<'COQ'
Axiom excluded_middle : forall P : Prop, P \/ ~ P.
COQ
cat >"$work/repo/p/UsesAxiom.v" <<'COQ'
Require Enaction.Lib.
Theorem dne_instance : forall P : Prop, P \/ ~ P.
Proof. exact Enaction.Lib.excluded_middle. Qed.
COQ

run() {
  # run <pass|fail> <label> <obligations-content or __MISSING__>
  local want="$1" label="$2" rc=0
  rm -f "$work/repo/p/OBLIGATIONS"
  [ "$3" = __MISSING__ ] || printf '%s' "$3" >"$work/repo/p/OBLIGATIONS"
  bash "$work/repo/scripts/check-coq-assumptions.sh" p >"$work/$label.log" 2>&1 || rc=$?
  if { [ "$want" = pass ] && [ "$rc" -eq 0 ]; } || { [ "$want" = fail ] && [ "$rc" -eq 1 ]; }; then
    echo "ok   $label (exit $rc)"
  else
    echo "FAIL $label: wanted $want, got exit $rc" >&2
    sed 's/^/     /' "$work/$label.log" >&2
    exit 1
  fi
}

run pass clean-theorem-passes            $'Clean.two_plus_two\n'
run fail imported-axiom-fails            $'Lib.excluded_middle\nUsesAxiom.dne_instance\n'
run fail empty-obligations-fails         $'# only a comment\n\n'
run fail missing-obligations-fails       __MISSING__
run fail unknown-name-fails              $'Clean.no_such_theorem\n'

# The imported-axiom case must fail on the axiom verdict, not on compilation.
grep -q 'excluded_middle' "$work/imported-axiom-fails.log" \
  || { echo "FAIL: imported-axiom case did not report the axiom" >&2; exit 1; }
echo "PASS: check-coq-assumptions.sh enforces closed obligations both ways"
