#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Both-ways test for the vacuity rule in scripts/check-proofs.sh: a MANIFEST
# that gates zero modules is a FAIL, never a pass.
#
# Before this rule, the lean4 leg passed while gating nothing: its only entry
# is a quarantined template, and a quarantined module that still fails is
# "expected", so the gate went green having proved nothing.
#
# This test exercises the script's MANIFEST logic, not a prover. Agda, Coq and
# Lean are not installed in every container, so each case puts a STUB prover
# first on PATH: it exits 0 for a file containing the word OK and 1 otherwise.
# That stands in for "compiles" and "does not compile". Whether the real
# provers accept the real proofs is the job of the per-prover gates
# (tests/check_proofs_gate_test.sh, proof-gates.yml), not this test.
#
# Cases, for each of lean4, agda and coq:
#   1. a MANIFEST whose only entry is quarantined (and still fails) FAILS,
#      for the zero-gated reason;
#   2. a MANIFEST with only comments and blank lines FAILS, for that reason;
#   3. a MANIFEST gating one compiling module PASSES (the rule is not a
#      blanket failure).

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# A stub prover: `--version` succeeds; otherwise the last argument is the
# module path (relative to the cwd the script chose) and "compiles" iff the
# file contains OK.
mkdir -p "$work/bin"
cat >"$work/bin/stub-prover" <<'STUB'
#!/usr/bin/env bash
[ "${1:-}" = --version ] && { echo "stub prover"; exit 0; }
for last in "$@"; do :; done
grep -q OK "$last" || { echo "stub: $last does not compile"; exit 1; }
STUB
chmod +x "$work/bin/stub-prover"
for cmd in lean agda coqc; do ln -s stub-prover "$work/bin/$cmd"; done

ext_of() { case "$1" in lean4) echo lean ;; agda) echo agda ;; coq) echo v ;; esac; }

# make_repo <name> <prover> <manifest-body> ; module Ok.<ext> compiles,
# Broken.<ext> does not.
make_repo() {
  local dir="$work/$1" prover="$2" ext
  ext="$(ext_of "$prover")"
  mkdir -p "$dir/scripts" "$dir/verification/proofs/$prover"
  cp "$here/scripts/check-proofs.sh" "$dir/scripts/"
  echo "OK" >"$dir/verification/proofs/$prover/Ok.$ext"
  echo "broken" >"$dir/verification/proofs/$prover/Broken.$ext"
  printf '%s\n' "$3" >"$dir/verification/proofs/$prover/MANIFEST"
  echo "$dir"
}

expect() {
  # expect <pass|fail> <label> <repo-dir> <prover> [required-log-text]
  local want="$1" label="$2" dir="$3" prover="$4" text="${5:-}" rc=0
  PATH="$work/bin:$PATH" bash "$dir/scripts/check-proofs.sh" "$prover" \
    >"$work/$label.log" 2>&1 || rc=$?
  if { [ "$want" = pass ] && [ "$rc" -eq 0 ]; } || { [ "$want" = fail ] && [ "$rc" -eq 1 ]; }; then
    if [ -n "$text" ] && ! grep -qF "$text" "$work/$label.log"; then
      echo "FAIL $label: exit $rc as wanted, but not for the reason '$text'" >&2
      sed 's/^/     /' "$work/$label.log" >&2
      exit 1
    fi
    echo "ok   $label (exit $rc)"
  else
    echo "FAIL $label: wanted $want, got exit $rc" >&2
    sed 's/^/     /' "$work/$label.log" >&2
    exit 1
  fi
}

reason="gates zero modules"
for prover in lean4 agda coq; do
  ext="$(ext_of "$prover")"
  root="verification/proofs/$prover"

  d="$(make_repo "$prover-quarantine-only" "$prover" "$root|Broken.$ext|quarantine| still broken")"
  rm "$d/$root/Ok.$ext"
  expect fail "$prover-quarantine-only-fails" "$d" "$prover" "$reason"

  d="$(make_repo "$prover-comments-only" "$prover" "# nothing gated here

")"
  rm "$d/$root/Ok.$ext" "$d/$root/Broken.$ext"
  expect fail "$prover-comments-only-fails" "$d" "$prover" "$reason"

  d="$(make_repo "$prover-one-gated" "$prover" "$root|Ok.$ext|gated| compiles")"
  rm "$d/$root/Broken.$ext"
  expect pass "$prover-one-gated-passes" "$d" "$prover"
done

echo "PASS: check-proofs.sh fails a MANIFEST that gates zero modules and passes one that gates a compiling module"
