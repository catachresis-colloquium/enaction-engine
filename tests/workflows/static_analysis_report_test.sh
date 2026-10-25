#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Contract tests for the pinned Panic Attack, Hypatia, Bridge, and fleet reports.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPORT_TOOL="$PROJECT_ROOT/scripts/static-analysis-report.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

pass_count=0
fail_count=0

pass() {
  printf '  PASS: %s\n' "$1"
  pass_count=$((pass_count + 1))
}

fail() {
  printf '  FAIL: %s\n' "$1" >&2
  fail_count=$((fail_count + 1))
}

expect_ok() {
  local name="$1" output
  shift
  if output=$("$@" 2>&1); then
    pass "$name"
  else
    fail "$name"
    [[ -z "$output" ]] || printf '    %s\n' "$output" >&2
  fi
}

expect_fail() {
  local name="$1" output
  shift
  if output=$("$@" 2>&1); then
    fail "$name (unexpectedly succeeded)"
  else
    pass "$name"
  fi
}

REAL_IJSON_JCS=false
if command -v ijson-jcs >/dev/null 2>&1; then
  REAL_IJSON_JCS=true
  printf '%s\n' 'Using installed ijson-jcs for RFC 7493 / RFC 8785 checks'
else
  # Lightweight local contract-test stand-in. The scanner-tools CI job runs
  # this same suite again with the real pinned binary, including strict I-JSON
  # cases below; this mock is not a substitute for that run.
  mkdir -p "$TMP/bin"
  cat > "$TMP/bin/ijson-jcs" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  check)
    [[ "$#" -eq 2 && -s "$2" ]] || exit 2
    expected="$(jq -cS . "$2")"
    actual="$(cat "$2")"
    [[ "$actual" == "$expected" ]]
    ;;
  canon)
    [[ "$#" -eq 1 ]] || exit 2
    jq -cS .
    ;;
  *) exit 2 ;;
esac
EOF
  chmod +x "$TMP/bin/ijson-jcs"
  export PATH="$TMP/bin:$PATH"
fi

jq -cnS '{schema_version:"2.5",weak_points:[{severity:"Critical",description:"critical fixture",file:"src/lib.rs",line:7,suppressed:false},{severity:"High",description:"suppressed fixture",suppressed:true}],statistics:{},file_statistics:[]}' > "$TMP/panic.json"
printf '{ "weak_points": [], "schema_version": "2.5", "statistics": {}, "file_statistics": [] }\n' > "$TMP/panic-raw.json"
jq -cnS '[{severity:"high",file:"src/lib.rs",line:12,message:"high fixture"},{severity:"warn",file:"scripts/check.sh",line:4,message:"warning fixture"}]' > "$TMP/hypatia.json"
jq -cnS '{schema_version:"0.2.0",project:".",total_dependencies:2,vulnerable_dependencies:1,cves:[{classification:"unmitigable"}],mitigated:0,unmitigable:1,concatenative:0,informational:0}' > "$TMP/bridge.json"

printf '%s\n' 'Static-analysis report contract tests'
expect_ok 'canonicalizes ordinary scanner JSON before strict JCS validation' \
  "$REPORT_TOOL" canonicalize "$TMP/panic-raw.json" "$TMP/panic-canonicalized.json"
expect_ok 'canonicalized Panic Attack report passes strict validation' \
  "$REPORT_TOOL" validate-panic "$TMP/panic-canonicalized.json"
expect_ok 'Panic Attack accepts canonical AssailReport object' "$REPORT_TOOL" validate-panic "$TMP/panic.json"
expect_ok 'Hypatia accepts a canonical findings array' "$REPORT_TOOL" validate-hypatia "$TMP/hypatia.json"
expect_ok 'Patch Bridge accepts a consistent triage object' "$REPORT_TOOL" validate-bridge "$TMP/bridge.json"

