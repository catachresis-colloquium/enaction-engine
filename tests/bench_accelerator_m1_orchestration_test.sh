#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Exercise the runner with fake compilers/binaries and real JSON/digest tools.
# No build, benchmark, or remote Git operation is performed.

set -euo pipefail
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT
PASS=0
FAIL=0
pass() { printf 'PASS: %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL: %s: %s\n' "$1" "$2" >&2; FAIL=$((FAIL + 1)); }
assert_eq() {
    if [[ "$2" == "$3" ]]; then pass "$1"; else fail "$1" "expected '$2', got '$3'"; fi
}
for tool in jq sha256sum; do command -v "$tool" >/dev/null; done

mkdir -p "$TMP_ROOT/bin"
cat >"$TMP_ROOT/bin/tool-stub" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
tool="${0##*/}"
printf '%s %s\n' "$tool" "$*" >>"$TOOL_LOG"
case "$tool $*" in
    'cargo -V') echo 'cargo fixture' ;;
    'cargo metadata --locked --format-version 1 --no-deps')
        jq -n --arg path "$TARGET_DIR" '{target_directory: $path}' ;;
    'cargo test --locked -p enaction-accelerator --all-targets' | \
    'cargo test --locked -p enaction-accelerator-native --all-targets' | \
    'cargo build --locked --release --target fixture-host -p enaction-accelerator --example m1_matmul' | \
    'cargo build --locked --release --target fixture-host -p enaction-accelerator-native --example m1_matmul_native')
        if [[ "$1" == test ]]; then
            stage="test:$4"
        else
            stage="build:$7"
        fi
        [[ "$FAIL_AT" != "$stage" ]] || exit 29
        if [[ "$1" == build ]]; then
            mkdir -p "$TARGET_DIR/fixture-host/release/examples"
            cp "$BINARY_STUB" "$TARGET_DIR/fixture-host/release/examples/${9}"
        fi ;;
    'rustc -Vv' | 'rustc -vV') printf 'rustc fixture\nhost: fixture-host\n' ;;
    'rustc --print cfg') echo 'target_pointer_width="64"' ;;
    'git rev-parse HEAD') echo '0000000000000000000000000000000000000001' ;;
    'git status --short') echo ' M verification/benchmarks/m1/experiment.rs' ;;
    'zig version') echo '0.16.0' ;;
    'zig targets') printf '    .native = fixture\n' ;;
    nm\ -g\ --defined-only\ *)
        [[ -f "$3" ]] || exit 98
        if [[ "$FAIL_AT" != native-symbols ]]; then echo '00000001 T enaction_accel_abi_version'; fi ;;
    *) printf 'unexpected fixture tool call: %s %s\n' "$tool" "$*" >&2; exit 98 ;;
esac
STUB
chmod +x "$TMP_ROOT/bin/tool-stub"
for tool in cargo rustc git nm zig; do ln -s tool-stub "$TMP_ROOT/bin/$tool"; done

cat >"$TMP_ROOT/binary-stub" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
name="${0##*/}"
printf 'run %s %s\n' "$name" "$*" >>"$TOOL_LOG"
[[ "$#" == 1 && "$1" =~ ^[1-9][0-9]*$ ]] || exit 98
if [[ "$FAIL_AT" == "run:$name" ]]; then
    echo 'planted execution/validation failure' >&2
    exit 29
fi
if [[ "$FAIL_AT" == invalid-json ]]; then echo 'invalid json'; exit 0; fi
printf '{"kind":"metadata","backend":"%s"}\n' "$name"
printf '{"kind":"sample","sample":0}\n'
printf '{"kind":"summary","backend":"%s","samples":%s}\n' "$name" "$1"
printf '{"kind":"memory","backend":"%s","process_peak_rss_kib":123}\n' "$name"
STUB
chmod +x "$TMP_ROOT/binary-stub"
export BINARY_STUB="$TMP_ROOT/binary-stub"

