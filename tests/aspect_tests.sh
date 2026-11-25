#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Repository-specific, cross-cutting source checks. This scans code files only;
# explanatory prose and examples in documentation are not executable proofs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

PASS=0
FAIL=0
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

pass() { printf 'PASS: %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL: %s\n' "$1" >&2; FAIL=$((FAIL + 1)); }

for source_root in crates src/interface verification tests examples; do
    if [ ! -d "$source_root" ]; then
        fail "required source root is missing: $source_root"
    fi
done
if ! find crates src/interface verification tests examples -type f \
    \( -name '*.rs' -o -name '*.zig' -o -name '*.idr' -o \
       -name '*.lean' -o -name '*.agda' -o -name '*.v' -o -name '*.hs' \) \
    -print0 >"$TMP_DIR/source-files" 2>"$TMP_DIR/find-errors"; then
    fail "source inventory failed: $(cat "$TMP_DIR/find-errors")"
    exit 1
fi
mapfile -d '' SOURCE_FILES <"$TMP_DIR/source-files"
if [ "${#SOURCE_FILES[@]}" -eq 0 ]; then
    fail "no implementation or proof source files were found"
    exit 1
fi

# Aspect 1: every implementation and proof source carries its SPDX identifier.
MISSING_SPDX=()
for file in "${SOURCE_FILES[@]}"; do
    if ! head -n 5 "$file" | grep -q 'SPDX-License-Identifier:'; then
        MISSING_SPDX+=("$file")
    fi
done
if [ "${#MISSING_SPDX[@]}" -eq 0 ]; then
    pass "all Rust, Zig, Idris2 and proof source files have SPDX headers"
else
    fail "source files missing SPDX headers: ${MISSING_SPDX[*]}"
fi

# Aspect 2: reject actual unsound proof constructs, after removing language
# comments so policy examples in source comments cannot create false positives.
PROOF_FINDINGS=()
for file in "${SOURCE_FILES[@]}"; do
    case "$file" in
        *.idr)
            sed -E '/^[[:space:]]*(--|\|\|\|)/d; s/[[:space:]]--.*$//' "$file" >"$TMP_DIR/source"
            if grep -nE '(^|[^[:alnum:]_])(believe_me|really_believe_me|assert_total)([^[:alnum:]_]|$)' "$TMP_DIR/source" >/dev/null; then
                PROOF_FINDINGS+=("$file: Idris2 partiality escape")
            fi
            ;;
        *.agda)
            sed -E '/^[[:space:]]*--/d; s/[[:space:]]--.*$//' "$file" >"$TMP_DIR/source"
            if grep -nE '^[[:space:]]*postulate([[:space:]]|$)' "$TMP_DIR/source" >/dev/null; then
                PROOF_FINDINGS+=("$file: Agda postulate")
            fi
            ;;
        *.v)
            awk '
              {
                line = $0
                out = ""
                i = 1
                while (i <= length(line)) {
                  pair = substr(line, i, 2)
                  if (in_comment) {
                    if (pair == "*)") { in_comment = 0; i += 2 }
                    else { i++ }
                  } else if (pair == "(*") {
                    in_comment = 1
                    i += 2
                  } else {
                    out = out substr(line, i, 1)
                    i++
                  }
                }
                print out
              }
            ' "$file" >"$TMP_DIR/source"
            if grep -nE '(^|[^[:alnum:]_])Admitted([[:space:].]|$)' "$TMP_DIR/source" >/dev/null; then
                PROOF_FINDINGS+=("$file: Coq Admitted")
            fi
            ;;
        *.lean)
            # Lean permits nested block comments. Strip both block and line comments.
            awk '
              {
                line = $0
                out = ""
                i = 1
                while (i <= length(line)) {
                  pair = substr(line, i, 2)
                  if (depth > 0) {
                    if (pair == "/-") { depth++; i += 2 }
                    else if (pair == "-/") { depth--; i += 2 }
                    else { i++ }
                  } else if (pair == "--") {
                    break
                  } else if (pair == "/-") {
                    depth = 1
                    i += 2
                  } else {
                    out = out substr(line, i, 1)
                    i++
                  }
                }
                print out
              }
            ' "$file" >"$TMP_DIR/source"
            if grep -nE '(^|[^[:alnum:]_])sorry([^[:alnum:]_]|$)' "$TMP_DIR/source" >/dev/null; then
                PROOF_FINDINGS+=("$file: Lean sorry")
            fi
            ;;
        *.hs)
            sed -E '/^[[:space:]]*--/d; s/[[:space:]]--.*$//' "$file" >"$TMP_DIR/source"
            if grep -nE '(^|[^[:alnum:]_])unsafeCoerce([^[:alnum:]_]|$)' "$TMP_DIR/source" >/dev/null; then
                PROOF_FINDINGS+=("$file: Haskell unsafeCoerce")
            fi
            ;;
    esac