jq -cnS '[{severity:"Critical"}]' > "$TMP/panic-array.json"
expect_fail 'Panic Attack rejects a bare array instead of its report object' "$REPORT_TOOL" validate-panic "$TMP/panic-array.json"
jq -cnS '{schema_version:"2.4",weak_points:[],statistics:{},file_statistics:[]}' > "$TMP/panic-old-schema.json"
expect_fail 'Panic Attack rejects an unpinned report schema' "$REPORT_TOOL" validate-panic "$TMP/panic-old-schema.json"
jq -cnS '{schema_version:"2.5",weak_points:[{severity:"CRITICAL",description:"wrong enum case"}],statistics:{},file_statistics:[]}' > "$TMP/panic-bad-severity.json"
expect_fail 'Panic Attack rejects severity variants not in its wire enum' "$REPORT_TOOL" validate-panic "$TMP/panic-bad-severity.json"
jq -cnS '{schema_version:"2.5",weak_points:[],statistics:{},file_statistics:[]}' > "$TMP/panic-noncanonical.json"
printf ' ' >> "$TMP/panic-noncanonical.json"
expect_fail 'Panic Attack rejects valid but non-canonical JSON' "$REPORT_TOOL" validate-panic "$TMP/panic-noncanonical.json"

if [[ "$REAL_IJSON_JCS" == true ]]; then
  printf '{"duplicate":1,"duplicate":2}' > "$TMP/ijson-duplicate-keys.json"
  expect_fail 'real ijson-jcs rejects duplicate object members under I-JSON' \
    ijson-jcs check "$TMP/ijson-duplicate-keys.json"
  expect_fail 'canonicalizer rejects duplicate object members under I-JSON' \
    "$REPORT_TOOL" canonicalize "$TMP/ijson-duplicate-keys.json" "$TMP/ijson-duplicate-canonical.json"
  printf '{"integer":9007199254740992}' > "$TMP/ijson-unsafe-integer.json"
  expect_fail 'real ijson-jcs rejects integers outside the interoperable range' \
    ijson-jcs check "$TMP/ijson-unsafe-integer.json"
  printf '{"b":1,"a":2}' > "$TMP/ijson-unsorted-keys.json"
  expect_fail 'real ijson-jcs rejects valid JSON with non-JCS property ordering' \
    ijson-jcs check "$TMP/ijson-unsorted-keys.json"
fi

jq -cnS '{severity:"high"}' > "$TMP/hypatia-object.json"
expect_fail 'Hypatia rejects an object instead of its findings array' "$REPORT_TOOL" validate-hypatia "$TMP/hypatia-object.json"
jq -cnS '[{severity:"catastrophic"}]' > "$TMP/hypatia-bad-severity.json"
expect_fail 'Hypatia rejects unknown severity labels rather than counting zero' "$REPORT_TOOL" validate-hypatia "$TMP/hypatia-bad-severity.json"

jq -cnS '{schema_version:"0.2.0",project:".",total_dependencies:2,vulnerable_dependencies:1,cves:[{classification:"unmitigable"}],mitigated:0,unmitigable:0,concatenative:0,informational:0}' > "$TMP/bridge-inconsistent.json"
expect_fail 'Patch Bridge rejects inconsistent summary counts' "$REPORT_TOOL" validate-bridge "$TMP/bridge-inconsistent.json"
expect_fail 'Missing reports fail closed' "$REPORT_TOOL" validate-panic "$TMP/missing.json"

expect_ok 'Unified report assembles only after every input validates' \
  "$REPORT_TOOL" assemble "$TMP/panic.json" "$TMP/hypatia.json" "$TMP/bridge.json" "$TMP/unified.json" \
  "example/enaction-engine" "0123456789abcdef0123456789abcdef01234567" "refs/pull/1/merge" "1" "2026-10-25T00:00:00Z"
if jq -e '
  .schema_version == "1.2.0"
  and (.findings | length == 3)
  and ([.findings[] | select(.scanner == "panic-attack" and .severity == "critical")] | length == 1)
  and ([.findings[] | select(.scanner == "panic-attack" and .severity == "high")] | length == 0)
  and ([.findings[] | select(.scanner == "hypatia" and .severity == "medium")] | length == 1)
  and (.patch_bridge.unmitigable == 1)
' "$TMP/unified.json" >/dev/null; then
  pass 'Unified report excludes suppressed findings and normalizes severities'
else
  fail 'Unified report excludes suppressed findings and normalizes severities'
fi

printf '\nResults: %d passed, %d failed\n' "$pass_count" "$fail_count"
[[ "$fail_count" -eq 0 ]]
