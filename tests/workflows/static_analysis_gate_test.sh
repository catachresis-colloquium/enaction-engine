#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Execute the workflow's actual shell blocks with synthetic reports/tools.
# The extractor deliberately supports only this workflow's literal run blocks;
# a missing or reformatted block fails instead of silently testing empty code.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="$ROOT/.github/workflows/static-analysis-gate.yml"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
PASS=0
FAIL=0
pass() { printf 'PASS: %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf 'FAIL: %s\n' "$1" >&2; FAIL=$((FAIL + 1)); }

extract_step() {
  awk -v wanted="      - name: $1" '
    $0 == wanted { selected = 1; next }
    selected && /^        run: \|$/ { body = 1; next }
    body && /^          / { print substr($0, 11); lines++; next }
    body && /^$/ { print; next }
    body { exit }
    END { if (!lines) exit 1 }
  ' "$WORKFLOW" > "$TMP/step.sh"
  [[ -s "$TMP/step.sh" ]]
}

export GITHUB_WORKSPACE="$TMP/workspace"
mkdir -p "$GITHUB_WORKSPACE/findings"
for scanner in panic hypatia unified; do
  case "$scanner" in
    panic) step='Emit panic-attack annotations'; file=panic-attack-findings.json ;;
    hypatia) step='Emit Hypatia annotations'; file=hypatia-findings.json ;;
    unified) step='Emit unified scanner annotations'; file=findings/unified-findings.json ;;
  esac
  jq -n --arg ws "$GITHUB_WORKSPACE" --arg scanner "$scanner" '
    [
      {severity:"high",file:($ws + "/src/a:b,c%\r\n.rs"),line:7,
       description:"first%\r\n::error::injected",reason:"first%\r\n::error::injected",scanner:"fixture"},
      {severity:"low",file:($ws + "/src/default.rs"),message:"fallback",description:"fallback",scanner:"fixture"},
      {severity:"critical",description:"no file",scanner:"fixture"}
    ] as $findings |
    if $scanner == "panic" then
      {weak_points: (($findings | map(.severity |= (if . == "high" then "High" elif . == "low" then "Low" else "Critical" end)))
        + [{severity:"Critical",file:"suppressed.rs",description:"hidden",suppressed:true}])}
    elif $scanner == "unified" then {findings:$findings}
    else $findings end
  ' > "$GITHUB_WORKSPACE/$file"
  extract_step "$step"
  status=0
  (cd "$GITHUB_WORKSPACE" && bash "$TMP/step.sh") > "$TMP/actual" 2> "$TMP/error" || status=$?
  case "$scanner" in panic) label=panic-attack ;; hypatia) label=hypatia ;; unified) label=fixture ;; esac
  cat > "$TMP/expected" <<EOF
::error file=src/a%3Ab%2Cc%25%0D%0A.rs,line=7::[$label] first%25%0D%0A::error::injected
::warning file=src/default.rs,line=1::[$label] fallback
EOF
  if [[ "$status" == 0 ]] && cmp -s "$TMP/expected" "$TMP/actual"; then
    pass "$scanner annotations escape properties/messages, default lines, and omit fileless/suppressed findings"
  else
    fail "$scanner annotation output differs (exit $status)"
    diff -u "$TMP/expected" "$TMP/actual" >&2 || true
    cat "$TMP/error" >&2
  fi
done

# Pons findings are advisory, while scanner errors, invalid reports, skipped
# inputs, and an empty scan scope must block. Canonicalization is stubbed here;
# RFC compliance belongs to the existing real-ijson-jcs CI test branch.
mkdir -p "$TMP/bin"
cat > "$TMP/bin/pons" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
{ printf '%s' "$1"; printf '|%s' "${@:2}"; printf '\n'; } >> "$CALL_LOG"
case "$1" in
  scan) cat "$PONS_FIXTURE"; exit "${SCAN_STATUS:-0}" ;;
  validate-report) exit "${VALIDATE_STATUS:-0}" ;;
  *) exit 90 ;;
esac
EOF
cat > "$TMP/bin/ijson-jcs" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  canon) cat; exit "${CANON_STATUS:-0}" ;;
  check) exit "${CHECK_STATUS:-0}" ;;
  *) exit 90 ;;
esac
EOF
chmod +x "$TMP/bin"/*
export PATH="$TMP/bin:$PATH" CALL_LOG="$TMP/calls" PONS_FIXTURE="$TMP/pons.json"
extract_step 'Scan each Rust crate source directory with complete language coverage'

for scenario in advisory empty-scope skips missing-skips wrong-findings scanner-error validator-error canon-error jcs-error; do
  export RUNNER_TEMP="$TMP/$scenario/temp"
  scan_root="$TMP/$scenario/repo"
  mkdir -p "$RUNNER_TEMP" "$scan_root"
  : > "$CALL_LOG"
  unset SCAN_STATUS VALIDATE_STATUS CANON_STATUS CHECK_STATUS
  jq -cn '{findings:[{severity:"critical"}],scanned:{skipped:[]}}' > "$PONS_FIXTURE"
  if [[ "$scenario" != empty-scope ]]; then
    mkdir -p "$scan_root/crates/first/src" "$scan_root/crates/second/src" "$scan_root/crates/without-source"
  fi
  expected_status=1
  case "$scenario" in
    advisory) expected_status=0 ;;
    skips) jq -cn '{findings:[],scanned:{skipped:["unsupported.rs"]}}' > "$PONS_FIXTURE" ;;
    missing-skips) jq -cn '{findings:[],scanned:{}}' > "$PONS_FIXTURE" ;;
    wrong-findings) jq -cn '{findings:{},scanned:{skipped:[]}}' > "$PONS_FIXTURE" ;;
    scanner-error) export SCAN_STATUS=17; expected_status=17 ;;
    validator-error) export VALIDATE_STATUS=18; expected_status=18 ;;
    canon-error) export CANON_STATUS=19; expected_status=19 ;;
    jcs-error) export CHECK_STATUS=20; expected_status=20 ;;
  esac
  status=0
  (cd "$scan_root" && bash "$TMP/step.sh") > "$TMP/actual" 2>&1 || status=$?
  if [[ "$status" == "$expected_status" ]]; then pass "Pons scan handles $scenario (exit $status)";
  else fail "Pons scan handles $scenario (expected $expected_status, got $status)"; fi
  if [[ "$scenario" == advisory ]]; then
    cat > "$TMP/expected" <<'EOF'
scan|crates/first/src|--format|json|--no-speculative|--require-language|rust|--fail-on-skips
validate-report|pons-findings/first.json
scan|crates/second/src|--format|json|--no-speculative|--require-language|rust|--fail-on-skips
validate-report|pons-findings/second.json
EOF
    if cmp -s "$TMP/expected" "$CALL_LOG" && [[ -s "$scan_root/pons-findings/first.json" && -s "$scan_root/pons-findings/second.json" ]]; then
      pass 'Pons scans every source directory with mandatory Rust coverage and validates each report'
    else fail 'Pons source coverage or validation arguments differ'; fi
  fi
done

printf '\nStatic-analysis workflow unit tests: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
