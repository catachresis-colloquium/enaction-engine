#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Unit-test pinned source/build orchestration with local Git, Rust, and binary
# stubs. No network access, toolchain installation, or scanner build is needed.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILDER="$ROOT/scripts/build-static-analysis-tools.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
PASS=0
FAIL=0
pass() { printf 'PASS: %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL: %s\n' "$1" >&2; FAIL=$((FAIL + 1)); }
export CALL_LOG="$TMP/calls"
mkdir -p "$TMP/bin"
cat > "$TMP/bin/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'git'; printf '|%s' "$@"; printf '\n'
if [[ "$1" == init ]]; then
  mkdir -p "$3"
  : > "$3/Cargo.toml"
  exit 0
fi
[[ "$1" == -C ]] || exit 90
destination="$2"
shift 2
case "$1" in
  fetch)
    [[ "${destination##*/}" != "${FAIL_FETCH:-}" ]] || exit 24
    printf '%s\n' "${@: -1}" > "$destination/.pin"
    ;;
  rev-parse)
    if [[ "${destination##*/}" == "${MISMATCH_REPO:-}" ]]; then
      printf '%040d\n' 0 > "$destination/.pin"
    fi
    cat "$destination/.pin" >&3
    ;;
esac
EOF
# Git's revision output belongs on stdout; its invocation trace goes to the
# shared log. Keep a wrapper so the stub itself can use ordinary printf.
mv "$TMP/bin/git" "$TMP/git-stub"
cat > "$TMP/bin/git" <<'EOF'
#!/usr/bin/env bash
exec bash "$STUB_ROOT/git-stub" "$@" 3>&1 >> "$CALL_LOG"
EOF
cat > "$TMP/bin/rustup" <<'EOF'
#!/usr/bin/env bash
printf 'rustup'; printf '|%s' "$@"; printf '\n'
exit "${RUSTUP_STATUS:-0}"
EOF
mv "$TMP/bin/rustup" "$TMP/rustup-stub"
cat > "$TMP/bin/rustup" <<'EOF'
#!/usr/bin/env bash
exec bash "$STUB_ROOT/rustup-stub" "$@" >> "$CALL_LOG"
EOF
cat > "$TMP/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
{ printf 'cargo'; printf '|%s' "$@"; printf '\n'; } >> "$CALL_LOG"
manifest=''
binary=''
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --manifest-path) manifest="$2"; shift ;;
    --bin) binary="$2"; shift ;;
  esac
  shift
