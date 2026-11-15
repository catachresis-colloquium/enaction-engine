#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>

# Contract tests for the SonarCloud queue gate. They check the workflow wiring
# and run the real inline scripts against a mocked `gh`. The gate verifies a
# receipt of the official SonarQube Cloud analysis. It never runs a scan.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKFLOW="${1:-$PROJECT_DIR/.github/workflows/sonarcloud-queue-gate.yml}"
PROPERTIES="${2:-$PROJECT_DIR/sonar-project.properties}"

PASS=0
FAIL=0

pass() {
  printf 'PASS: %s\n' "$1"
  PASS=$((PASS + 1))
}

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  FAIL=$((FAIL + 1))
}

assert_present() {
  local label="$1"
  local pattern="$2"

  if grep -Eq "$pattern" "$WORKFLOW"; then
    pass "$label"
  else
    fail "$label"
  fi
}

assert_absent() {
  local label="$1"
  local pattern="$2"

  if grep -Eq "$pattern" "$WORKFLOW"; then
    fail "$label"
  else
    pass "$label"
  fi
}

if [ ! -f "$WORKFLOW" ]; then
  printf 'FAIL: workflow not found: %s\n' "$WORKFLOW" >&2
  exit 1
fi
if [ ! -f "$PROPERTIES" ]; then
  printf 'FAIL: properties file not found: %s\n' "$PROPERTIES" >&2
  exit 1
fi

# ---------- names, triggers, permissions ----------
assert_present 'workflow name is unchanged' '^name: SonarCloud Queue Gate[[:space:]]*$'
assert_present 'job name is unchanged' '^[[:space:]]{4}name: SonarCloud queue attestation[[:space:]]*$'
assert_present 'runs for pull requests' '^[[:space:]]{2}pull_request:[[:space:]]*$'
assert_present 'runs for merge groups' '^[[:space:]]{2}merge_group:[[:space:]]*$'
assert_present 'reads repository contents only' '^[[:space:]]{2}contents:[[:space:]]*read[[:space:]]*$'
assert_present 'reads check runs' '^[[:space:]]{2}checks:[[:space:]]*read[[:space:]]*$'
assert_present 'reads pull requests for the merge-group lookup' '^[[:space:]]{2}pull-requests:[[:space:]]*read[[:space:]]*$'
assert_absent 'never writes with the workflow token' '^[[:space:]]+[a-z-]+:[[:space:]]*write[[:space:]]*$'
assert_present 'job has a timeout' '^[[:space:]]{4}timeout-minutes:[[:space:]]*[0-9]+[[:space:]]*$'

timeout_minutes=$(sed -n 's/^[[:space:]]\{4\}timeout-minutes:[[:space:]]*\([0-9][0-9]*\)[[:space:]]*$/\1/p' "$WORKFLOW" | head -n 1)
wait_seconds=$(sed -n "s/^[[:space:]]*wait-seconds:[[:space:]]*'\{0,1\}\([0-9][0-9]*\)'\{0,1\}[[:space:]]*$/\1/p" "$WORKFLOW" | head -n 1)
if [[ "$timeout_minutes" =~ ^[0-9]+$ && "$wait_seconds" =~ ^[0-9]+$ ]] \
  && [ "$wait_seconds" -le 300 ] \
  && [ $((timeout_minutes * 60)) -ge $((wait_seconds + 60)) ]; then
  pass 'job timeout fits the action wait budget (at most 300 seconds plus 60 seconds)'
else
  fail "job timeout fits the action wait budget (timeout=$timeout_minutes minutes, wait=$wait_seconds seconds)"
fi

# ---------- no way to hide or weaken the check ----------
assert_absent 'does not weaken failures with continue-on-error' '^[[:space:]]*continue-on-error:'
assert_absent 'has no path filter that could suppress a required check' '^[[:space:]]+paths(-ignore)?:'
assert_absent 'does not run a second scanner' 'sonarqube-scan-action|sonarcloud-github-action|SONAR_TOKEN'
assert_absent 'does not use pull_request_target' '^[[:space:]]*pull_request_target:'
assert_absent 'does not poll inline any more' 'for attempt in'

