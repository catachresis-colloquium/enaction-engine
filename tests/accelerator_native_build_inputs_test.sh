#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Unit-test Cargo directives emitted by the native build script. Compile only
# build.rs, then substitute Zig so no native library or benchmark is built.

set -euo pipefail
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT
PASS=0
FAIL=0
pass() { printf 'PASS: %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL: %s\n' "$1" >&2; FAIL=$((FAIL + 1)); }

rustc --edition 2024 "$PROJECT_DIR/crates/enaction-accelerator-native/build.rs" \
    -o "$TMP_ROOT/build-script"
mkdir -p "$TMP_ROOT/repository with spaces/crates/enaction-accelerator-native" "$TMP_ROOT/output with spaces"
cat >"$TMP_ROOT/zig override" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" >"$ZIG_ARGS"
printf '%s\n' "$PWD" >"$ZIG_CWD"
STUB
chmod +x "$TMP_ROOT/zig override"

CARGO_MANIFEST_DIR="$TMP_ROOT/repository with spaces/crates/enaction-accelerator-native" \
    OUT_DIR="$TMP_ROOT/output with spaces" ZIG="$TMP_ROOT/zig override" \
    ZIG_ARGS="$TMP_ROOT/zig-args" ZIG_CWD="$TMP_ROOT/zig-cwd" \
    "$TMP_ROOT/build-script" >"$TMP_ROOT/directives"

# Imported kernels must be individual Cargo inputs. Tracking only the root
# accelerator.zig left warm builds silently using stale imported code.
for input in src/interface/ffi/src/accelerator.zig \
    src/interface/ffi/src/axiom_matmul.zig src/interface/ffi/src/axiom_pointwise.zig \
    src/interface/generated/abi/enaction_accelerator.zig \
    src/interface/generated/abi/enaction_accelerator.rs; do
    expected="cargo:rerun-if-changed=$TMP_ROOT/repository with spaces/$input"
    count="$(grep -Fxc "$expected" "$TMP_ROOT/directives" || true)"
    if [[ "$count" == 1 ]]; then pass "tracks $input exactly once";
    else fail "expected one rebuild directive for $input, got $count"; fi
done

if grep -Fxq 'cargo:rerun-if-env-changed=ZIG' "$TMP_ROOT/directives"; then
    pass 'compiler override changes invalidate Cargo output'
else fail 'missing ZIG environment rebuild directive'; fi
if [[ "$(cat "$TMP_ROOT/zig-cwd")" == "$TMP_ROOT/repository with spaces" ]] &&
    grep -Fxq -- "-Mroot=$TMP_ROOT/repository with spaces/src/interface/ffi/src/accelerator.zig" "$TMP_ROOT/zig-args" &&
    grep -Fxq -- "-Mabi=$TMP_ROOT/repository with spaces/src/interface/generated/abi/enaction_accelerator.zig" "$TMP_ROOT/zig-args"; then
    pass 'tracked source paths agree with actual compiler inputs and working directory'
else fail 'compiler received incorrect source paths or working directory'; fi
if grep -Fxq "cargo:rustc-link-search=native=$TMP_ROOT/output with spaces" "$TMP_ROOT/directives" &&
    grep -Fxq 'cargo:rustc-link-lib=static=enaction_accelerator' "$TMP_ROOT/directives"; then
    pass 'rebuild directives preserve static library linking'
else fail 'missing static library link directives'; fi

printf '\nNative build input tests: %d passed, %d failed\n' "$PASS" "$FAIL"
if ((FAIL > 0)); then exit 1; fi