done
[[ "$binary" != "${FAIL_BUILD:-}" ]] || exit 35
build_dir="${manifest%/Cargo.toml}/target/release"
mkdir -p "$build_dir"
cat > "$build_dir/$binary" <<'BINARY'
#!/usr/bin/env bash
set -euo pipefail
probe="${0##*/}"
for arg in "$@"; do probe+="|$arg"; done
printf '%s\n' "$probe" >> "$CALL_LOG"
[[ "$probe" != "${FAIL_PROBE:-}" ]] || exit 46
BINARY
EOF
chmod +x "$TMP/bin"/*
export STUB_ROOT="$TMP" PATH="$TMP/bin:$PATH"

new_fixture() {
  unset FAIL_FETCH MISMATCH_REPO FAIL_BUILD FAIL_PROBE RUSTUP_STATUS
  export TMPDIR="$TMP/$1/build roots"
  OUTPUT_DIR="$TMP/$1/installed tools"
  mkdir -p "$TMPDIR"
  : > "$CALL_LOG"
}

run_builder() {
  STATUS=0
  bash "$BUILDER" "$@" > "$TMP/output" 2>&1 || STATUS=$?
}

assert_status() {
  if [[ "$STATUS" == "$2" ]] && { [[ -z "${3:-}" ]] || grep -Fq -- "$3" "$TMP/output"; }; then
    pass "$1"
  else
    fail "$1 (expected $2, got $STATUS)"
    cat "$TMP/output" >&2
  fi
}

assert_cleaned() {
  if compgen -G "$TMPDIR/*" > /dev/null; then fail 'temporary source trees were left behind';
  else pass 'temporary source trees are removed'; fi
}

new_fixture success
run_builder "$OUTPUT_DIR"
assert_status 'builds into an output path containing spaces' 0
build_root="$(sed -n 's/^git|init|-q|//p' "$CALL_LOG" | head -1)"
build_root="${build_root%/panic-attack}"
cat > "$TMP/expected" <<EOF
rustup|toolchain|install|1.97.1|--profile|minimal|--no-self-update
EOF
repos=(panic-attack pons-asinorum ijson-jcs)
pins=(b5530834a839734d1d181bfe6edb0d43d4233cf4 51b8920f541782ceb9465ea3f0d33c96104fc1fb 2c880594c8b1f823a7ab2a8a830c894975a1792e)
for index in "${!repos[@]}"; do
  source_dir="$build_root/${repos[$index]}"
  cat >> "$TMP/expected" <<EOF
git|init|-q|$source_dir
git|-C|$source_dir|remote|add|origin|https://github.com/hyperpolymath/${repos[$index]}.git
git|-C|$source_dir|fetch|-q|--depth=1|origin|${pins[$index]}
git|-C|$source_dir|checkout|-q|--detach|FETCH_HEAD
git|-C|$source_dir|rev-parse|HEAD
EOF
done
cat >> "$TMP/expected" <<EOF
cargo|+1.97.1|build|--locked|--release|--manifest-path|$build_root/panic-attack/Cargo.toml|--features|http|--bin|panic-attack
cargo|+1.97.1|build|--locked|--release|--manifest-path|$build_root/pons-asinorum/Cargo.toml|--package|pons-cli|--bin|pons
cargo|+1.97.1|build|--locked|--release|--manifest-path|$build_root/ijson-jcs/Cargo.toml|--bin|ijson-jcs
panic-attack|--version
pons|--version
ijson-jcs|--version
panic-attack|--quiet|assail|--help
panic-attack|--quiet|bridge|triage|--help
pons|scan|--help
pons|validate-report|--help
ijson-jcs|--help
EOF
if cmp -s "$TMP/expected" "$CALL_LOG"; then
  pass 'verifies all immutable pins before locked builds and probes every required command'
else
  fail 'source/build/probe invocation contract differs'
  diff -u "$TMP/expected" "$CALL_LOG" >&2 || true
fi
for binary in panic-attack pons ijson-jcs; do
  if [[ "$(stat -c '%a' "$OUTPUT_DIR/$binary")" == 755 ]]; then
    pass "installs $binary with executable mode 0755"
  else fail "incorrect executable mode for $binary"; fi
done
assert_cleaned

for repo in "${repos[@]}"; do
  new_fixture "pin-$repo"
  export MISMATCH_REPO="$repo"
  run_builder "$OUTPUT_DIR"
  assert_status "rejects a mismatched $repo source revision" 1 'source pin mismatch'
  if grep -q '^cargo' "$CALL_LOG"; then fail 'build started with an unverified pin'; else pass 'pin mismatch prevents all builds'; fi
  assert_cleaned
done
new_fixture fetch-failure
export FAIL_FETCH=pons-asinorum
run_builder "$OUTPUT_DIR"
assert_status 'propagates source fetch failures' 24
assert_cleaned

new_fixture toolchain-failure
export RUSTUP_STATUS=19
run_builder "$OUTPUT_DIR"
assert_status 'propagates toolchain installation failure' 19
if grep -qE '^(git|cargo)' "$CALL_LOG"; then fail 'continues after toolchain failure'; else pass 'toolchain failure prevents fetches and builds'; fi

for binary in panic-attack pons ijson-jcs; do
  new_fixture "build-$binary"
  export FAIL_BUILD="$binary"
  run_builder "$OUTPUT_DIR"
  assert_status "propagates $binary build failure" 35
  if [[ ! -e "$OUTPUT_DIR/panic-attack" && ! -e "$OUTPUT_DIR/pons" && ! -e "$OUTPUT_DIR/ijson-jcs" ]]; then
    pass 'does not publish binaries after a failed build'
  else fail 'publishes binaries after a failed build'; fi
  assert_cleaned
done
for probe in 'panic-attack|--version' 'pons|--version' 'ijson-jcs|--version' \
  'panic-attack|--quiet|assail|--help' 'panic-attack|--quiet|bridge|triage|--help' \
  'pons|scan|--help' 'pons|validate-report|--help' 'ijson-jcs|--help'; do
  new_fixture "probe-$PASS"
  export FAIL_PROBE="$probe"
  run_builder "$OUTPUT_DIR"
  assert_status "fails when command surface drifts: $probe" 46
  if [[ "$(tail -1 "$CALL_LOG")" == "$probe" ]]; then pass 'stops at the failed probe'; else fail 'continues after probe failure'; fi
  assert_cleaned
done

new_fixture usage
run_builder
assert_status 'rejects a missing output argument' 2 'Usage:'
run_builder "$OUTPUT_DIR" extra
assert_status 'rejects extra output arguments' 2 'Usage:'
if [[ ! -s "$CALL_LOG" ]]; then pass 'usage errors never install or build tools'; else fail 'usage errors invoke tools'; fi

printf '\nScanner builder unit tests: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
