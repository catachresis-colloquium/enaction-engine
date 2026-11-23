#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
#
# check-identity.sh -- the repo id is a derived fact; fail loudly on drift.
#
# Canon (hyperpolymath/standards 2-protocols/chora/CHORA-SPEC.adoc, rules
# U1, U4, V8, V9, L1, B1, Q2; gv-clade-index ADR 0008):
#   * the id is minted once, from the BIRTH path, and never moves;
#   * it is the ADR-008 profile C UUIDv8 of "<domain>:github.com/<owner>/<name>"
#     = the first 16 bytes of SHA-256 of that text, version nibble 8,
#     variant bits 10;
#   * a repo born before V9 and registered late keeps the domain
#     "gv-clade-index" (L1);
#   * a moved repo's :beholding-chora is its birth estate (B1), the profile C
#     UUIDv8 of "chora:estate/github.com/<birth owner>";
#   * a deed carries both as opaque #uuid literals; #u5 is an error (Q2).
#
# This file re-implements profile C in POSIX sh only to CHECK, never to mint.
# The registry mints with standards' scripts/uuid-v8.js. Drift between the two
# is caught first: the script refuses to check anything until it reproduces
# the spec's known answer for github.com/hyperpolymath/standards.
#
# Usage: check-identity.sh            check CLADE.a2ml against the deed
#        check-identity.sh --self-test  also prove a planted drift is refused
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLADE="$ROOT/.machine_readable/descriptiles/CLADE.a2ml"
DEED="$ROOT/enaction-engine_chora.deed"

fail() { echo "FAIL: $*" >&2; exit 1; }

# profile_c TEXT -> lowercase 8-4-4-4-12 UUIDv8 (ADR-008 profile C)
profile_c() {
  h=$(printf '%s' "$1" | sha256sum | cut -c1-32)
  n=$(printf '%s' "$h" | cut -c17)
  v=$(printf '%x' $(( (0x$n & 3) | 8 )))
  printf '%s-%s-8%s-%s%s-%s\n' \
    "$(printf '%s' "$h" | cut -c1-8)" "$(printf '%s' "$h" | cut -c9-12)" \
    "$(printf '%s' "$h" | cut -c14-16)" "$v" "$(printf '%s' "$h" | cut -c18-20)" \
    "$(printf '%s' "$h" | cut -c21-32)"
}

# 1. Known answers from CHORA-SPEC section 9 (computed there with deed_lint.js).
kat() {
  got=$(profile_c "$1")
  [ "$got" = "$2" ] || fail "profile C drift: '$1' gives $got, canon says $2"
}
kat "gv-clade-index:github.com/hyperpolymath/standards" "b32d6be4-73ba-873e-9d61-e57f43c75cd1"
kat "chora:estate/github.com/metadatastician" "1642c392-8eac-8e3f-b762-2898b9344228"
kat "chora:estate/github.com/catachresis-colloquium" "ca875671-2b07-84f1-b009-621713eec801"