# ---------- the action reference ----------
action_pattern='hyperpolymath/proven-cicd-and-workflows/actions/sonarcloud-result-check'
assert_present 'calls the proven receipt verifier' "^[[:space:]]+uses:[[:space:]]*${action_pattern}@"
assert_present 'pins the receipt verifier to a 40-character lowercase SHA' "^[[:space:]]+uses:[[:space:]]*${action_pattern}@[0-9a-f]{40}([[:space:]]+#.*)?[[:space:]]*$"
assert_absent 'never uses a moving ref for the receipt verifier' "${action_pattern}@(main|master|v[0-9]|[0-9a-f]{0,39}([[:space:]]|\$))"
uses_count=$(grep -Ec '^[[:space:]]*-?[[:space:]]*uses:' "$WORKFLOW")
if [ "$uses_count" -eq 1 ]; then
  pass 'has exactly one uses: entry'
else
  fail "has exactly one uses: entry (found $uses_count)"
fi
assert_present 'passes the resolved head SHA to the action' '^[[:space:]]+sha:[[:space:]]*\$\{\{[[:space:]]*steps\.subject\.outputs\.sha[[:space:]]*\}\}[[:space:]]*$'

# ---------- project key agrees with sonar-project.properties ----------
prop_org=$(sed -n 's/^sonar\.organization=//p' "$PROPERTIES" | head -n 1)
prop_key=$(sed -n 's/^sonar\.projectKey=//p' "$PROPERTIES" | head -n 1)
wf_key=$(sed -n "s/^[[:space:]]*SONAR_PROJECT_KEY:[[:space:]]*'\{0,1\}\([^'[:space:]]*\)'\{0,1\}[[:space:]]*\$/\1/p" "$WORKFLOW" | head -n 1)
wf_repo=$(sed -n "s/^[[:space:]]*EXPECTED_REPOSITORY:[[:space:]]*'\{0,1\}\([^'[:space:]]*\)'\{0,1\}[[:space:]]*\$/\1/p" "$WORKFLOW" | head -n 1)

if [ "$prop_org" = 'catachresis-colloquium' ]; then
  pass 'sonar-project.properties names the catachresis-colloquium organisation'
else
  fail "sonar-project.properties organisation is '$prop_org'"
fi
if [ -n "$prop_key" ] && [ "$wf_key" = "$prop_key" ]; then
  pass "workflow project key matches sonar-project.properties ($prop_key)"
else
  fail "workflow project key '$wf_key' differs from sonar-project.properties '$prop_key'"
fi
if [ "$prop_key" = 'catachresis-colloquium_enaction-engine' ]; then
  pass 'project key is catachresis-colloquium_enaction-engine'
else
  fail "project key is '$prop_key'"
fi
if [ "$wf_repo" = 'catachresis-colloquium/enaction-engine' ]; then
  pass 'workflow is bound to catachresis-colloquium/enaction-engine'
else
  fail "workflow expected repository is '$wf_repo'"
fi
if grep -Eq 'ASSUMED, NOT VERIFIED|metadatastician' "$PROPERTIES"; then
  fail 'sonar-project.properties still carries the old assumed organisation text'
else
  pass 'sonar-project.properties no longer carries the assumed organisation text'
fi

# ---------- run the real inline scripts against a mocked gh ----------
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

extract_run() {
  local step="$1" dest="$2"
  awk -v target="      - name: $step" '
    $0 == target { found = 1; next }
    found && /^      - / { exit }
    found && /^        run: \|$/ { in_run = 1; next }
    in_run && /^          / { sub(/^          /, ""); print; next }
    in_run && /^$/ { print; next }
    in_run { exit }
    END { if (!in_run) exit 1 }
  ' "$WORKFLOW" > "$dest"
}

if ! extract_run 'Resolve the pull request head' "$tmp/subject.sh" \
  || ! extract_run 'Verify project and receipt context' "$tmp/context.sh" \
  || ! bash -n "$tmp/subject.sh" || ! bash -n "$tmp/context.sh"; then
  fail 'inline scripts can be extracted and parse'
  printf '\nsonarcloud-gate tests: %d passed, %d failed\n' "$PASS" "$FAIL"
  exit "$FAIL"
