#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# arrival_pack_drift_test.sh: proves that the arrival-pack gate can fail.
#
# 1. Control. A copy of CLAUDE.md with the arrival pack must verify: exit 0 and
#    an "OK:" line. Without this, a verifier that always fails would pass the
#    drift half below and prove nothing.
# 2. Planted drift. One character inside the ARRIVAL-PACK region of the copy is
#    changed. The verifier must exit non-zero AND print a "DRIFT:" line. The
#    DRIFT line is required so that a non-zero exit for another reason (for
#    example nickel missing from PATH) cannot pass as a caught drift.
#
# Exits 0 only when both hold. It needs nickel on PATH, as the gate does.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
# An inherited GIT_DIR or GIT_WORK_TREE would point the fixture's git calls at
# the caller's checkout. Clear them after ROOT is found and before any fixture
# git call.
unset GIT_DIR GIT_WORK_TREE
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# verify.sh resolves its root with `git rev-parse` and reads the a2ml from
# <root>/.machine_readable/descriptiles, so the copy needs those files and its
# own git repository.
mkdir -p "$TMP/.machine_readable"
cp "$ROOT/CLAUDE.md" "$TMP/CLAUDE.md"
cp -r "$ROOT/.machine_readable/arrival-pack" "$TMP/.machine_readable/arrival-pack"
cp -r "$ROOT/.machine_readable/descriptiles" "$TMP/.machine_readable/descriptiles"
git -C "$TMP" init -q
cd "$TMP"

# 1. Control: the unmodified copy must verify.
set +e
control_out="$(bash .machine_readable/arrival-pack/verify.sh 2>&1)"
control_rc=$?
set -e
echo "== control (unmodified copy): exit $control_rc"
printf '%s\n' "$control_out"
if [ "$control_rc" -ne 0 ] || ! grep -q '^OK:' <<<"$control_out"; then
  echo "FAIL: the unmodified copy does not verify, so the planted check would prove nothing." >&2
  exit 1
fi

# 2. Planted drift: replace the first ASCII letter inside the region with '#'.
line="$(awk '/<!-- ARRIVAL-PACK:BEGIN/{f=1; next} /ARRIVAL-PACK:END/{f=0} f && /[A-Za-z]/{print NR; exit}' CLAUDE.md)"
if [ -z "$line" ]; then
  echo "FAIL: no letter found inside the ARRIVAL-PACK region." >&2
  exit 1
fi
cp CLAUDE.md CLAUDE.md.orig
sed -i "${line}s/[A-Za-z]/#/" CLAUDE.md
if cmp -s CLAUDE.md CLAUDE.md.orig; then
  echo "FAIL: the planted edit changed no bytes." >&2
  exit 1
fi
echo "== planted drift: line $line of the region"
diff CLAUDE.md.orig CLAUDE.md || true

set +e
planted_out="$(bash .machine_readable/arrival-pack/verify.sh 2>&1)"
planted_rc=$?
set -e
echo "== verifier on planted copy: exit $planted_rc"
printf '%s\n' "$planted_out"
if [ "$planted_rc" -eq 0 ]; then
  echo "FAIL: the verifier passed a planted drift." >&2
  exit 1
fi
if ! grep -q '^DRIFT:' <<<"$planted_out"; then
  echo "FAIL: the verifier exited $planted_rc without reporting DRIFT, so the failure is not the drift check." >&2
  exit 1
fi
echo "PASS: the control verified, and the planted drift was caught (exit $planted_rc, DRIFT reported)."
