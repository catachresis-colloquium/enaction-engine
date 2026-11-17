#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# fetch-a2ml-base.sh -- fetch the commit that bounds the A2ML ledger and print
# its full SHA (the value for A2ML_BASE). Fails closed: if the bound cannot be
# fetched, the ledger cannot be compared, and skipping the comparison would let
# a new ledger entry through exactly when it cannot be checked.
#
#   pull_request  the tip of the PR base branch (BASE_REF)
#   push          the previous commit of the branch (BEFORE_SHA, GitHub's
#                 github.event.before); a new branch (all-zero SHA) has no
#                 previous commit and fails
#   anything else fails
#
# Environment: EVENT_NAME, BASE_REF, BEFORE_SHA, TOKEN (optional; sent as a
# header so it never lands in .git/config), ORIGIN (default: origin).
# Exit: 0 prints the SHA on stdout; 1 cannot bound the ledger.

set -euo pipefail

EVENT_NAME="${EVENT_NAME:-}"
BASE_REF="${BASE_REF:-}"
BEFORE_SHA="${BEFORE_SHA:-}"
TOKEN="${TOKEN:-}"
ORIGIN="${ORIGIN:-origin}"

die() { echo "::error::$*" >&2; exit 1; }

case "$EVENT_NAME" in
  pull_request|pull_request_target)
    [ -n "$BASE_REF" ] || die "pull request event with no base branch: cannot bound the A2ML ledger"
    WANT="$BASE_REF" ;;
  push)
    case "$BEFORE_SHA" in
      ''|*[!0-9a-f]*) die "push event with no usable previous commit ('$BEFORE_SHA'): cannot bound the A2ML ledger" ;;
    esac
    [ -n "${BEFORE_SHA//0/}" ] || die "push event with an all-zero previous commit (a new branch): cannot bound the A2ML ledger"
    WANT="$BEFORE_SHA" ;;
  *) die "event '$EVENT_NAME' has no A2ML ledger bound: cannot compare, failing closed" ;;
esac

ARGS=()
if [ -n "$TOKEN" ]; then
  ARGS+=(-c "http.extraheader=AUTHORIZATION: basic $(printf 'x-access-token:%s' "$TOKEN" | base64 -w0)")
fi
git "${ARGS[@]}" fetch -q --depth=1 "$ORIGIN" "$WANT" >&2 \
  || die "could not fetch $WANT from $ORIGIN to bound the A2ML ledger"
git rev-parse --verify -q 'FETCH_HEAD^{commit}' || die "fetched $WANT but it is not a commit"
