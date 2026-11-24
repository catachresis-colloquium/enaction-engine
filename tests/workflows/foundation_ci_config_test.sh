#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>

# Contract tests for the foundation CI configuration managed in this repository.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEPENDABOT="$PROJECT_DIR/.github/dependabot.yml"
CODEQL="$PROJECT_DIR/.github/workflows/codeql.yml"
GOVERNANCE="$PROJECT_DIR/.github/workflows/governance.yml"
HYPATIA="$PROJECT_DIR/.github/workflows/hypatia-scan.yml"
SCORECARD="$PROJECT_DIR/.github/workflows/scorecard.yml"
MIRROR="$PROJECT_DIR/.github/workflows/mirror.yml"
ACTION_LOCK="$PROJECT_DIR/.github/workflows/actions.lock"
WORKFLOW_LINTER="$PROJECT_DIR/.github/workflows/workflow-linter.yml"

PASS=0
FAIL=0

pass() {
  printf 'PASS: %s\n' "$1"
  PASS=$((PASS + 1))
}

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  if [ -n "${2:-}" ]; then
    printf '      %s\n' "$2" >&2
  fi
  FAIL=$((FAIL + 1))
}

assert_eq() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  if [ "$actual" = "$expected" ]; then
    pass "$label"
  else
    fail "$label" "expected '$expected', got '${actual:-<empty>}'"
  fi
}

assert_sha() {
  local label="$1"
  local value="$2"

  if [[ "$value" =~ ^[0-9a-f]{40}$ ]]; then
    pass "$label"
  else
    fail "$label" "expected a lowercase 40-character commit SHA, got '${value:-<empty>}'"
  fi
}

