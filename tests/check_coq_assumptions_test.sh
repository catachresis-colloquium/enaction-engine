#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Both-ways test for scripts/check-coq-assumptions.sh, plus a namespace check
# shared with scripts/check-proofs.sh coq:
#   1. an axiom-free theorem whose module imports an UNLISTED clean helper
#      PASSES (the checker builds the local dependency closure);
#   2. a theorem whose proof uses an Axiom from an UNLISTED helper module
#      FAILS, on that theorem's own verdict, and the report names the axiom
#      (the case a lexical ban on our own file cannot see);
#   3. an empty OBLIGATIONS file FAILS (no vacuous pass);
#   4. a missing OBLIGATIONS file FAILS;
#   5. a name that does not exist FAILS;
#   6. check-proofs.sh coq compiles the same `From Enaction Require …`
#      import, so both gates use one logical namespace.
# Fixtures live in a throwaway repository; repository proofs are untouched.

set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command -v coqc >/dev/null 2>&1 || { echo "FAIL: coqc not on PATH" >&2; exit 1; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

repo="$work/repo"
p="$repo/verification/proofs/coq"
mkdir -p "$repo/scripts" "$p"
cp "$here/scripts/check-coq-assumptions.sh" "$here/scripts/check-proofs.sh" "$repo/scripts/"
cat >"$p/Helper.v" <<'COQ'
Lemma two_two : 2 + 2 = 4.
Proof. reflexivity. Qed.
COQ
cat >"$p/Clean.v" <<'COQ'
From Enaction Require Helper.
Theorem four : 2 + 2 = 4.
Proof. exact Enaction.Helper.two_two. Qed.
COQ
cat >"$p/Lib.v" <<'COQ'
Axiom excluded_middle : forall P : Prop, P \/ ~ P.
COQ
cat >"$p/UsesAxiom.v" <<'COQ'
From Enaction Require Lib.
Theorem dne_instance : forall P : Prop, P \/ ~ P.
Proof. exact Enaction.Lib.excluded_middle. Qed.
COQ

run() {
  # run <pass|fail> <label> <obligations-content or __MISSING__>
  local want="$1" label="$2" rc=0
  rm -f "$p/OBLIGATIONS"
  [ "$3" = __MISSING__ ] || printf '%s' "$3" >"$p/OBLIGATIONS"
  bash "$repo/scripts/check-coq-assumptions.sh" verification/proofs/coq >"$work/$label.log" 2>&1 || rc=$?
  if { [ "$want" = pass ] && [ "$rc" -eq 0 ]; } || { [ "$want" = fail ] && [ "$rc" -eq 1 ]; }; then
    echo "ok   $label (exit $rc)"
  else
    echo "FAIL $label: wanted $want, got exit $rc" >&2
    sed 's/^/     /' "$work/$label.log" >&2
    exit 1
  fi
}

run pass clean-theorem-with-unlisted-helper-passes $'Clean.four\n'
run fail imported-axiom-fails                       $'UsesAxiom.dne_instance\n'
run fail empty-obligations-fails                    $'# only a comment\n\n'
run fail missing-obligations-fails                  __MISSING__
run fail unknown-name-fails                         $'Clean.no_such_theorem\n'

# Case 2 must fail on the theorem's own verdict, naming the axiom, not on a
# compile error or on some other listed name.
grep -q '^FAIL UsesAxiom.dne_instance: not closed' "$work/imported-axiom-fails.log" \
  || { echo "FAIL: imported-axiom case did not report a non-closed verdict for UsesAxiom.dne_instance" >&2
       sed 's/^/     /' "$work/imported-axiom-fails.log" >&2; exit 1; }
grep -q 'excluded_middle' "$work/imported-axiom-fails.log" \
  || { echo "FAIL: imported-axiom case did not name the axiom" >&2; exit 1; }

# Case 6: check-proofs.sh coq resolves the same imports (dependencies first).
printf '%s\n' \
  'verification/proofs/coq|Helper.v|gated| helper' \
  'verification/proofs/coq|Clean.v|gated| imports Helper' \
  'verification/proofs/coq|Lib.v|gated| axiom fixture' \
  'verification/proofs/coq|UsesAxiom.v|gated| imports Lib' >"$p/MANIFEST"
if bash "$repo/scripts/check-proofs.sh" coq >"$work/check-proofs.log" 2>&1; then
  echo "ok   check-proofs-shares-namespace (exit 0)"
else
  echo "FAIL check-proofs-shares-namespace: check-proofs.sh coq could not resolve From Enaction Require" >&2
  sed 's/^/     /' "$work/check-proofs.log" >&2
  exit 1
fi

echo "PASS: check-coq-assumptions.sh enforces closed obligations both ways"