fi
pass 'inline scripts can be extracted and parse'

mkdir -p "$tmp/bin"
cat > "$tmp/bin/gh" <<'MOCK'
#!/usr/bin/env bash
# Mock gh. MOCK_ERROR=1 simulates a denied API call.
[[ ${MOCK_ERROR:-0} == 0 ]] || exit 1
case "$*" in
  *graphql*) printf '%s\n' "${MOCK_QUEUE_SHA:-}" ;;
  *"/pulls/"*) printf '%s\n' "${MOCK_PR_HEAD:-}" ;;
  *) cat "$MOCK_RESPONSE" ;;
esac
MOCK
chmod +x "$tmp/bin/gh"
export PATH="$tmp/bin:$PATH" MOCK_RESPONSE="$tmp/response.json"

repo='catachresis-colloquium/enaction-engine'
key='catachresis-colloquium_enaction-engine'
sha=0123456789abcdef0123456789abcdef01234567
queue_sha=fedcba9876543210fedcba9876543210fedcba98

subject_case() {
  local label="$1" expected="$2" event="$3" head_ref="$4" pr_sha="$5" pr_num="$6" github_sha="$7"
  local rc=0
  : > "$tmp/output"
  GITHUB_OUTPUT="$tmp/output" GITHUB_REPOSITORY="$repo" GH_REPO="$repo" GITHUB_SHA="$github_sha" \
    EVENT_NAME="$event" HEAD_REF="$head_ref" PR_HEAD_SHA="$pr_sha" PR_NUMBER="$pr_num" \
    MOCK_QUEUE_SHA="${MOCK_QUEUE_SHA:-$github_sha}" MOCK_PR_HEAD="${MOCK_PR_HEAD:-$sha}" \
    bash "$tmp/subject.sh" > "$tmp/log" 2>&1 || rc=$?
  if [ "$expected" = failure ]; then
    if [ "$rc" -ne 0 ] && [ ! -s "$tmp/output" ]; then pass "$label"; else fail "$label (rc=$rc)"; fi
  else
    if [ "$rc" -eq 0 ] && grep -qx "sha=$expected" "$tmp/output"; then pass "$label"; else fail "$label (rc=$rc)"; cat "$tmp/log" >&2; fi
  fi
}

subject_case 'pull request uses its own head SHA' "$sha" pull_request '' "$sha" 47 "$queue_sha"
subject_case 'pull request without a head SHA fails closed' failure pull_request '' '' 47 "$queue_sha"
subject_case 'pull request with a malformed head SHA fails closed' failure pull_request '' 'main' 47 "$queue_sha"
subject_case 'merge group resolves the pull request head' "$sha" merge_group \
  'refs/heads/gh-readonly-queue/main/pr-47-abcdef' '' '' "$queue_sha"
MOCK_QUEUE_SHA=ffffffffffffffffffffffffffffffffffffffff \
  subject_case 'merge group whose queue entry differs from the event fails closed' failure merge_group \
  'refs/heads/gh-readonly-queue/main/pr-47-abcdef' '' '' "$queue_sha"
subject_case 'merge group ref without a pull request number fails closed' failure merge_group \
  'refs/heads/gh-readonly-queue/main/other' '' '' "$queue_sha"
MOCK_PR_HEAD=not-a-sha subject_case 'merge group with a malformed pull request head fails closed' failure merge_group \
  'refs/heads/gh-readonly-queue/main/pr-47-abcdef' '' '' "$queue_sha"
MOCK_ERROR=1 subject_case 'denied API lookup fails closed' failure merge_group \
  'refs/heads/gh-readonly-queue/main/pr-47-abcdef' '' '' "$queue_sha"
subject_case 'other events fail closed' failure workflow_dispatch '' "$sha" '' "$queue_sha"

jq -n --arg sha "$sha" --arg key "$key" '{
  id: 10, head_sha: $sha, name: "SonarCloud Code Analysis",
  app: {id: 12526, slug: "sonarqubecloud"},
  status: "completed", conclusion: "success",
  started_at: "2026-11-15T10:00:10Z", completed_at: "2026-11-15T10:00:20Z",
  details_url: ("https://sonarcloud.io/dashboard?id=" + $key + "&pullRequest=47")
}' > "$tmp/base.json"

