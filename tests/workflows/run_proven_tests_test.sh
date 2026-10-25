#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Unit tests of runner orchestration. Stub external tools in a disposable tree;
# these tests do not claim to compile Idris2 or execute the real ABI suite.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
PASS=0
FAIL=0
pass() { printf 'PASS: %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL: %s\n' "$1" >&2; FAIL=$((FAIL + 1)); }

export MOCK_BIN="$TMP/bin" CALL_LOG="$TMP/calls"
mkdir -p "$MOCK_BIN"
cat > "$MOCK_BIN/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" == 4 && "$1" == -C && "$2" == "$EXPECTED_FRAMEWORK" && "$3" == rev-parse && "$4" == HEAD ]] || exit 91
exit_status="${GIT_STATUS:-0}"
[[ "$exit_status" == 0 ]] || exit "$exit_status"
printf '%s\n' "$MOCK_COMMIT"
EOF
cat > "$MOCK_BIN/idris2" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == --version ]]; then
  printf '%s\n' "${MOCK_VERSION:-Idris 2, version 0.7.0}"
  exit "${VERSION_STATUS:-0}"
fi
printf '%s|%s|%s\n' "$PWD" "$1" "$2" >> "$CALL_LOG"
[[ "$1:$2" != "${FAIL_STEP:-}" ]] || exit 37
if [[ "$1:$2" == --build:enaction-engine-tests.ipkg && "${OMIT_RUNNER:-0}" == 0 ]]; then
  mkdir -p build/exec
  cat > build/exec/enaction-engine-tests <<'RUNNER'
#!/usr/bin/env bash
printf 'runner\n' >> "$CALL_LOG"
exit "${RUNNER_STATUS:-0}"
RUNNER
  chmod "${RUNNER_MODE:-755}" build/exec/enaction-engine-tests
fi
EOF
chmod +x "$MOCK_BIN"/*

# Functions take priority even if the developer has Idris2 in ~/.idris2/bin,
# which the production script prepends. No user toolchain or HOME is changed.
git() { "$MOCK_BIN/git" "$@"; }
idris2() { "$MOCK_BIN/idris2" "$@"; }
export -f git idris2

new_fixture() {
  export REPO_FIXTURE="$TMP/$1/repository with spaces"
  export EXPECTED_FRAMEWORK="$TMP/$1/framework with spaces"
  export MOCK_COMMIT=eca682bcd419554f6779e72969902e6faef4fd9d
  unset MOCK_VERSION GIT_STATUS VERSION_STATUS FAIL_STEP OMIT_RUNNER RUNNER_STATUS RUNNER_MODE PROVEN_TESTS_ROOT
  mkdir -p "$REPO_FIXTURE/scripts" "$REPO_FIXTURE/verification/tests" "$EXPECTED_FRAMEWORK/.git"
  cp "$ROOT/scripts/run-proven-tests.sh" "$REPO_FIXTURE/scripts/"
  : > "$CALL_LOG"
}

run_runner() {
  STATUS=0
  # Invoke from outside the fixture to detect accidental dependence on cwd.
  (cd "$TMP" && bash "$REPO_FIXTURE/scripts/run-proven-tests.sh" "$@") > "$TMP/output" 2>&1 || STATUS=$?
}

assert_status() {
  if [[ "$STATUS" == "$2" ]] && { [[ -z "${3:-}" ]] || grep -Fq -- "$3" "$TMP/output"; }; then
    pass "$1"
  else
    fail "$1 (expected $2, got $STATUS)"
    cat "$TMP/output" >&2
  fi
}

new_fixture success
run_runner "$EXPECTED_FRAMEWORK"
assert_status 'builds and executes the suite with paths containing spaces' 0
cat > "$TMP/expected" <<EOF
$EXPECTED_FRAMEWORK|--build|proven-tests.ipkg
$EXPECTED_FRAMEWORK|--install|proven-tests.ipkg
$REPO_FIXTURE|--build|abi.ipkg
$REPO_FIXTURE|--install|abi.ipkg
$REPO_FIXTURE/verification/tests|--build|enaction-engine-tests.ipkg
runner
EOF
if cmp -s "$TMP/expected" "$CALL_LOG"; then
  pass 'builds and installs packages in dependency order and the correct directories'
else
  fail 'builds and installs packages in dependency order and the correct directories'
fi

new_fixture environment
export PROVEN_TESTS_ROOT="$EXPECTED_FRAMEWORK"
run_runner
assert_status 'accepts the framework location from PROVEN_TESTS_ROOT' 0
export PROVEN_TESTS_ROOT="$TMP/nonexistent"
run_runner "$EXPECTED_FRAMEWORK"
assert_status 'explicit framework argument takes precedence over the environment' 0

for invalid in unset nonexistent not-git; do
  new_fixture "usage-$invalid"
  case "$invalid" in
    unset) run_runner ;;
    nonexistent) run_runner "$TMP/absent" ;;
    not-git) mkdir -p "$TMP/not-git"; run_runner "$TMP/not-git" ;;
  esac
  assert_status "rejects $invalid framework configuration before compilation" 2 'Usage:'
  if [[ ! -s "$CALL_LOG" ]]; then pass 'usage failure does not build packages'; else fail 'usage failure builds packages'; fi
done

for failure in wrong-commit git-error wrong-version version-error; do
  new_fixture "$failure"
  expected_status=1
  diagnostic=''
  case "$failure" in
    wrong-commit) export MOCK_COMMIT=0000000000000000000000000000000000000000; diagnostic='expected framework' ;;
    git-error) export GIT_STATUS=29; expected_status=29 ;;
    wrong-version) export MOCK_VERSION='Idris 2, version 0.8.0'; diagnostic='expected Idris2 0.7.0' ;;
    version-error) export VERSION_STATUS=31; expected_status=31 ;;
  esac
  run_runner "$EXPECTED_FRAMEWORK"
  assert_status "fails closed on $failure" "$expected_status" "$diagnostic"
  if [[ ! -s "$CALL_LOG" ]]; then pass 'preflight failure does not build packages'; else fail 'preflight failure builds packages'; fi
done

steps=(--build:proven-tests.ipkg --install:proven-tests.ipkg --build:abi.ipkg --install:abi.ipkg --build:enaction-engine-tests.ipkg)
for index in "${!steps[@]}"; do
  new_fixture "build-failure-$index"
  export FAIL_STEP="${steps[$index]}"
  run_runner "$EXPECTED_FRAMEWORK"
  assert_status "propagates failure of $FAIL_STEP" 37
  if [[ "$(wc -l < "$CALL_LOG")" -eq $((index + 1)) ]]; then
    pass 'stops immediately without downstream builds or execution'
  else
    fail 'continues after a failed build/install'
  fi
done

for invalid in missing not-executable; do
  new_fixture "runner-$invalid"
  if [[ "$invalid" == missing ]]; then export OMIT_RUNNER=1; else export RUNNER_MODE=644; fi
  run_runner "$EXPECTED_FRAMEWORK"
  assert_status "rejects a $invalid compiled runner" 1 'did not produce executable'
done
new_fixture failing-suite
export RUNNER_STATUS=43
run_runner "$EXPECTED_FRAMEWORK"
assert_status 'propagates a failing suite exit status to CI' 43

printf '\nProven-Tests runner unit tests: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