dependabot_limit() {
  local ecosystem="$1"

  awk -v wanted="$ecosystem" '
    /^[[:space:]]*-[[:space:]]*package-ecosystem:/ {
      current = $0
      sub(/^[^:]+:[[:space:]]*/, "", current)
      gsub(/["[:space:]]/, "", current)
      next
    }
    current == wanted && /^[[:space:]]*open-pull-requests-limit:/ {
      value = $0
      sub(/^[^:]+:[[:space:]]*/, "", value)
      gsub(/[[:space:]]/, "", value)
      print value
    }
  ' "$DEPENDABOT"
}

action_refs() {
  local action="$1"
  local workflow="$2"

  awk -v action="$action" '
    /^[[:space:]]*uses:[[:space:]]*/ {
      value = $0
      sub(/^[[:space:]]*uses:[[:space:]]*/, "", value)
      sub(/[[:space:]]*#.*/, "", value)
      separator = substr(value, length(action) + 1, 1)
      if (index(value, action) == 1 && (separator == "@" || separator == "/")) {
        ref = value
        sub(/^.*@/, "", ref)
        print ref
      }
    }
  ' "$workflow"
}

locked_commit() {
  local dependency="$1"
  local lock_file="${2:-$ACTION_LOCK}"

  awk -v wanted="    '$dependency':" '
    $0 == wanted { in_entry = 1; next }
    in_entry && substr($0, 1, 4) == "    " && substr($0, 5, 1) != " " && $0 != wanted { exit }
    in_entry && /^[[:space:]]*commit:/ {
      value = $0
      sub(/^.*commit:[[:space:]]*/, "", value)
      gsub(/[\047\"]/, "", value)
      sub(/^sha1-/, "", value)
      print value
      exit
    }
  ' "$lock_file"
}

# Tags the lock records for an action in one workflow's own entry (the
# `workflows:` list for that file), one per line. Other workflows' entries and
# transitive dependency keys are deliberately not consulted.
locked_tags() {
  local action="$1"
  local lock_file="$2"
  local workflow_key="$3"

  awk -v header="    '$workflow_key':" -v prefix="        - '$action@" '
    $0 == header { in_entry = 1; next }
    in_entry && substr($0, 1, 8) != "        " { exit }
    in_entry && index($0, prefix) == 1 {
      value = substr($0, length(prefix) + 1)
      sub(/\047.*$/, "", value)
      print value
    }
  ' "$lock_file"
}

checkout_persist_credentials() {
  awk '
    /^[[:space:]]+-[[:space:]]+(name|uses|run):/ && checkout { exit }
    /^[[:space:]]*uses:[[:space:]]*actions\/checkout@/ { checkout = 1; next }
    checkout && /^[[:space:]]*persist-credentials:/ {
      value = $0
      sub(/^[^:]+:[[:space:]]*/, "", value)
      gsub(/[[:space:]]/, "", value)
      print value
      exit
    }
  ' "$CODEQL"
}

for required_file in "$DEPENDABOT" "$CODEQL" "$GOVERNANCE" "$HYPATIA" "$SCORECARD" "$ACTION_LOCK" "$WORKFLOW_LINTER"; do
  if [ ! -f "$required_file" ]; then
    fail "configuration file exists: ${required_file#"$PROJECT_DIR/"}"
  fi
done

assert_eq 'GitHub Actions updates allow at most two open pull requests' \
  '2' "$(dependabot_limit github-actions)"
assert_eq 'Mix updates allow at most three open pull requests' \
  '3' "$(dependabot_limit mix)"
assert_eq 'npm updates allow at most three open pull requests' \
  '3' "$(dependabot_limit npm)"
assert_eq 'pip updates allow at most three open pull requests' \
  '3' "$(dependabot_limit pip)"

# Regression boundary: Cargo deliberately remains security-only while the
# neighbouring ecosystem blocks receive non-zero routine-update limits.
assert_eq 'Cargo routine version updates remain suppressed' \
  '0' "$(dependabot_limit cargo)"

assert_eq 'Workflow linter runs for every pull request' \
  '1' "$(grep -c '^  pull_request:[[:space:]]*$' "$WORKFLOW_LINTER" || true)"
assert_eq 'Workflow linter is not path-filtered' \
  '0' "$(grep -Ec '^[[:space:]]+paths(-ignore)?:' "$WORKFLOW_LINTER" || true)"
assert_eq 'Workflow linter runs on main pushes' \
  '1' "$(grep -c '^      - main$' "$WORKFLOW_LINTER" || true)"

mapfile -t checkout_refs < <(action_refs actions/checkout "$CODEQL")
assert_eq 'CodeQL has exactly one checkout step' '1' "${#checkout_refs[@]}"
if [ "${#checkout_refs[@]}" -eq 1 ]; then
  assert_eq 'CodeQL checkout uses the locked immutable release tag' 'v7.0.1' "${checkout_refs[0]}"
  assert_eq 'CodeQL checkout tag resolves to the locked commit' \
    '3d3c42e5aac5ba805825da76410c181273ba90b1' "$(locked_commit actions/checkout@v7.0.1)"
fi
assert_eq 'CodeQL checkout does not persist repository credentials' \
  'false' "$(checkout_persist_credentials)"

# CodeQL pin contract: the pin lives in actions.lock; codeql.yml must agree
# with it. Each check prints a reason when violated and nothing when it holds.
codeql_tag_problem_same() {
  local workflow="$1"
  local init analyze

  init="$(action_refs github/codeql-action/init "$workflow")"
  analyze="$(action_refs github/codeql-action/analyze "$workflow")"
  if [ -z "$init" ] || [ "$init" != "$analyze" ]; then
    printf "init uses '%s' but analyze uses '%s'" "${init:-<empty>}" "${analyze:-<empty>}"
  fi
}

codeql_tag_problem_locked() {
  local workflow="$1"
  local lock_file="$2"
  local init recorded

  init="$(action_refs github/codeql-action/init "$workflow")"
  recorded="$(locked_tags github/codeql-action "$lock_file" .github/workflows/codeql.yml | paste -sd, -)"
  if [ -z "$init" ] || [ "$init" != "$recorded" ]; then
    printf "codeql.yml uses '%s' but its lock entry records '%s'" "${init:-<empty>}" "${recorded:-<none>}"
  fi
}

codeql_tag_problem_resolution() {
  local workflow="$1"
  local lock_file="$2"
  local init commit

  init="$(action_refs github/codeql-action/init "$workflow")"
  commit="$(locked_commit "github/codeql-action@$init" "$lock_file")"
  if [[ ! "$init" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    printf "'%s' is not an immutable release tag (vMAJOR.MINOR.PATCH)" "${init:-<empty>}"
  elif [[ ! "$commit" =~ ^[0-9a-f]{40}$ ]]; then
    printf "lock resolves '%s' to '%s', not a 40-character commit SHA" "$init" "${commit:-<empty>}"
  fi
}

assert_no_problem() {
  local label="$1"
  local problem="$2"

  if [ -z "$problem" ]; then
    pass "$label"
  else
    fail "$label" "$problem"
  fi
}

# Both-ways check: a deliberately broken copy of codeql.yml/actions.lock must
# be rejected, naming the expected reason. Plants live in a temporary
# directory only.
assert_codeql_pin_plant_rejected() {
  local label="$1"
  local expected="$2"
  local workflow="$3"
  local lock_file="$4"
  local problems

  problems="$(
    codeql_tag_problem_same "$workflow"
    printf '\n'
    codeql_tag_problem_locked "$workflow" "$lock_file"
    printf '\n'
    codeql_tag_problem_resolution "$workflow" "$lock_file"
  )"
  if [[ "$problems" == *"$expected"* ]]; then
    pass "$label"
  else
    fail "$label" "expected a rejection containing '$expected', got '${problems//$'\n'/ }'"
  fi
}

mapfile -t codeql_refs < <(action_refs github/codeql-action "$CODEQL")
assert_eq 'CodeQL has one init and one analysis action step' '2' "${#codeql_refs[@]}"
assert_no_problem 'CodeQL init and analysis use the same release tag' \
  "$(codeql_tag_problem_same "$CODEQL")"
assert_no_problem 'CodeQL release tag is the tag recorded in actions.lock' \
  "$(codeql_tag_problem_locked "$CODEQL" "$ACTION_LOCK")"
assert_no_problem 'CodeQL release tag resolves to a full locked commit' \
  "$(codeql_tag_problem_resolution "$CODEQL" "$ACTION_LOCK")"

plant_dir="$(mktemp -d)"
trap 'rm -rf "$plant_dir"' EXIT
codeql_tag="$(action_refs github/codeql-action/init "$CODEQL")"
codeql_commit="$(locked_commit "github/codeql-action@$codeql_tag")"

sed '/codeql-action\/analyze@/s/@[^[:space:]]*/@v0.0.1/' "$CODEQL" >"$plant_dir/split.yml"
assert_codeql_pin_plant_rejected 'plant: init and analysis at different tags is rejected' \
  'but analyze uses' "$plant_dir/split.yml" "$ACTION_LOCK"

sed "s#codeql-action/\\(init\\|analyze\\)@$codeql_tag#codeql-action/\\1@v0.0.1#" "$CODEQL" >"$plant_dir/unlocked.yml"
assert_codeql_pin_plant_rejected 'plant: tag absent from actions.lock is rejected' \
  'but its lock entry records' "$plant_dir/unlocked.yml" "$ACTION_LOCK"

# Scoping, both ways: another workflow pinning a different CodeQL tag must not
# matter; a stale codeql.yml entry must not be masked by another workflow.
sed "/'.github\\/workflows\\/oikosbot.yml':/,/hyperpolymath/s#codeql-action@$codeql_tag'#codeql-action@v0.0.1'#" "$ACTION_LOCK" >"$plant_dir/other-workflow.lock"
assert_no_problem 'plant control: a different tag in another workflow'"'"'s lock entry is ignored' \
  "$(codeql_tag_problem_locked "$CODEQL" "$plant_dir/other-workflow.lock")"
sed "/'.github\\/workflows\\/codeql.yml':/,/hyperpolymath\\|oikosbot/s#codeql-action@$codeql_tag'#codeql-action@v0.0.1'#" "$ACTION_LOCK" >"$plant_dir/stale.lock"
assert_codeql_pin_plant_rejected 'plant: stale codeql.yml lock entry is rejected despite a matching global key' \
  'but its lock entry records' "$CODEQL" "$plant_dir/stale.lock"

sed "/codeql-action@$codeql_tag':/,/commit:/s/sha1-[0-9a-f]*/sha1-${codeql_commit:0:7}/" "$ACTION_LOCK" >"$plant_dir/short.lock"
assert_codeql_pin_plant_rejected 'plant: short commit in actions.lock is rejected' \
  'not a 40-character commit SHA' "$CODEQL" "$plant_dir/short.lock"

sed "/codeql-action@$codeql_tag':/,/commit:/s/commit:.*/commit: ''/" "$ACTION_LOCK" >"$plant_dir/empty.lock"
assert_codeql_pin_plant_rejected 'plant: empty commit in actions.lock is rejected' \
  'not a 40-character commit SHA' "$CODEQL" "$plant_dir/empty.lock"

sed "s#codeql-action/\\(init\\|analyze\\)@$codeql_tag#codeql-action/\\1@main#" "$CODEQL" >"$plant_dir/branch.yml"
sed "s/codeql-action@$codeql_tag/codeql-action@main/; /codeql-action@main':/,/ref:/s/ref: .*/ref: 'main'/" "$ACTION_LOCK" >"$plant_dir/branch.lock"
assert_codeql_pin_plant_rejected 'plant: branch name instead of a tag is rejected even if the lock agrees' \
  'not an immutable release tag' "$plant_dir/branch.yml" "$plant_dir/branch.lock"

# The standards repository moved from hyperpolymath/ to metadatastician/
# (the same GitHub repo_id, 1116521501); metadatastician/ is its name now.
#
# Two separate defects kept Governance, Hypatia, Mirror and Scorecard from
# creating a job. Both were measured on pull request 133 (2026-11-17).
#
# 1. The pin. The actions a reusable workflow uses are covered by the lock of
#    the repository that defines it, read at the pinned commit. At da2c748
#    that lock recorded each action these workflows use with `ref:` equal to
#    its own SHA, a record the runner rejects. standards
#    docs/GOVERNANCE-STARTUP-REPAIR.adoc lists da2c748 as POISON. At b8f6051,
#    the commit secret-scanner.yml already ran on, every one of those records
#    names a reviewed tag that still points at the locked commit. This file
#    asserts that pin.
#
# 2. The Actions policy in force for this repository, which this repository
#    cannot change. It refuses, at startup and before any lock is read:
#      - every standards reusable workflow measured except
#        secret-scanner-reusable.yml (governance, hypatia-scan, scorecard,
#        allowlist-preflight, provisioning-check, elixir-ci, js-runtimes);
#        two of those use only actions/checkout, as the admitted one does, so
#        the refusal follows the workflow's path and not its actions;
#      - dtolnay/rust-toolchain and webfactory/ssh-agent, which
#        mirror-reusable.yml uses;
#      - hyperpolymath/oikosbot, by tag and by commit.
#    It admits actions/*, github/*, erlef/setup-beam, editorconfig-checker,
#    denoland/setup-deno, oven-sh/setup-bun and ossf/scorecard-action. The
#    estate's canonical allow-list (standards
#    rhodium-standard-repositories/actions-allowlist/allowed-actions.json)
#    admits everything refused here, so the live list is not a superset of
#    it. Until the owner widens the live list, the pin below is necessary
#    and not sufficient, and no assertion in this file can show otherwise:
#    a run refused at startup creates no job to report from.
SECRET_SCANNER="$PROJECT_DIR/.github/workflows/secret-scanner.yml"
STANDARDS_PIN='b8f60511e76c9015d86990317162dce653c92fe1'
governance_ref="$(action_refs metadatastician/standards/.github/workflows/governance-reusable.yml "$GOVERNANCE")"
hypatia_ref="$(action_refs metadatastician/standards/.github/workflows/hypatia-scan-reusable.yml "$HYPATIA")"
scorecard_ref="$(action_refs metadatastician/standards/.github/workflows/scorecard-reusable.yml "$SCORECARD")"
mirror_ref="$(action_refs metadatastician/standards/.github/workflows/mirror-reusable.yml "$MIRROR")"
secret_scanner_ref="$(action_refs metadatastician/standards/.github/workflows/secret-scanner-reusable.yml "$SECRET_SCANNER")"

assert_sha 'Governance reusable workflow is pinned to an immutable revision' "$governance_ref"
assert_eq 'Governance reusable workflow uses the approved revision' \
  "$STANDARDS_PIN" "$governance_ref"
assert_sha 'Hypatia reusable workflow is pinned to an immutable revision' "$hypatia_ref"
assert_eq 'Hypatia reusable workflow uses the approved revision' \
  "$STANDARDS_PIN" "$hypatia_ref"
assert_sha 'Scorecard reusable workflow is pinned to an immutable revision' "$scorecard_ref"
assert_eq 'Scorecard reusable workflow uses the approved revision' \
  "$STANDARDS_PIN" "$scorecard_ref"
assert_sha 'Mirror reusable workflow is pinned to an immutable revision' "$mirror_ref"
assert_eq 'Mirror reusable workflow uses the approved revision' \
  "$STANDARDS_PIN" "$mirror_ref"
# One pin for every standards caller: a commit is either safe to pin a caller
# to or it is not, and that is a property of the commit's lock, not of the
# workflow file called from it. Moving one caller means moving all five.
assert_sha 'Secret scanner reusable workflow is pinned to an immutable revision' "$secret_scanner_ref"
assert_eq 'Secret scanner reusable workflow uses the approved revision' \
  "$STANDARDS_PIN" "$secret_scanner_ref"

# No workflow may call standards at a commit known to carry a lock the runner
# refuses. Each of these is listed POISON in standards
# docs/GOVERNANCE-STARTUP-REPAIR.adoc (the "consumer census" table).
poisoned_pins="$(grep -hnE '^[[:space:]]*-?[[:space:]]*uses:[[:space:]]*["'"'"']?[^#]*/standards/[^#]*@(fad242d3|da2c748a|e3b9929c|e16a2317)' \
  "$PROJECT_DIR"/.github/workflows/*.yml "$PROJECT_DIR"/.github/workflows/*.yaml 2>/dev/null || true)"
assert_eq 'No workflow calls standards at a commit whose lock the runner refuses' '' "$poisoned_pins"

# mirror-reusable.yml at the approved revision declares its secrets and says
# callers must pass them by name; `secrets: inherit` forwards every secret
# this repository can see (standards#1108 section 6b).
mirror_inherit="$(grep -nE '^[[:space:]]*secrets:[[:space:]]*inherit[[:space:]]*(#.*)?$' "$MIRROR" || true)"
assert_eq 'Mirror caller does not forward every secret with secrets: inherit' '' "$mirror_inherit"
mirror_named="$(grep -cE '^[[:space:]]+(GITLAB_SSH_KEY|BITBUCKET_SSH_KEY|CODEBERG_SSH_KEY|SOURCEHUT_SSH_KEY|DISROOT_SSH_KEY|GITEA_SSH_KEY|RADICLE_KEY|R2_ACCESS_KEY_ID|R2_SECRET_ACCESS_KEY|R2_AGE_RECIPIENT): \$\{\{ secrets\.[A-Z0-9_]+ \}\}$' "$MIRROR" || true)"
assert_eq 'Mirror caller passes the ten declared secrets by name' '10' "$mirror_named"

# No leaf record in this repository's own lock may repeat its key's SHA in
# `ref:`. That is the defect that made da2c748 unusable, and nothing else here
# checks for it: check-lock-sync.sh verifies coverage and closure, not the
# shape of a record.
self_sha_records="$(awk '
  /^dependencies:/ { in_deps = 1; next }
  in_deps && /^    \047[^\047]+\047:/ {
    key = $0; sub(/^    \047/, "", key); sub(/\047:.*/, "", key)
    sha = key; sub(/^.*@/, "", sha)
    next
  }
  in_deps && /^        ref:/ {
    ref = $2; gsub(/\047/, "", ref)
    if (ref == sha && sha ~ /^[0-9a-f]{40}$/) print key
  }
' "$ACTION_LOCK")"
assert_eq 'No lock record repeats its own SHA as ref' '' "$self_sha_records"

# Names with a working replacement that must not come back: mlugg/setup-zig
# (scripts/install-zig.sh) and hyperpolymath/standards (the repository is
# metadatastician/standards now). Comment lines are not uses:.
# hyperpolymath/oikosbot is refused by the policy as well, but it has no
# replacement, so it is not listed: oikosbot.yml creates no job until the
# owner admits it.
refused_uses="$(grep -hE '^[[:space:]]*-?[[:space:]]*uses:[[:space:]]*["'"'"']?(mlugg/|hyperpolymath/standards/)' \
  "$PROJECT_DIR"/.github/workflows/*.yml "$PROJECT_DIR"/.github/workflows/*.yaml 2>/dev/null || true)"
assert_eq 'No workflow uses a replaced action or the old standards name' '' "$refused_uses"

printf '\nfoundation CI configuration tests: %d passed, %d failed\n' "$PASS" "$FAIL"
exit "$FAIL"
