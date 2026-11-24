# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# VALIDATION & COMPLIANCE
#
# Imported by ../../Justfile via `import? "build/just/validate.just"`.
# These recipes check selected repository-structure and metadata invariants.
# They are not a full RSR compliance assessment or certification. Run via
# `just validate`.

# Check selected RSR-derived structural invariants; this is not a certification.
validate-rsr:
    #!/usr/bin/env bash
    cd "{{justfile_directory()}}"
    echo "=== Repository Structure Baseline ==="
    MISSING=""
    for f in .editorconfig .gitignore Justfile README.adoc LICENSE 0-AI-MANIFEST.a2ml; do
        [ -f "$f" ] || MISSING="$MISSING $f"
    done
    for f in .machine_readable/descriptiles/STATE.a2ml .machine_readable/descriptiles/META.a2ml .machine_readable/descriptiles/ECOSYSTEM.a2ml .machine_readable/descriptiles/anchors/ANCHOR.a2ml .machine_readable/policies/MAINTENANCE-AXES.a2ml .machine_readable/policies/MAINTENANCE-CHECKLIST.a2ml .machine_readable/policies/SOFTWARE-DEVELOPMENT-APPROACH.a2ml; do
        [ -f "$f" ] || MISSING="$MISSING $f"
    done
    for f in docs/legal/SOURCE-NOTICE.txt docs/legal/EXHIBIT-B-QUANTUM-SAFE.txt LICENSES/MPL-2.0.txt LICENSES/CC-BY-SA-4.0.txt LICENSES/AGPL-3.0-or-later.txt; do
        [ -f "$f" ] || MISSING="$MISSING $f"
    done
    if [ ! -d "src/interface/abi" ] && [ ! -d "src/interface/Abi" ]; then
        MISSING="$MISSING src/interface/abi"
    fi
    for f in src/interface/ffi src/interface/generated; do
        [ -d "$f" ] || MISSING="$MISSING $f"
    done
    for f in docs/governance/MAINTENANCE-CHECKLIST.adoc docs/governance/SOFTWARE-DEVELOPMENT-APPROACH.adoc; do
        [ -f "$f" ] || MISSING="$MISSING $f"
    done
    if [ -f ".machine_readable/descriptiles/META.a2ml" ]; then
        grep -q 'axis-1 = "must > intend > like"' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:axis-1"
        grep -q 'axis-2 = "corrective > adaptive > perfective"' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:axis-2"
        grep -q 'axis-3 = "systems > compliance > effects"' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:axis-3"
        grep -q 'scoping-first = true' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:scoping-first"
        grep -q 'idris-unsound-scan = "believe_me/assert_total"' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:idris-unsound-scan"
        grep -q 'audit-focus = "systems in place, documentation explains actual state, safety/security accounted for, observed effects reviewed"' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:audit-focus"
        grep -q 'compliance-focus = "seams/compromises/exception register, bounded exceptions, anti-drift checks"' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:compliance-focus"
        grep -q 'effects-evidence = "benchmark execution/results and maintainer status dialogue/review"' .machine_readable/descriptiles/META.a2ml || MISSING="$MISSING META.a2ml:effects-evidence"
        grep -q 'compliance-tooling = "panic-attack"' .machine_readable/policies/MAINTENANCE-AXES.a2ml || MISSING="$MISSING MAINTENANCE-AXES.a2ml:compliance-tooling"
        grep -q 'effects-tooling = "ecological checking with sustainabot guidance"' .machine_readable/policies/MAINTENANCE-AXES.a2ml || MISSING="$MISSING MAINTENANCE-AXES.a2ml:effects-tooling"
        grep -q 'source-human = "docs/governance/MAINTENANCE-CHECKLIST.adoc"' .machine_readable/policies/MAINTENANCE-CHECKLIST.a2ml || MISSING="$MISSING MAINTENANCE-CHECKLIST.a2ml:source-human"
        grep -q 'source-human = "docs/governance/SOFTWARE-DEVELOPMENT-APPROACH.adoc"' .machine_readable/policies/SOFTWARE-DEVELOPMENT-APPROACH.a2ml || MISSING="$MISSING SOFTWARE-DEVELOPMENT-APPROACH.a2ml:source-human"
    fi
    if [ -n "$MISSING" ]; then
        echo "MISSING:$MISSING"
        exit 1
    fi
    echo "Repository structure baseline: PASS (this is not a certification)"

# Structural smoke check for STATE.a2ml; this is not a full A2ML parser.
validate-state:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    STATE=".machine_readable/descriptiles/STATE.a2ml"
    if [ ! -f "$STATE" ]; then
        echo "FAIL: $STATE is missing" >&2
        exit 1
    fi
    grep -q '^\[metadata\]' "$STATE" || { echo "FAIL: $STATE has no [metadata] section" >&2; exit 1; }
    grep -Eq '^[[:space:]]*project[[:space:]]*=' "$STATE" || { echo "FAIL: $STATE has no project field" >&2; exit 1; }
    echo "STATE.a2ml structural smoke check passed (not a full A2ML parse)"

# Validate the AI installation guide only when this project provides one.
# A missing optional guide is reported as SKIP, not as a compliance pass.
validate-ai-install:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    GUIDE="docs/AI_INSTALLATION_GUIDE.adoc"
    README="README.adoc"

    if [ ! -f "$GUIDE" ]; then
        echo "SKIP: optional AI installation guide is not present"
        exit 0
    fi

    errors=0
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT

    if grep -nF '[TODO-AI-INSTALL' "$GUIDE" >"$tmp"; then
        echo "FAIL: unfilled [TODO-AI-INSTALL] markers in $GUIDE:"
        head -10 "$tmp"
        errors=1
    else
        status=$?
        if [ "$status" -gt 1 ]; then
            echo "FAIL: could not scan $GUIDE (grep exit $status)" >&2
            errors=1
        fi
    fi

    if ! grep -qF 'ai-implementation' "$GUIDE"; then
        echo "FAIL: missing [[ai-implementation]] anchor in $GUIDE"
        errors=1
    fi
    if ! grep -qi 'privacy' "$GUIDE"; then
        echo "FAIL: missing privacy notice in $GUIDE"
        errors=1
    fi
    if ! grep -qi 'AI-Assisted Installation' "$README"; then
        echo "FAIL: $README has no AI-Assisted Installation section"
        errors=1
    fi

    if grep -nF '[TODO-AI-INSTALL' "$README" >"$tmp"; then
        echo "FAIL: unfilled [TODO-AI-INSTALL] markers in $README:"
        head -10 "$tmp"
        errors=1
    else
        status=$?
        if [ "$status" -gt 1 ]; then
            echo "FAIL: could not scan $README (grep exit $status)" >&2
            errors=1
        fi
    fi

    if ! grep -q 'git clone' "$GUIDE"; then
        echo "WARN: $GUIDE has no git clone example; review its setup instructions"
    fi

    if [ "$errors" -ne 0 ]; then
        echo "AI installation guide validation failed ($errors issue(s))" >&2
        exit 1
    fi
    echo "AI installation guide validation passed"

# Full validation suite
validate: validate-rsr validate-state validate-ai-install
    @echo "All configured repository validation checks passed."
