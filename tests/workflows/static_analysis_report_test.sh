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

# Exercise schema fields independently so one rejected field cannot mask a
# missing check on another. These ASCII/integer fixtures are also valid JCS
# when the real pinned canonicalizer is available.
for mutation in \
  'del(.weak_points)' '.weak_points = {}' \
  'del(.statistics)' '.statistics = []' \
  'del(.file_statistics)' '.file_statistics = {}' \
  '.weak_points = [null]' \
  'del(.weak_points[0].severity)' '.weak_points[0].severity = 1' \
  'del(.weak_points[0].description)' '.weak_points[0].description = false' \
  '.weak_points[0].suppressed = "false"' \
  '.weak_points[0].file = 7' '.weak_points[0].line = "7"'; do
  jq -cS "$mutation" "$TMP/panic.json" > "$TMP/invalid.json"
  expect_fail "Panic Attack rejects $mutation" "$REPORT_TOOL" validate-panic "$TMP/invalid.json"
done

for mutation in \
  '.[0] = null' '.[0] = "finding"' \
  'del(.[0].severity)' '.[0].severity = false' '.[0].severity = "High"' \
  '.[0].file = []' '.[0].line = "12"'; do
  jq -cS "$mutation" "$TMP/hypatia.json" > "$TMP/invalid.json"
  expect_fail "Hypatia rejects $mutation" "$REPORT_TOOL" validate-hypatia "$TMP/invalid.json"
done

for mutation in \
  '.schema_version = "0.1.0"' '.cves = {}' '.cves = [null]' \
  '.cves[0].classification = "unknown"' 'del(.cves[0].classification)' \
  '.vulnerable_dependencies = 0' '.mitigated = 1' '.informational = 1'; do
  jq -cS "$mutation" "$TMP/bridge.json" > "$TMP/invalid.json"
  expect_fail "Patch Bridge rejects $mutation" "$REPORT_TOOL" validate-bridge "$TMP/invalid.json"
done
for field in total_dependencies vulnerable_dependencies mitigated unmitigable concatenative informational; do
  for value in null -1 '"0"'; do
    jq -cS --arg field "$field" ".[\$field] = $value" "$TMP/bridge.json" > "$TMP/invalid.json"
    expect_fail "Patch Bridge rejects $field=$value" "$REPORT_TOOL" validate-bridge "$TMP/invalid.json"
  done
done

jq -cnS '{schema_version:"2.5",weak_points:[],statistics:{},file_statistics:[]}' > "$TMP/empty-panic.json"
printf '[]\n' > "$TMP/empty-hypatia.json"
jq -cnS '{schema_version:"0.2.0",total_dependencies:0,vulnerable_dependencies:0,cves:[],mitigated:0,unmitigable:0,concatenative:0,informational:0}' > "$TMP/empty-bridge.json"
for scanner in panic hypatia bridge; do
  expect_ok "$scanner accepts an explicitly empty valid report" \
    "$REPORT_TOOL" "validate-$scanner" "$TMP/empty-$scanner.json"
  : > "$TMP/empty.json"
  printf '{broken' > "$TMP/malformed.json"
  printf 'null\n' > "$TMP/null.json"
  for invalid in empty malformed null missing; do
    expect_fail "$scanner rejects a $invalid report" \
      "$REPORT_TOOL" "validate-$scanner" "$TMP/$invalid.json"
  done
done

# Normalization must retain payloads, replace untrusted scanner labels, and
# exclude only boolean true suppression (absent/null/false stay visible).
jq -cnS '{schema_version:"2.5",statistics:{},file_statistics:[],weak_points:[
  {severity:"Low",description:"absent",scanner:"spoofed",extra:{rule:1}},
  {severity:"Medium",description:"null",suppressed:null,file:null,line:null},
  {severity:"High",description:"false",suppressed:false},
  {severity:"Critical",description:"true",suppressed:true}]}' > "$TMP/panic-normalization.json"
expect_ok 'normalizes Panic Attack findings' bash -c '"$1" normalize-panic "$2" > "$3"' \
  _ "$REPORT_TOOL" "$TMP/panic-normalization.json" "$TMP/normalized.json"
expect_ok 'Panic Attack preserves payloads and only removes suppressed findings' jq -e '. == [
  {severity:"low",description:"absent",scanner:"panic-attack",extra:{rule:1}},
  {severity:"medium",description:"null",suppressed:null,file:null,line:null,scanner:"panic-attack"},
  {severity:"high",description:"false",suppressed:false,scanner:"panic-attack"}]' "$TMP/normalized.json"

jq -cnS '["critical","high","medium","low","info","warn","warning"] | map({severity:.,message:.,scanner:"spoofed",extra:[1],file:null,line:null})' > "$TMP/hypatia-severities.json"
expect_ok 'normalizes every supported Hypatia severity' bash -c '"$1" normalize-hypatia "$2" > "$3"' \
  _ "$REPORT_TOOL" "$TMP/hypatia-severities.json" "$TMP/normalized.json"
expect_ok 'Hypatia aliases warn and warning while preserving all payload fields' jq -e '. == (
  ["critical","high","medium","low","info","warn","warning"] | map({
    severity:(if . == "warn" or . == "warning" then "medium" else . end),
    message:.,scanner:"hypatia",extra:[1],file:null,line:null}))' "$TMP/normalized.json"
for scanner in panic hypatia; do
  expect_fail "$scanner normalization validates its input" \
    "$REPORT_TOOL" "normalize-$scanner" "$TMP/malformed.json"
done

