#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# check-coq-assumptions.sh [proof-dir]
#
# For every theorem named in <proof-dir>/OBLIGATIONS, ask the Coq kernel for
# its assumptions and require the exact verdict "Closed under the global
# context". This closes the gap that check-proofs.sh leaves on purpose:
# check-proofs.sh only asks "does each gated module compile?", and a theorem
# that rests on an imported Axiom or Parameter compiles. A lexical ban on
# `Axiom` in our own files cannot see an axiom in an imported library.
#
# OBLIGATIONS format: one fully qualified name per line, relative to the proof
# directory, e.g. `TypeSafety.type_soundness`. Blank lines and `#` comments
# are ignored. A missing or empty OBLIGATIONS file is a FAILURE: a gate with
# nothing to check must not report success (standards PROOF-POLICY §5, §8.4).
#
# The proofs are compiled in a throwaway copy under the logical prefix
# `Enaction`, so no .vo files are written into the repository.
#
# Exit: 0 = every listed obligation is closed; 1 = otherwise; 2 = misuse.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
PROOF_DIR="${1:-verification/proofs/coq}"
OBL="$PROOF_DIR/OBLIGATIONS"

command -v coqc >/dev/null 2>&1 || { echo "FAIL: coqc not on PATH" >&2; exit 1; }
[ -d "$PROOF_DIR" ] || { echo "usage: $(basename "$0") [proof-dir]" >&2; exit 2; }
[ -f "$OBL" ] || { echo "FAIL: $OBL is missing; a gate with no obligations cannot pass" >&2; exit 1; }

mapfile -t names < <(grep -v -E '^[[:space:]]*(#|$)' "$OBL" | sed 's/[[:space:]]*$//')
if [ "${#names[@]}" -eq 0 ]; then
  echo "FAIL: $OBL lists no obligations; a gate with no obligations cannot pass" >&2
  exit 1
fi

coqc --version | head -1
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cp -R "$PROOF_DIR/." "$work/"

# Compile each module that an obligation names, in dependency order as given
# by the names (coqc resolves Require against already-built .vo files).
mapfile -t modules < <(printf '%s\n' "${names[@]}" | sed 's/\.[^.]*$//' | awk '!seen[$0]++')
for m in "${modules[@]}"; do
  src="$work/${m//.//}.v"
  [ -f "$src" ] || { echo "FAIL: obligation module $m has no source ${m//.//}.v" >&2; exit 1; }
  if ! out="$(cd "$work" && coqc -Q . Enaction "${m//.//}.v" 2>&1)"; then
    echo "FAIL: module $m does not compile:" >&2
    printf '%s\n' "$out" | sed 's/^/  /' >&2
    exit 1
  fi
done

fails=0
for n in "${names[@]}"; do
  m="${n%.*}"
  probe="$work/Probe_assumptions.v"
  printf 'Require Enaction.%s.\nPrint Assumptions Enaction.%s.\n' "$m" "$n" >"$probe"
  out="$(cd "$work" && coqc -Q . Enaction Probe_assumptions.v 2>&1)" || {
    echo "FAIL $n: probe did not compile:"; printf '%s\n' "$out" | sed 's/^/  /'
    fails=$((fails + 1)); continue; }
  verdict="$(printf '%s' "$out" | tr -s '[:space:]' ' ' | sed 's/^ //; s/ $//')"
  if [ "$verdict" = "Closed under the global context" ]; then
    echo "ok   $n: Closed under the global context"
  else
    echo "FAIL $n: not closed; the kernel reports:"
    printf '%s\n' "$out" | sed 's/^/  /'
    fails=$((fails + 1))
  fi
done

if [ "$fails" -gt 0 ]; then
  echo "RESULT: FAIL ($fails of ${#names[@]} obligation(s) rest on assumptions)"
  exit 1
fi
echo "RESULT: PASS (${#names[@]} obligation(s), each closed under the global context)"
