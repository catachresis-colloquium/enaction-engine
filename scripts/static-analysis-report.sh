#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Validate and normalize the pinned scanner wire formats. No scanner failure,
# malformed report, unknown severity, or JCS failure is converted to an empty
# findings set.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  scripts/static-analysis-report.sh validate-panic REPORT.json
  scripts/static-analysis-report.sh validate-hypatia REPORT.json
  scripts/static-analysis-report.sh validate-bridge REPORT.json
  scripts/static-analysis-report.sh canonicalize INPUT.json OUTPUT.json
  scripts/static-analysis-report.sh normalize-panic REPORT.json
  scripts/static-analysis-report.sh normalize-hypatia REPORT.json
  scripts/static-analysis-report.sh assemble PANIC.json HYPATIA.json BRIDGE.json OUTPUT.json REPOSITORY SHA REF RUN_ID TIMESTAMP

All scanner reports must be canonical I-JSON (RFC 7493 / RFC 8785) and pass
`ijson-jcs check` before they are used.
EOF
  exit 2
}

fail() {
  echo "static-analysis-report: $*" >&2
  exit 1
}

require_tools() {
  command -v jq >/dev/null 2>&1 || fail "jq is required"
  command -v ijson-jcs >/dev/null 2>&1 || fail "ijson-jcs is required; refusing to skip JSON/JCS validation"
}

require_file() {
  local file="$1"
  [[ -s "$file" ]] || fail "missing or empty report: $file"
}

check_jcs() {
  local file="$1"
  require_file "$file"
  ijson-jcs check "$file" >/dev/null || fail "report is not valid canonical I-JSON: $file"
}

canonicalize() {
  [[ "$#" -eq 2 ]] || usage
  require_tools
  local input="$1" output="$2" tmp
  require_file "$input"
  mkdir -p "$(dirname "$output")"
  tmp="$(mktemp "$(dirname "$output")/.ijson-jcs.XXXXXX")"
  if ! ijson-jcs canon < "$input" > "$tmp"; then
    rm -f -- "$tmp"
    fail "input is not valid interoperable JSON: $input"
  fi
  if ! ijson-jcs check "$tmp" >/dev/null; then
    rm -f -- "$tmp"
    fail "canonicalization did not produce valid JCS: $input"
  fi
  mv -- "$tmp" "$output"
}

validate_panic() {
  local file="$1"
  require_tools
  check_jcs "$file"
  jq -e '
    type == "object"
    and .schema_version == "2.5"
    and (.weak_points | type == "array")
    and (.statistics | type == "object")
    and (.file_statistics | type == "array")
    and (.weak_points | all(.[];
      type == "object"
      and (.severity | type == "string" and IN("Low", "Medium", "High", "Critical"))
      and (.description | type == "string")
      and ((.suppressed == null) or (.suppressed | type == "boolean"))
      and ((.file == null) or (.file | type == "string"))
      and ((.line == null) or (.line | type == "number"))
    ))
  ' "$file" >/dev/null || fail "panic-attack report schema mismatch: $file"
}

validate_hypatia() {
  local file="$1"
  require_tools
  check_jcs "$file"
  jq -e '
    type == "array"
    and all(.[];
      type == "object"
      and (.severity | type == "string" and IN("critical", "high", "medium", "low", "info", "warn", "warning"))
      and ((.file == null) or (.file | type == "string"))
      and ((.line == null) or (.line | type == "number"))
    )
  ' "$file" >/dev/null || fail "Hypatia report schema mismatch: $file"
}