jq -cnS '{schema_version:"0.2.0",total_dependencies:3,vulnerable_dependencies:3,cves:[{classification:"mitigable"},{classification:"unmitigable"},{classification:"informational"}],mitigated:1,unmitigable:1,concatenative:0,informational:1}' > "$TMP/all-bridge-classes.json"
expect_ok 'Patch Bridge accepts all three classifications with matching counts' \
  "$REPORT_TOOL" validate-bridge "$TMP/all-bridge-classes.json"

expect_ok 'assembly retains exact metadata and complete Bridge and finding payloads' jq -e \
  --slurpfile panic "$TMP/panic.json" --slurpfile hypatia "$TMP/hypatia.json" --slurpfile bridge "$TMP/bridge.json" '
  .repository == "example/enaction-engine"
  and .commit_sha == "0123456789abcdef0123456789abcdef01234567"
  and .ref == "refs/pull/1/merge" and .run_id == "1"
  and .timestamp == "2026-10-25T00:00:00Z"
  and .patch_bridge == $bridge[0]
  and .findings == [
    ($panic[0].weak_points[0] + {severity:"critical",scanner:"panic-attack"}),
    ($hypatia[0][0] + {scanner:"hypatia"}),
    ($hypatia[0][1] + {severity:"medium",scanner:"hypatia"})]' "$TMP/unified.json"
expect_ok 'assembly creates nested output directories and accepts zero findings' \
  "$REPORT_TOOL" assemble "$TMP/empty-panic.json" "$TMP/empty-hypatia.json" "$TMP/empty-bridge.json" \
  "$TMP/nested reports/empty/unified.json" 'repo/"quoted"' sha ref '007' timestamp
expect_ok 'empty assembly retains string metadata without numeric coercion' jq -e \
  '.findings == [] and .repository == "repo/\"quoted\"" and .run_id == "007" and .patch_bridge.total_dependencies == 0' \
  "$TMP/nested reports/empty/unified.json"
expect_ok 'assembled report is canonical' ijson-jcs check "$TMP/unified.json"

for invalid_scanner in panic hypatia bridge; do
  inputs=("$TMP/panic.json" "$TMP/hypatia.json" "$TMP/bridge.json")
  case "$invalid_scanner" in
    panic) inputs[0]="$TMP/malformed.json" ;;
    hypatia) inputs[1]="$TMP/malformed.json" ;;
    bridge) inputs[2]="$TMP/malformed.json" ;;
  esac
  cp "$TMP/unified.json" "$TMP/preserved.json"
  expect_fail "assembly refuses an invalid $invalid_scanner input" "$REPORT_TOOL" assemble \
    "${inputs[@]}" "$TMP/preserved.json" repo sha ref run timestamp
  expect_ok "invalid $invalid_scanner input is rejected before overwriting output" \
    cmp -s "$TMP/unified.json" "$TMP/preserved.json"
done

# Canonicalization promises atomic replacement even if conversion or the
# subsequent check fails. Inject both failures without requiring a real scanner.
mkdir -p "$TMP/failing-jcs"
cat > "$TMP/failing-jcs/ijson-jcs" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  canon) printf '{"partial":true}'; exit "${CANON_STATUS:-0}" ;;
  check) exit 1 ;;
esac
EOF
chmod +x "$TMP/failing-jcs/ijson-jcs"
mkdir -p "$TMP/atomic output"
printf 'original report\n' > "$TMP/original"
for canon_status in 0 23; do
  cp "$TMP/original" "$TMP/atomic output/report.json"
  expect_fail "canonicalization refuses failed conversion/check (canon=$canon_status)" \
    env PATH="$TMP/failing-jcs:$PATH" CANON_STATUS="$canon_status" "$REPORT_TOOL" canonicalize \
    "$TMP/panic-raw.json" "$TMP/atomic output/report.json"
  expect_ok 'failed canonicalization preserves the existing destination' \
    cmp -s "$TMP/original" "$TMP/atomic output/report.json"
  leftovers=("$TMP/atomic output"/.ijson-jcs.*)
  if [[ ! -e "${leftovers[0]}" ]]; then
    pass 'failed canonicalization removes temporary files'
  else
    fail 'failed canonicalization removes temporary files'
  fi
done
expect_ok 'canonicalization supports replacing the input file in place' \
  "$REPORT_TOOL" canonicalize "$TMP/panic-raw.json" "$TMP/panic-raw.json"
expect_ok 'in-place canonicalization produces a valid report' \
  "$REPORT_TOOL" validate-panic "$TMP/panic-raw.json"

# Usage errors should be distinct from scanner/report failures.
for command_name in validate-panic validate-hypatia validate-bridge normalize-panic normalize-hypatia canonicalize assemble unknown; do
  status=0
  "$REPORT_TOOL" "$command_name" > "$TMP/usage.log" 2>&1 || status=$?
  if [[ "$status" -eq 2 ]] && grep -q '^Usage:' "$TMP/usage.log"; then
    pass "$command_name rejects missing arguments with usage status 2"
  else
    fail "$command_name rejects missing arguments with usage status 2"
  fi
done

expect_ok 'pinned scanner builder handles source, build, and command-surface failures' \
  bash "$PROJECT_ROOT/tests/workflows/build_static_analysis_tools_test.sh"
expect_ok 'workflow annotations and Rust coverage obey their failure contracts' \
  bash "$PROJECT_ROOT/tests/workflows/static_analysis_gate_test.sh"

printf '\nResults: %d passed, %d failed\n' "$pass_count" "$fail_count"
[[ "$fail_count" -eq 0 ]]
