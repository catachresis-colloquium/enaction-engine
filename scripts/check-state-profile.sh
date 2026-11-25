#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# check-state-profile.sh -- enforce the STATE.deed profile. The plain deed
# lint (validate-deed.sh) accepts a STATE.deed whose profile is wrong; the
# profile is checked only by the linter's --state mode, with --root so that
# :of-chora is compared with the repo's chora deed, and --strict.
#
# Needs STANDARDS_DIR (a standards checkout) and bun, and refuses to run
# without them: a missing linter must not look like a pass.
# A repository with no tracked STATE.deed prints a notice (not a silent skip).
#
# Usage: STANDARDS_DIR=... check-state-profile.sh [REPO_ROOT]
# Exit: 0 passed or no STATE.deed; 1 profile violation; 2 setup error

set -euo pipefail

ROOT="${1:-.}"
STATE=.machine_readable/descriptiles/STATE.deed
: "${STANDARDS_DIR:?set STANDARDS_DIR to a standards checkout}"
LINTER="$STANDARDS_DIR/1-formats/deed/tools/deed_lint.js"
[ -f "$LINTER" ] || { echo "::error::no deed linter at $LINTER" >&2; exit 2; }
command -v bun >/dev/null 2>&1 || { echo "::error::bun is required for the STATE.deed profile check" >&2; exit 2; }

cd "$ROOT"
if [ -z "$(git ls-files -- "$STATE")" ]; then
  echo "::notice::STATE.deed profile: $STATE is not tracked here, so there is no profile to check"
  exit 0
fi
bun "$LINTER" --state "$STATE" --root . --strict
