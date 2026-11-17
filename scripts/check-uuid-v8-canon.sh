#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
#
# check-uuid-v8-canon.sh -- run the estate's own UUID literal checker,
# hyperpolymath/standards scripts/check-uuid-v8.sh, at a pinned commit.
#
# Solutions at source: the rule lives in standards, so this repo runs that
# script rather than a copy of its logic. It is fetched at a pinned commit
# into the user cache (never into the checkout) and its sha256 is checked on
# every run, so a moved or edited upstream copy fails loudly instead of
# changing the rule silently. STANDARDS_CHECK_UUID_V8 names an existing copy.
# To move the pin, change both values together after reading the upstream
# diff.
set -eu

REV=b49a33e991f37a2249d873cbdc02c27a08f05d6d
SHA256=2f95bdd104268aa9be20c8fb4ca566bed719e255730ab94d5b45b65e58f4c945
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/enaction-engine"
SCRIPT="${STANDARDS_CHECK_UUID_V8:-$CACHE/check-uuid-v8-$REV.sh}"

fail() { echo "FAIL: check-uuid-v8-canon: $*" >&2; exit 1; }

if [ ! -f "$SCRIPT" ]; then
  mkdir -p "$(dirname "$SCRIPT")"
  curl --proto '=https' --proto-redir '=https' --fail --silent --show-error --location \
    "https://raw.githubusercontent.com/hyperpolymath/standards/$REV/scripts/check-uuid-v8.sh" \
    --output "$SCRIPT.part" || fail "could not fetch scripts/check-uuid-v8.sh at standards@$REV"
  mv "$SCRIPT.part" "$SCRIPT"
fi
echo "$SHA256  $SCRIPT" | sha256sum --check --strict --status \
  || fail "$SCRIPT is not standards@$REV scripts/check-uuid-v8.sh (sha256 mismatch)"

cd "$ROOT"
exec sh "$SCRIPT" .
