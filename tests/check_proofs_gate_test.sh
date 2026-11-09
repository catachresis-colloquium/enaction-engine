#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Both-ways test for scripts/check-proofs.sh (agda leg):
#   1. a K-free gated module PASSES;
#   2. a gated module that needs axiom K FAILS, which proves the canonical
#      `--without-K` flag (standards PROOF-POLICY §2.1) is actually in force;
#   3. a gated module that needs a non-cubical-compatible feature FAILS, which
#      proves `--cubical-compatible` is in force;
#   4. a MANIFEST status other than gated|quarantine FAILS, so a typo cannot
#      turn a broken module into an "expected" quarantine failure.
#
# Each case runs the real script against a throwaway repository layout, so
# the repository's own proofs are never touched. Requires agda on PATH; a
# missing toolchain is a failure, never a skip.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command -v agda >/dev/null 2>&1 || { echo "FAIL: agda not on PATH" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# make_repo <name> <module-file-name> <status> <<< "<module source>"
make_repo() {
  local dir="$work/$1"
  mkdir -p "$dir/scripts" "$dir/verification/proofs/agda"
  cp "$here/scripts/check-proofs.sh" "$dir/scripts/"
  cat >"$dir/verification/proofs/agda/$2"
  printf 'verification/proofs/agda|%s|%s| test fixture\n' "$2" "$3" \
    >"$dir/verification/proofs/agda/MANIFEST"
  echo "$dir"
}

expect() {
  # expect <pass|fail> <label> <repo-dir>
  local want="$1" label="$2" dir="$3" rc=0
  bash "$dir/scripts/check-proofs.sh" agda >"$work/$label.log" 2>&1 || rc=$?
  if { [ "$want" = pass ] && [ "$rc" -eq 0 ]; } || { [ "$want" = fail ] && [ "$rc" -eq 1 ]; }; then
    echo "ok   $label (exit $rc)"
  else
    echo "FAIL $label: wanted $want, got exit $rc" >&2
    sed 's/^/     /' "$work/$label.log" >&2
    exit 1
  fi
}

ok_dir="$(make_repo ok Ok.agda gated <<'AGDA'
module Ok where
data N : Set where
  z : N
  s : N -> N
plus : N -> N -> N
plus z     n = n
plus (s m) n = s (plus m n)
AGDA
)"
expect pass k-free-module-passes "$ok_dir"

k_dir="$(make_repo k UsesK.agda gated <<'AGDA'
module UsesK where
data _==_ {A : Set} (x : A) : A -> Set where
  refl : x == x
-- Uniqueness of identity proofs: provable only with axiom K.
uip : {A : Set} {x : A} (p : x == x) -> p == refl
uip refl = refl
AGDA
)"
expect fail k-module-fails "$k_dir"

cub_dir="$(make_repo cub NotCubical.agda gated <<'AGDA'
{-# OPTIONS --with-K #-}
module NotCubical where
data _==_ {A : Set} (x : A) : A -> Set where
  refl : x == x
AGDA
)"
expect fail with-k-pragma-fails "$cub_dir"

typo_dir="$(make_repo typo Ok.agda gate <"$ok_dir/verification/proofs/agda/Ok.agda")"
expect fail unknown-status-fails "$typo_dir"

echo "PASS: check-proofs.sh agda leg enforces the canonical flags and status set"
