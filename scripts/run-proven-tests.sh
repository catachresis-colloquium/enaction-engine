#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Build and run the pinned proven-tests-and-benches suite for the ABI seam.
set -euo pipefail

readonly FRAMEWORK_COMMIT="eca682bcd419554f6779e72969902e6faef4fd9d"
readonly IDRIS2_VERSION="0.7.0"
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRAMEWORK_ROOT="${1:-${PROVEN_TESTS_ROOT:-}}"

if [[ -z "$FRAMEWORK_ROOT" || ! -d "$FRAMEWORK_ROOT/.git" ]]; then
  echo "Usage: PROVEN_TESTS_ROOT=/path/to/proven-tests-and-benches $0" >&2
  echo "The framework must be a git checkout at $FRAMEWORK_COMMIT." >&2
  exit 2
fi

actual_commit="$(git -C "$FRAMEWORK_ROOT" rev-parse HEAD)"
if [[ "$actual_commit" != "$FRAMEWORK_COMMIT" ]]; then
  echo "run-proven-tests: expected framework $FRAMEWORK_COMMIT, got $actual_commit" >&2
  exit 1
fi

export PATH="$HOME/.idris2/bin:$PATH"
command -v idris2 >/dev/null 2>&1 || {
  echo "run-proven-tests: Idris2 $IDRIS2_VERSION is required" >&2
  exit 1
}
version_output="$(idris2 --version 2>&1)"
if [[ "$version_output" != *"$IDRIS2_VERSION"* ]]; then
  echo "run-proven-tests: expected Idris2 $IDRIS2_VERSION, got: $version_output" >&2
  exit 1
fi

for libdir in "$HOME"/.idris2/idris2-*/lib; do
  [[ -d "$libdir" ]] || continue
  export LD_LIBRARY_PATH="$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
done

printf 'Using proven-tests-and-benches %s\n' "$actual_commit"
printf '%s\n' "$version_output"

# Install both packages from their pinned, repository-local source. New tests
# use the direct classifiers and non-debt fixture obligations; they do not call
# the framework's legacy constructors.
(
  cd "$FRAMEWORK_ROOT"
  idris2 --build proven-tests.ipkg
  idris2 --install proven-tests.ipkg
)
(
  cd "$REPO_ROOT"
  idris2 --build abi.ipkg
  idris2 --install abi.ipkg
)
(
  cd "$REPO_ROOT/verification/tests"
  idris2 --build enaction-engine-tests.ipkg
)

runner="$REPO_ROOT/verification/tests/build/exec/enaction-engine-tests"
[[ -x "$runner" ]] || {
  echo "run-proven-tests: Idris2 did not produce executable: $runner" >&2
  exit 1
}
"$runner"