new_fixture() {
    local name="$1"
    FIXTURE="$TMP_ROOT/$name/repository"
    export TARGET_DIR="$TMP_ROOT/$name/custom target with spaces"
    export TOOL_LOG="$TMP_ROOT/$name/tool-calls"
    export FAIL_AT=''
    RESULT_DIR="$TMP_ROOT/$name/results with spaces"
    mkdir -p "$FIXTURE/conformance/accelerator"
    # Copy only runner inputs; real SHA-256 checks remain active in the fixture.
    (
        cd "$PROJECT_DIR"
        cp --parents abi.ipkg Cargo.toml Cargo.lock \
            crates/enaction-accelerator/{Cargo.toml,src/lib.rs,src/version.rs} \
            crates/enaction-accelerator-native/{Cargo.toml,src/lib.rs,build.rs} \
            src/interface/Abi/*.idr src/interface/generated/abi/*.{rs,zig,h,jl} \
            src/interface/generated/abi/SOURCE.sha256 src/interface/ffi/src/*.zig \
            verification/benchmarks/m1/experiment.rs \
            crates/enaction-accelerator/examples/m1_matmul.rs \
            crates/enaction-accelerator-native/examples/m1_matmul_native.rs \
            scripts/bench-accelerator-m1.sh "$FIXTURE"
        cp -R conformance/accelerator/v1 "$FIXTURE/conformance/accelerator/"
    )
    : >"$TOOL_LOG"
}

run_runner() {
    if OUTPUT="$(PATH="$TMP_ROOT/bin:$PATH" ZIG="$ZIG_PATH" \
        bash "$FIXTURE/scripts/bench-accelerator-m1.sh" "$@" 2>&1)"; then
        STATUS=0
    else
        STATUS=$?
    fi
}

assert_not_finalized() {
    if [[ ! -e "$RESULT_DIR/SHA256SUMS" && "$OUTPUT" != *'Results:'* ]]; then
        pass "$1 does not report completed results"
    else
        fail "$1 does not report completed results" "$OUTPUT"
    fi
}

assert_no_measurements() {
    if ! grep -q '^run ' "$TOOL_LOG"; then pass "$1 stops before measurement";
    else fail "$1 stops before measurement" "$(cat "$TOOL_LOG")"; fi
}

new_fixture scalar
ZIG_PATH="$TMP_ROOT/missing-zig"
run_runner scalar "$RESULT_DIR" 2 2
assert_eq 'scalar succeeds without Zig' 0 "$STATUS"
assert_eq 'scalar runs one portable binary per round with the sample count' \
    $'run m1_matmul 2\nrun m1_matmul 2' "$(grep '^run ' "$TOOL_LOG" || true)"
if grep -Eq '^zig |^nm |enaction-accelerator-native' "$TOOL_LOG"; then
    fail 'scalar uses only the portable toolchain' "$(cat "$TOOL_LOG")"
else pass 'scalar uses only the portable toolchain'; fi
if [[ "$STATUS" == 0 ]]; then
    assert_eq 'successful runner reports the absolute output path' "Results: $RESULT_DIR" "$OUTPUT"
    assert_eq 'summaries exclude metadata and samples across rounds' 4 \
        "$(jq -er 'length' "$RESULT_DIR/summaries.json")"
    if jq -e 'all(.[]; (.kind == "summary" or .kind == "memory") and .backend == "m1_matmul")' \
        "$RESULT_DIR/summaries.json" >/dev/null; then
        pass 'summary records retain their kind and backend'
    else fail 'summary records retain their kind and backend' 'unexpected JSON records'; fi
    assert_eq 'binary hashes record each fresh process' 2 "$(wc -l <"$RESULT_DIR/BINARY-SHA256SUMS" | tr -d ' ')"
    if (cd "$RESULT_DIR" && sha256sum -c SHA256SUMS >/dev/null) &&
        sha256sum -c "$RESULT_DIR/BINARY-SHA256SUMS" >/dev/null &&
        (cd "$FIXTURE" && sha256sum -c "$RESULT_DIR/SOURCE-SHA256SUMS" >/dev/null); then
        pass 'result, binary, and source manifests verify with spaced paths'
    else fail 'result, binary, and source manifests verify with spaced paths' 'digest mismatch'; fi
fi

new_fixture compare
# Quoting the explicit compiler path is part of the runner's contract.
mkdir -p "$TMP_ROOT/compiler with spaces"
ln -s "$TMP_ROOT/bin/tool-stub" "$TMP_ROOT/compiler with spaces/zig"
ZIG_PATH="$TMP_ROOT/compiler with spaces/zig"
run_runner compare "$RESULT_DIR" 3 3
assert_eq 'comparison succeeds with pinned Zig' 0 "$STATUS"
assert_eq 'backend order alternates across three fresh-process rounds' \
    $'run m1_matmul 3\nrun m1_matmul_native 3\nrun m1_matmul_native 3\nrun m1_matmul 3\nrun m1_matmul 3\nrun m1_matmul_native 3' \
    "$(grep '^run ' "$TOOL_LOG" || true)"
if [[ "$STATUS" == 0 ]]; then
    assert_eq 'comparison collects summaries from both backends and all rounds' 12 \
        "$(jq -er 'length' "$RESULT_DIR/summaries.json")"
    if [[ -s "$RESULT_DIR/native-symbols.txt" && -f "$RESULT_DIR/native-tests.txt" ]] &&
        grep -q '^cargo test --locked -p enaction-accelerator-native --all-targets$' "$TOOL_LOG"; then
        pass 'comparison requires native tests and linked symbol evidence'
    else fail 'comparison requires native tests and linked symbol evidence' 'missing native evidence'; fi
    assert_eq 'comparison hashes both binaries in every round' 6 \
        "$(wc -l <"$RESULT_DIR/BINARY-SHA256SUMS" | tr -d ' ')"
fi

for stage in test:enaction-accelerator build:enaction-accelerator \
    test:enaction-accelerator-native build:enaction-accelerator-native native-symbols; do
    new_fixture "failure-$stage"
    FAIL_AT="$stage"
    run_runner compare "$RESULT_DIR" 1 1
    expected=29
    if [[ "$stage" == native-symbols ]]; then expected=1; fi
    assert_eq "$stage fails explicitly" "$expected" "$STATUS"
    assert_no_measurements "$stage"
    assert_not_finalized "$stage"
done

for backend in m1_matmul m1_matmul_native; do
    new_fixture "execution-$backend"
    FAIL_AT="run:$backend"
    run_runner compare "$RESULT_DIR" 1 3
    assert_eq "$backend execution failure propagates" 29 "$STATUS"
    expected='run m1_matmul 1'
    if [[ "$backend" == m1_matmul_native ]]; then expected+=$'\nrun m1_matmul_native 1'; fi
    assert_eq "$backend failure is not retried or replaced" "$expected" "$(grep '^run ' "$TOOL_LOG" || true)"
    if grep -q 'planted execution/validation failure' "$RESULT_DIR/$backend-0.stderr.txt"; then
        pass "$backend failure diagnostic is retained"
    else fail "$backend failure diagnostic is retained" 'stderr missing'; fi
    assert_not_finalized "$backend execution failure"
done

for manifest in conformance/accelerator/v1/SHA256SUMS src/interface/generated/abi/SOURCE.sha256; do
    new_fixture "bad-digest-${manifest%%/*}"
    printf '%064d  missing-fixture\n' 0 >>"$FIXTURE/$manifest"
    run_runner scalar "$RESULT_DIR" 1 1
    assert_eq "$manifest failure propagates" 1 "$STATUS"
    assert_no_measurements "$manifest failure"
    if grep -Eq '^cargo (metadata|test|build) ' "$TOOL_LOG"; then
        fail "$manifest failure prevents builds" "$(cat "$TOOL_LOG")"
    else pass "$manifest failure prevents builds"; fi
    assert_not_finalized "$manifest failure"
done

new_fixture invalid-json
FAIL_AT=invalid-json
run_runner scalar "$RESULT_DIR" 1 1
if [[ "$STATUS" -ne 0 ]]; then pass 'malformed benchmark JSON fails aggregation';
else fail 'malformed benchmark JSON fails aggregation' 'runner succeeded'; fi
assert_not_finalized 'malformed JSON'

printf '\nM1 orchestration tests: %d passed, %d failed\n' "$PASS" "$FAIL"
if ((FAIL > 0)); then exit 1; fi
