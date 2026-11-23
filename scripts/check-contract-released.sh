#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# check-contract-released.sh: the release gate of ADR-0030.
#
# A kernel contract version has a state, `draft` or `released`. While it is a
# draft, nothing may be published as conforming to it, so a release must
# refuse. This gate asks the built host what its state is, and exits 0 only
# if the host says `released`.
#
#   sh scripts/check-contract-released.sh              the gate
#   sh scripts/check-contract-released.sh --self-test  the gate's own test
#
# The state is read from `enaction-validate --version`, which prints
# `enaction-validate kernel contract <major>.<minor> (<state>)`. Nothing is
# grepped from source. The gate fails closed: it exits non-zero when the
# state is `draft`, when the line is not exactly that shape, when the host
# does not build or run, and when the state is any word but `released`.
#
# ENACTION_VALIDATE names a program to run in place of the cargo build. The
# self-test uses it to plant a host that says each state; nothing else should.
#
# Exit 0: released. Exit 1: refused. Exit 2: bad invocation (self-test only).

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)

# Print the host's `--version` line. A failure to build or run is a failure
# of the gate, which `set -e` turns into a refusal at the call site.
host_version() {
    if [ -n "${ENACTION_VALIDATE:-}" ]; then
        "$ENACTION_VALIDATE" --version
    else
        (cd "$root" && cargo run --quiet --locked -p enaction-contracts \
            --bin enaction-validate -- --version)
    fi
}

# Decide from one version line. Prints the reason; returns 0 only for a
# well-formed line whose state is `released`.
decide() {
    line=$1
    case "$line" in
    *'
'*)
        echo "check-contract-released: REFUSED: the host printed more than one line" >&2
        return 1
        ;;
    esac
    if printf '%s\n' "$line" | grep -Eq '^enaction-validate kernel contract [0-9]+\.[0-9]+ \(released\)$'; then
        echo "check-contract-released: ok: $line"
        return 0
    elif printf '%s\n' "$line" | grep -Eq '^enaction-validate kernel contract [0-9]+\.[0-9]+ \(draft\)$'; then
        echo "check-contract-released: REFUSED: the kernel contract is a draft (ADR-0030): $line" >&2
        return 1
    else
        echo "check-contract-released: REFUSED: cannot read a contract state from: $line" >&2
        return 1
    fi
}

gate() {
    if ! out=$(host_version); then
        echo "check-contract-released: REFUSED: the host did not report its version" >&2
        return 1
    fi
    decide "$out"
}

# Plant a host named `$2` in dir `$1` whose program body is `$3`, and run the
# gate against it.
# Prints the gate's exit status.
plant_and_run() {
    planted=$1/$2
    printf '#!/bin/sh\n%s\n' "$3" >"$planted"
    chmod +x "$planted"
    status=0
    ENACTION_VALIDATE=$planted sh "$here/check-contract-released.sh" >/dev/null 2>&1 || status=$?
    echo "$status"
}

expect() {
    # expect <label> <wanted status> <got status>
    if [ "$2" != "$3" ]; then
        echo "check-contract-released --self-test: FAILED: $1: wanted exit $2, got $3" >&2
        failed=1
    else
        echo "check-contract-released --self-test: ok: $1 (exit $3)"
    fi
}

self_test() {
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT INT TERM
    failed=0

    expect "a draft host is refused" 1 "$(plant_and_run "$tmp" draft \
        'echo "enaction-validate kernel contract 2.0 (draft)"')"
    expect "a released host is accepted" 0 "$(plant_and_run "$tmp" released \
        'echo "enaction-validate kernel contract 2.0 (released)"')"
    expect "draft text before a released suffix is refused" 1 "$(plant_and_run "$tmp" draft_suffix \
        'echo "enaction-validate kernel contract 2.0 (draft) (released)"')"
    expect "trailing draft text is refused" 1 "$(plant_and_run "$tmp" trailing_draft \
        'echo "enaction-validate kernel contract 2.0 (released) (draft)"')"
    expect "a non-digit major is refused" 1 "$(plant_and_run "$tmp" bad_major \
        'echo "enaction-validate kernel contract 2x.0 (released)"')"
    expect "a non-digit minor is refused" 1 "$(plant_and_run "$tmp" bad_minor \
        'echo "enaction-validate kernel contract 2.0x (released)"')"
    expect "an unknown state is refused" 1 "$(plant_and_run "$tmp" unknown \
        'echo "enaction-validate kernel contract 2.0 (rc)"')"
    expect "a line with no state is refused" 1 "$(plant_and_run "$tmp" nostate \
        'echo "enaction-validate kernel contract 2.0"')"
    expect "an empty answer is refused" 1 "$(plant_and_run "$tmp" empty ':')"
    expect "a host that fails is refused" 1 "$(plant_and_run "$tmp" broken \
        'echo "enaction-validate kernel contract 2.0 (released)"; exit 3')"
    expect "two lines are refused, whatever the last says" 1 "$(plant_and_run "$tmp" twolines \
        'echo "enaction-validate kernel contract 2.0 (draft)"; echo "enaction-validate kernel contract 2.0 (released)"')"

    # The gate refuses a missing host program too.
    status=0
    ENACTION_VALIDATE=$tmp/no-such-host sh "$here/check-contract-released.sh" >/dev/null 2>&1 || status=$?
    expect "a missing host is refused" 1 "$status"

    return "$failed"
}

case "${1:-}" in
'')
    gate
    ;;
--self-test)
    self_test
    ;;
*)
    echo "usage: check-contract-released.sh [--self-test]" >&2
    exit 2
    ;;
esac