context_case() {
  local label="$1" expected="$2" filter="$3" event="${4:-pull_request}" pr="${5:-47}" id="${6:-10}"
  local event_time="${7-2026-11-15T10:00:00Z}" repository="${8:-$repo}" rc=0
  jq "$filter" "$tmp/base.json" > "$MOCK_RESPONSE"
  GITHUB_REPOSITORY="$repository" EXPECTED_REPOSITORY="$repo" SONAR_PROJECT_KEY="$key" \
    CHECK_RUN_ID="$id" SUBJECT_SHA="$sha" EVENT_NAME="$event" EVENT_TIME="$event_time" PR_NUMBER="$pr" \
    bash "$tmp/context.sh" > "$tmp/log" 2>&1 || rc=$?
  if [ "$expected" = success ]; then
    if [ "$rc" -eq 0 ]; then pass "$label"; else fail "$label (rc=$rc)"; cat "$tmp/log" >&2; fi
  else
    if [ "$rc" -ne 0 ]; then pass "$label"; else fail "$label unexpectedly passed"; fi
  fi
}

context_case 'matching project, pull request and commit' success '.'
context_case 'other repository refused' failure '.' pull_request 47 10 2026-11-15T10:00:00Z other-org/enaction-engine
context_case 'other project refused' failure '.details_url="https://sonarcloud.io/dashboard?id=other_project&pullRequest=47"'
context_case 'old hyperpolymath project key refused' failure '.details_url="https://sonarcloud.io/dashboard?id=hyperpolymath_enaction-engine&pullRequest=47"'
context_case 'old metadatastician project key refused' failure '.details_url="https://sonarcloud.io/dashboard?id=metadatastician_enaction-engine&pullRequest=47"'
context_case 'other pull request refused' failure '.details_url="https://sonarcloud.io/dashboard?id=catachresis-colloquium_enaction-engine&pullRequest=48"'
context_case 'branch dashboard refused for a pull request' failure '.details_url="https://sonarcloud.io/dashboard?id=catachresis-colloquium_enaction-engine&branch=main"'
context_case 'wrong check ID refused' failure '.id=11'
context_case 'wrong commit refused' failure '.head_sha="ffffffffffffffffffffffffffffffffffffffff"'
context_case 'wrong check name refused' failure '.name="SonarQube"'
context_case 'wrong app ID refused' failure '.app.id=15368'
context_case 'wrong app slug refused' failure '.app.slug="impostor"'
context_case 'pending receipt refused' failure '.status="in_progress" | .conclusion=null'
context_case 'skipped receipt refused' failure '.conclusion="skipped"'
context_case 'official failure refused' failure '.conclusion="failure"'
context_case 'receipt older than the event refused' failure '.started_at="2026-11-14T10:00:10Z" | .completed_at="2026-11-14T10:00:20Z"'
context_case 'same-second receipt is not provably fresh' failure '.started_at="2026-11-15T10:00:00Z"'
context_case 'missing start time refused' failure 'del(.started_at)'
context_case 'missing completion time refused' failure 'del(.completed_at)'
context_case 'completion before start refused' failure '.completed_at="2026-11-15T10:00:05Z"'
context_case 'missing event timestamp refused' failure '.' pull_request 47 10 ''
context_case 'invalid event timestamp refused' failure '.' pull_request 47 10 'yesterday;false'
context_case 'untrusted check ID refused' failure '.' pull_request 47 '10;touch /tmp/sonar-injection'
context_case 'untrusted pull request number refused' failure '.' pull_request '47;false'
context_case 'merge group fails closed (no event time can prove freshness)' failure '.' merge_group 47
context_case 'push fails closed (not a trigger of this gate)' failure '.' push ''
MOCK_ERROR=1 context_case 'check API denial fails closed' failure '.'

printf '\nsonarcloud-gate tests: %d passed, %d failed\n' "$PASS" "$FAIL"
exit "$FAIL"