validate_bridge() {
  local file="$1"
  require_tools
  check_jcs "$file"
  jq -e '
    type == "object"
    and .schema_version == "0.2.0"
    and (.total_dependencies | type == "number" and . >= 0)
    and (.vulnerable_dependencies | type == "number" and . >= 0)
    and (.cves | type == "array")
    and (.mitigated | type == "number" and . >= 0)
    and (.unmitigable | type == "number" and . >= 0)
    and (.concatenative | type == "number" and . >= 0)
    and (.informational | type == "number" and . >= 0)
    and (.cves | all(.[];
      type == "object"
      and (.classification | type == "string" and IN("mitigable", "unmitigable", "informational"))
    ))
    and (.vulnerable_dependencies == (.cves | length))
    and (.mitigated == ([.cves[] | select(.classification == "mitigable")] | length))
    and (.unmitigable == ([.cves[] | select(.classification == "unmitigable")] | length))
    and (.informational == ([.cves[] | select(.classification == "informational")] | length))
  ' "$file" >/dev/null || fail "Patch Bridge report schema/invariant mismatch: $file"
}

normalize_panic() {
  local file="$1"
  validate_panic "$file"
  jq -cS '
    [.weak_points[]
      | select(.suppressed != true)
      | . + {scanner: "panic-attack", severity: (.severity | ascii_downcase)}]
  ' "$file"
}

normalize_hypatia() {
  local file="$1"
  validate_hypatia "$file"
  jq -cS '
    [.[]
      | . + {
          scanner: "hypatia",
          severity: (if (.severity | ascii_downcase) == "warn" or (.severity | ascii_downcase) == "warning"
                     then "medium" else (.severity | ascii_downcase) end)
        }]
  ' "$file"
}

assemble() {
  [[ "$#" -eq 9 ]] || usage
  local panic_file="$1" hypatia_file="$2" bridge_file="$3" output_file="$4"
  local repository="$5" sha="$6" ref="$7" run_id="$8" timestamp="$9"
  validate_panic "$panic_file"
  validate_hypatia "$hypatia_file"
  validate_bridge "$bridge_file"

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap "rm -rf -- '$tmp_dir'" RETURN
  normalize_panic "$panic_file" > "$tmp_dir/panic-findings.json"
  normalize_hypatia "$hypatia_file" > "$tmp_dir/hypatia-findings.json"
  jq -nS \
    --arg repository "$repository" \
    --arg commit_sha "$sha" \
    --arg ref "$ref" \
    --arg run_id "$run_id" \
    --arg timestamp "$timestamp" \
    --slurpfile panic "$tmp_dir/panic-findings.json" \
    --slurpfile hypatia "$tmp_dir/hypatia-findings.json" \
    --slurpfile bridge "$bridge_file" \
    '{
      schema_version: "1.2.0",
      repository: $repository,
      commit_sha: $commit_sha,
      ref: $ref,
      run_id: $run_id,
      timestamp: $timestamp,
      findings: ($panic[0] + $hypatia[0]),
      patch_bridge: $bridge[0]
    }' > "$tmp_dir/unified-findings.json"

  mkdir -p "$(dirname "$output_file")"
  ijson-jcs canon < "$tmp_dir/unified-findings.json" > "$output_file"
  check_jcs "$output_file"
  jq -e '
    type == "object"
    and .schema_version == "1.2.0"
    and (.findings | type == "array")
    and (.patch_bridge | type == "object")
  ' "$output_file" >/dev/null || fail "unified report schema mismatch: $output_file"
}

[[ "$#" -ge 1 ]] || usage
command_name="$1"
shift
case "$command_name" in
  validate-panic)
    [[ "$#" -eq 1 ]] || usage
    validate_panic "$1"
    ;;
  validate-hypatia)
    [[ "$#" -eq 1 ]] || usage
    validate_hypatia "$1"
    ;;
  validate-bridge)
    [[ "$#" -eq 1 ]] || usage
    validate_bridge "$1"
    ;;
  canonicalize)
    canonicalize "$@"
    ;;
  normalize-panic)
    [[ "$#" -eq 1 ]] || usage
    normalize_panic "$1"
    ;;
  normalize-hypatia)
    [[ "$#" -eq 1 ]] || usage
    normalize_hypatia "$1"
    ;;
  assemble)
    assemble "$@"
    ;;
  *) usage ;;
esac