# 2. Read the recorded preimage parts from CLADE.a2ml (the birth path lives
#    there and in the registry archive, never in the deed: U4).
field() { sed -n "s/^$1 *= *\"\\([^\"]*\\)\".*/\\1/p" "$CLADE" | head -n1; }
uuid=$(field uuid)
domain=$(field minted-in)
birth=$(field birth-path)
[ -n "$uuid" ] || fail "CLADE.a2ml has no uuid"
[ -n "$domain" ] || fail "CLADE.a2ml has no minted-in"
[ -n "$birth" ] || fail "CLADE.a2ml has no birth-path"
case "$domain" in gv-clade-index|chora-registry) ;; *) fail "minted-in '$domain' is not a registry domain";; esac
birth_owner=${birth%%/*}

check() {
  want=$(profile_c "$domain:github.com/$birth")
  [ "$1" = "$want" ] || fail "$2 is $1; profile C of $domain:github.com/$birth is $want"
}
check "$uuid" "CLADE.a2ml uuid"

# 3. The deed must carry the same id, opaquely, and the birth estate.
deed_id=$(sed -n 's/^ *:repo-uuid #uuid"\([0-9a-f-]*\)".*/\1/p' "$DEED")
[ -n "$deed_id" ] || fail "deed :repo-uuid is not an opaque #uuid literal (#u5 is an error under Q2)"
check "$deed_id" "deed :repo-uuid"
estate=$(sed -n 's/^ *:beholding-chora #uuid"\([0-9a-f-]*\)".*/\1/p' "$DEED")
want_estate=$(profile_c "chora:estate/github.com/$birth_owner")
[ "$estate" = "$want_estate" ] || fail "deed :beholding-chora is '$estate'; birth estate $birth_owner is $want_estate (B1)"

# 4. Parity while both forms exist. CLADE.a2ml is deprecated (CHORA-SPEC 11.1
#    rule 5): its identity facts are clauses of the chora deed and its
#    lifecycle is the (status) clause of STATE.deed (standards
#    clade-to-repo-deed.adoc section 3; A7 J15). Until the legacy file is
#    deleted, a fact that differs between the two is drift and fails here.
#    Delete this block in the commit that deletes CLADE.a2ml.
STATE="$ROOT/.machine_readable/descriptiles/STATE.deed"
[ -f "$STATE" ] || fail "no STATE.deed: the (status) clause has no home (A7 J15)"
# deed_val FILE KEY -> the value of the first ":KEY value" in FILE, unquoted
deed_val() { sed -n "s/^ *:$2 \\(.*[^)]\\))*\$/\\1/p" "$1" | head -n1 | sed "s/^\"\\(.*\\)\"\$/\\1/"; }
same() {
  [ -n "$2" ] || [ "$3" = "empty-ok" ] || fail "parity: CLADE.a2ml has no $1"
  [ "$2" = "$4" ] || fail "parity: $1 is \"$2\" in CLADE.a2ml and \"$4\" in $5"
}
same primary-forge  "$(field primary-forge)"  - "$(deed_val "$DEED" primary-forge)"  "the chora (identity)"
same primary-owner  "$(field primary-owner)"  - "$(deed_val "$DEED" owner)"          "the chora (identity)"
same prefixed-name  "$(field prefixed-name)"  - "$(deed_val "$DEED" prefixed-name)"  "the chora (identity)"
same canonical-name "$(field canonical-name)" - "$(deed_val "$DEED" canonical-name)" "the chora head"
same canonical-name "$(field canonical-name)" - "$(deed_val "$STATE" canonical-name)" "STATE.deed"
same github         "$(field github)"         - "$(deed_val "$DEED" github)"         "the chora (forges)"
same gitlab         "$(field gitlab)"   empty-ok "$(deed_val "$DEED" gitlab)"         "the chora (forges)"
same bitbucket      "$(field bitbucket)" empty-ok "$(deed_val "$DEED" bitbucket)"     "the chora (forges)"
same type           "$(field type)"           - "$(deed_val "$DEED" type)"           "the chora (lineage)"
same parent         "$(field parent)"   empty-ok "$(deed_val "$DEED" parent)"         "the chora (lineage)"
same born           "$(field born)"           - "$(deed_val "$DEED" born)"           "the chora (lineage)"
same instantiated-from "$(field instantiated-from)" - "$(deed_val "$DEED" instantiated-from)" "the chora (lineage)"
same phase          "$(field phase)"          - "$(deed_val "$STATE" phase)"         "STATE.deed (status)"
same since          "$(field since)"          - "$(deed_val "$STATE" since)"         "STATE.deed (status)"
state_of=$(sed -n 's/^ *:of-chora #uuid"\([0-9a-f-]*\)".*/\1/p' "$STATE")
[ "$state_of" = "$uuid" ] || fail "STATE.deed :of-chora is \"$state_of\"; the repo id is $uuid (YD08)"
# No (clade ...) clause may appear before the registry holds a placement.
if grep -q "^ *(clade" "$DEED"; then
  fail "the chora carries a (clade ...) placement, but a placement comes from chora-registry and this repo is not registered (CHORA-SPEC 11.1 rule 4)"
fi

if [ "${1:-}" = "--self-test" ]; then
  # A planted drift must be refused: one changed character in the preimage.
  planted=$(profile_c "$domain:github.com/${birth}x")
  [ "$planted" != "$uuid" ] || fail "self-test: a changed preimage gave the same id"
  if ( check "$planted" "planted" ) 2>/dev/null; then
    fail "self-test: the planted id was accepted"
  fi
  echo "self-test: planted drift refused"
fi

echo "PASS: id $uuid = profile C of $domain:github.com/$birth; deed agrees; beholding = $birth_owner estate"