done
if [ "${#PROOF_FINDINGS[@]}" -eq 0 ]; then
    pass "no banned proof escape constructs in source files"
else
    fail "banned proof constructs found: ${PROOF_FINDINGS[*]}"
fi

# Aspect 3: Hypatia treats Zig pointer conversions as a safety boundary. There
# is one narrowly reviewed @ptrFromInt fixture in an inline test to construct
# overlapping buffers of different element types. No conversion is allowed in
# production code or anywhere else; the file-scoped Hypatia exemption is guarded
# here so it cannot silently become a production escape hatch.
ZIG_PTRCAST_FINDINGS=()
for file in "${SOURCE_FILES[@]}"; do
    case "$file" in
        *.zig)
            if grep -nF '@ptrCast' "$file" >/dev/null; then
                ZIG_PTRCAST_FINDINGS+=("$file: @ptrCast")
            fi
            if grep -nF '@ptrFromInt' "$file" >/dev/null; then
                if [ "$file" != "src/interface/ffi/src/accelerator.zig" ] \
                    || [ "$(grep -Fc '@ptrFromInt' "$file")" -ne 1 ] \
                    || ! grep -Fq '@ptrFromInt(@intFromPtr(&aliased_storage))' "$file"; then
                    ZIG_PTRCAST_FINDINGS+=("$file: unreviewed @ptrFromInt")
                else
                    marker_line="$(grep -nF 'test "null dimensions and aliasing are refused without mutation"' "$file" | head -1 | cut -d: -f1)"
                    cast_line="$(grep -nF '@ptrFromInt' "$file" | cut -d: -f1)"
                    if [ -z "$marker_line" ] || [ -z "$cast_line" ] || [ "$cast_line" -le "$marker_line" ]; then
                        ZIG_PTRCAST_FINDINGS+=("$file: sanctioned cast is not inside its named aliasing test")
                    fi
                    if ! grep -Fq 'code_safety/zig_ptr_cast:src/interface/ffi/src/accelerator.zig' .hypatia-ignore \
                        || ! head -5 "$file" | grep -Fq 'hypatia: allow code_safety/zig_ptr_cast'; then
                        ZIG_PTRCAST_FINDINGS+=("$file: test-fixture exemption lacks its documented Hypatia scope")
                    fi
                fi
            fi
            ;;
    esac
done
if [ "${#ZIG_PTRCAST_FINDINGS[@]}" -eq 0 ]; then
    pass "no production Zig pointer conversions; the single test-fixture exception is scoped"
else
    fail "unreviewed Zig pointer conversion found: ${ZIG_PTRCAST_FINDINGS[*]}"
fi

# Aspect 4: the old generic FFI scaffold and placeholder test must not return.
if [ ! -e src/interface/ffi/src/main.zig ] && [ ! -e src/interface/ffi/test/integration_test.zig ]; then
    pass "no disconnected generic FFI module or placeholder integration test"
else
    fail "obsolete generic FFI scaffold or placeholder integration test is present"
fi

printf '\nAspect test summary: %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
