#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Unit tests for the M1 runner's validation, without building or measuring code.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUNNER="$PROJECT_DIR/scripts/bench-accelerator-m1.sh"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT
RESULT_DIR="$TMP_ROOT/results with spaces"
TOOL_LOG="$TMP_ROOT/tool-calls"
export TOOL_LOG
PASS=0
FAIL=0

pass() { printf 'PASS: %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL: %s: %s\n' "$1" "$2" >&2; FAIL=$((FAIL + 1)); }

# Satisfy command discovery but fail immediately if validation accidentally
# reaches any build, metadata, or digest work. Never invoke the real toolchain.
mkdir -p "$TMP_ROOT/bin"
for tool in cargo rustc jq sha256sum git; do
    cat >"$TMP_ROOT/bin/$tool" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "${0##*/} $*" >>"$TOOL_LOG"
exit 97
STUB
    chmod +x "$TMP_ROOT/bin/$tool"
done

run_runner() {
    : >"$TOOL_LOG"
    if OUTPUT="$(PATH="$TMP_ROOT/bin:$PATH" bash "$RUNNER" "$@" 2>&1)"; then
        STATUS=0
    else
        STATUS=$?
    fi
}

expect_rejection() {
    local label="$1" expected_status="$2" diagnostic="${3:-}"
    if [[ "$STATUS" -ne "$expected_status" ]]; then
        fail "$label" "expected exit $expected_status, got $STATUS: $OUTPUT"
    elif [[ -n "$diagnostic" && "$OUTPUT" != *"$diagnostic"* ]]; then
        fail "$label" "missing diagnostic '$diagnostic': $OUTPUT"
    elif [[ -s "$TOOL_LOG" ]]; then
        fail "$label" "unexpected tool execution: $(cat "$TOOL_LOG")"
    else
        pass "$label"
    fi
}

expect_no_results() {
    if [[ -e "$RESULT_DIR" ]]; then
        fail 'validation does not create results' "$RESULT_DIR exists"
    else
        pass 'validation does not create results'
    fi
}

for samples in 0 -1 10001 1.5 abc 01 ' 1' 1e2; do
    run_runner scalar "$RESULT_DIR" "$samples" 1
    expect_rejection "rejects sample count '$samples'" 2
    expect_no_results
done

for rounds in 0 -1 21 1.5 abc 01 ' 1' 1e2; do
    run_runner scalar "$RESULT_DIR" 1 "$rounds"
    expect_rejection "rejects round count '$rounds'" 2
    expect_no_results
done

run_runner scalar "$RESULT_DIR" 1 1 unexpected
expect_rejection 'rejects extra arguments' 2 'usage:'
expect_no_results

run_runner unsupported "$RESULT_DIR" 1 1
expect_rejection 'rejects unknown modes' 2 'unknown mode: unsupported'
expect_no_results

# A rejected Zig toolchain must not silently run the reference experiment.
ZIG="$TMP_ROOT/missing-zig" run_runner compare "$RESULT_DIR" 1 1
expect_rejection 'comparison requires an available Zig compiler' 1 'comparison requires Zig 0.16.0'
expect_no_results

cat >"$TMP_ROOT/wrong-zig" <<'STUB'
#!/usr/bin/env bash
[[ "$#" -eq 1 && "$1" == version ]] || exit 98
printf '%s\n' '0.15.2'
STUB
chmod +x "$TMP_ROOT/wrong-zig"
ZIG="$TMP_ROOT/wrong-zig" run_runner compare "$RESULT_DIR" 1 1
expect_rejection 'comparison rejects an unpinned Zig version' 1 'comparison requires Zig 0.16.0'
expect_no_results

# Existing files and directories are both protected, including quoted paths.
printf '%s\n' 'preserve previous measurements' >"$TMP_ROOT/expected"
cp "$TMP_ROOT/expected" "$RESULT_DIR"
run_runner scalar "$RESULT_DIR" 1 1
expect_rejection 'refuses to overwrite an existing file' 2 'refusing to overwrite results:'
if cmp -s "$TMP_ROOT/expected" "$RESULT_DIR"; then
    pass 'existing file contents are preserved'
else
    fail 'existing file contents are preserved' 'file was modified'
fi
rm "$RESULT_DIR"
mkdir -p "$RESULT_DIR"
cp "$TMP_ROOT/expected" "$RESULT_DIR/retained.jsonl"
for mode in scalar compare; do
    # Valid endpoint counts must reach overwrite protection. In scalar mode,
    # missing Zig is irrelevant; in compare mode, protection precedes Zig work.
    for counts in '1 1' '10000 20'; do
        read -r samples rounds <<<"$counts"
        ZIG="$TMP_ROOT/missing-zig" run_runner "$mode" "$RESULT_DIR" "$samples" "$rounds"
        expect_rejection "$mode accepts bounds $counts and protects existing results" 2 \
            'refusing to overwrite results:'
    done
done
if cmp -s "$TMP_ROOT/expected" "$RESULT_DIR/retained.jsonl" &&
    [[ "$(find "$RESULT_DIR" -type f | wc -l)" -eq 1 ]]; then
    pass 'existing result directory contents are preserved'
else
    fail 'existing result directory contents are preserved' 'results were modified'
fi

printf '\nM1 runner tests: %d passed, %d failed\n' "$PASS" "$FAIL"
if ((FAIL > 0)); then
    exit 1
fi
