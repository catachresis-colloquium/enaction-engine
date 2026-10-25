# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# Enaction Engine project task entry point
# https://just.systems/man/en/
#
# Run `just` to list the project recipes. A recipe must perform its advertised
# check or fail clearly; no placeholder command may exit successfully.

set shell := ["bash", "-uc"]
set dotenv-load := true
set positional-arguments := true

# Import auto-generated contractile recipes (must-check, trust-verify, etc.)
# Re-generate with: contractile gen-just
import? "build/contractile.just"

# Project metadata — customize these
project := "enaction-engine"
OWNER := "metadatastician"
REPO := "enaction-engine"
version := "0.1.0"
maturity := "experimental"

# ═══════════════════════════════════════════════════════════════════════════════
# DEFAULT & HELP
# ═══════════════════════════════════════════════════════════════════════════════

# Show all available recipes with descriptions
default:
    @just --list --unsorted

# Show detailed help for a specific recipe
help recipe="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{recipe}}" ]; then
        just --list --unsorted
        echo ""
        echo "Usage: just help <recipe>"
        echo "       just cookbook     # Generate full documentation"
        echo "       just combinations # Show matrix recipes"
    else
        just --show "{{recipe}}" 2>/dev/null || echo "Recipe '{{recipe}}' not found"
    fi

# Show this project's info
info:
    @echo "Project: Enaction Engine"
    @echo "Version: {{version}}"
    @echo "Maturity: {{maturity}}"
    @echo "Recipes: $(just --summary | wc -w)"
    @[ -f ".machine_readable/descriptiles/STATE.a2ml" ] && grep -oP 'phase\s*=\s*"\K[^"]+' .machine_readable/descriptiles/STATE.a2ml | head -1 | xargs -I{} echo "Phase: {}" || true

# Run Invariant Path overlay tools for this repository
invariant-path *ARGS:
    ./scripts/invariant-path.sh {{ARGS}}

# ═══════════════════════════════════════════════════════════════════════════════
# INIT — see build/just/init.just
# ═══════════════════════════════════════════════════════════════════════════════

import? "build/just/init.just"

# >>> container-module (three-tier: OCI · portable engine · stapeln) >>>
# Self-contained. Remove the entire block — this and the import — with `just no-container`.
import? "build/just/container.just"
# <<< container-module <<<

# ═══════════════════════════════════════════════════════════════════════════════
# GROOVE PROTOCOL — see build/just/groove.just
# ═══════════════════════════════════════════════════════════════════════════════

import? "build/just/groove.just"

# ═══════════════════════════════════════════════════════════════════════════════
# PROJECT SELF-ASSESSMENT + OPENSSF COMPLIANCE — see build/just/assess.just
# ═══════════════════════════════════════════════════════════════════════════════

import? "build/just/assess.just"

# ═══════════════════════════════════════════════════════════════════════════════
# BUILD & COMPILE
# ═══════════════════════════════════════════════════════════════════════════════

# Build the default Rust members (the Zig-linked native adapter has its own recipe/job)
build *args:
    cargo build --locked {{args}}

# Build default Rust members in release mode
build-release *args:
    cargo build --locked --release {{args}}

# Rebuild Rust members on file changes (requires `cargo-watch`)
build-watch:
    @command -v cargo-watch >/dev/null 2>&1 || { echo "cargo-watch is required: cargo install cargo-watch" >&2; exit 1; }
    cargo watch -x 'build --locked'

# Clean build artifacts [reversible: rebuild with `just build`]
clean:
    @echo "Cleaning..."
    # NOTE: `build/` is NOT an artifact directory here — it holds 11 tracked
    # files including build/just/init.just, which this Justfile imports. The
    # RSR template ships `rm -rf ... build/ ...`, which destroys `just init`,
    # `just verify` and the proof gates in any repo that runs it.
    cargo clean
    rm -rf dist/ out/

# Deep clean including caches [reversible: rebuild]
clean-all: clean
    rm -rf .cache .tmp

# ═══════════════════════════════════════════════════════════════════════════════
# TEST & QUALITY
# ═══════════════════════════════════════════════════════════════════════════════

# Run tests for the default Rust workspace members, matching the ordinary Rust CI job
test *args:
    cargo test --locked --all-targets {{args}}

# Run default-member tests with captured output disabled
test-verbose:
    cargo test --locked --all-targets -- --nocapture

# Fast compile check of all default-member targets
test-smoke:
    cargo check --locked --all-targets

# Run end-to-end tests: conformance-corpus integrity (SHA256SUMS) + the
# cross-implementation accelerator parity legs (scalar reference and the
# Zig-native backend against the same locked cases). The native leg needs
# the pinned Zig (mise.toml: 0.16.0).
e2e:
    @bash tests/e2e.sh

# Run cross-cutting source and proof-hygiene checks
aspect:
    @bash tests/aspect_tests.sh

# Reproducible synthetic advisory matrix benchmark (portable Rust by default).
# `just bench compare` explicitly requires the pinned Zig native toolchain.
bench mode="scalar" output="target/m1-results" samples="101" rounds="3":
    bash scripts/bench-accelerator-m1.sh "{{mode}}" "{{output}}" "{{samples}}" "{{rounds}}"

# Check that the public readiness record is complete and does not imply an unearned grade.
readiness:
    @bash tests/readiness_status_test.sh

# Ensure generated recovery tasks cannot discard unrelated working-tree edits.
contractile-safety:
    @bash tests/contractile_safety_test.sh

# CRG is graded per component; this repository publishes no aggregate grade or badge.
crg-status:
    @cat docs/status/READINESS.adoc

# Run implemented test and quality categories plus cross-language conformance
test-all: quality e2e
    @echo "All implemented test categories passed."

# Run default-member format, lint, tests, source hygiene, readiness, and contractile-safety checks
quality: fmt-check lint test aspect readiness contractile-safety
    @echo "All implemented quality checks passed."

# Apply configured formatters; review the diff and restore only selected paths if needed.
fix: fmt
    @echo "Workspace formatting complete. Review the diff before committing."

# ═══════════════════════════════════════════════════════════════════════════════
# LINT & FORMAT
# ═══════════════════════════════════════════════════════════════════════════════

# Format workspace sources; inspect the resulting diff before accepting it.
fmt:
    cargo fmt --all

# Check formatting without changes
fmt-check:
    cargo fmt --all -- --check

# Run linter
lint:
    cargo clippy --locked --all-targets -- -D warnings

# ═══════════════════════════════════════════════════════════════════════════════
# RUN & EXECUTE
# ═══════════════════════════════════════════════════════════════════════════════

# This workspace contains libraries and no executable game host yet.
run:
    @echo "Enaction Engine has no runnable application or game host yet." >&2
    @exit 1

# Same explicit boundary for the verbose alias; there is no application output.
run-verbose:
    @echo "Enaction Engine has no runnable application or game host yet." >&2
    @exit 1

# No binary or installable application is published from this library workspace.
install:
    @echo "No installable executable exists; consume the individual crates as libraries." >&2
    @exit 1

# ═══════════════════════════════════════════════════════════════════════════════
# DEPENDENCIES
# ═══════════════════════════════════════════════════════════════════════════════

# Fetch exactly the dependencies recorded in Cargo.lock
deps:
    cargo fetch --locked

# Audit Cargo.lock against RustSec advisories; missing tooling is a hard failure
deps-audit:
    @command -v cargo-audit >/dev/null 2>&1 || { echo "cargo-audit is required: cargo install cargo-audit --locked" >&2; exit 1; }
    cargo audit

# ═══════════════════════════════════════════════════════════════════════════════
# ARRIVAL PACK — agent-facing CLAUDE.md, compiled from a2ml
# ═══════════════════════════════════════════════════════════════════════════════

# Compile CLAUDE.md (the agent arrival pack) from this repo's a2ml
claude-md:
    @bash .machine_readable/arrival-pack/generate.sh

# Fail if CLAUDE.md's generated region drifted from a2ml or was hand-edited
validate-claude-md:
    @bash .machine_readable/arrival-pack/verify.sh

# ═══════════════════════════════════════════════════════════════════════════════
# COAPTATION — typed descriptile↔contractile face-off (homeostasis reading)
# ═══════════════════════════════════════════════════════════════════════════════

# Emit the coaptation receipt: how the descriptiles coapt with the contractiles (SITREP)
coapt:
    @bash .machine_readable/coaptation/coapt.sh --report

# Assemble a re-anchor basis IF the band is red (the drop itself is a human act)
coapt-reanchor:
    @bash .machine_readable/coaptation/coapt.sh --reanchor

# Fail if the committed coaptation receipt drifted from the contractiles/descriptiles
validate-coapt:
    @bash .machine_readable/coaptation/verify.sh

# ═══════════════════════════════════════════════════════════════════════════════
# DOCUMENTATION
# ═══════════════════════════════════════════════════════════════════════════════

# Generate all documentation
docs:
    @mkdir -p docs/generated docs/man
    just cookbook
    just man
    @echo "Documentation generated in docs/"

# Generate justfile cookbook documentation
cookbook:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p docs
    OUTPUT="docs/just-cookbook.adoc"
    echo "= enaction_engine Justfile Cookbook" > "$OUTPUT"
    echo ":toc: left" >> "$OUTPUT"
    echo ":toclevels: 3" >> "$OUTPUT"
    echo "" >> "$OUTPUT"
    echo "Generated: $(date -Iseconds)" >> "$OUTPUT"
    echo "" >> "$OUTPUT"
    echo "== Recipes" >> "$OUTPUT"
    echo "" >> "$OUTPUT"
    just --list --unsorted | while read -r line; do
        if [[ "$line" =~ ^[[:space:]]+([a-z_-]+) ]]; then
            recipe="${BASH_REMATCH[1]}"
            echo "=== $recipe" >> "$OUTPUT"
            echo "" >> "$OUTPUT"
            echo "[source,bash]" >> "$OUTPUT"
            echo "----" >> "$OUTPUT"
            echo "just $recipe" >> "$OUTPUT"
            echo "----" >> "$OUTPUT"
            echo "" >> "$OUTPUT"
        fi
    done
    echo "Generated: $OUTPUT"

# Generate man page
man:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p docs/man
    cat > docs/man/enaction_engine.1 << EOF
    .TH enaction_engine 1 "$(date +%Y-%m-%d)" "{{version}}" "enaction_engine Manual"
    .SH NAME
    enaction_engine \- experimental deterministic game-engine libraries
    .SH SYNOPSIS
    .B just
    [recipe] [args...]
    .SH DESCRIPTION
    Rust game-engine libraries managed with just; no executable game host exists yet.
    .SH AUTHOR
    $(git config user.name 2>/dev/null || echo "Author") <$(git config user.email 2>/dev/null || echo "email")>
    EOF
    echo "Generated: docs/man/enaction_engine.1"

# ═══════════════════════════════════════════════════════════════════════════════
# CI & AUTOMATION
# ═══════════════════════════════════════════════════════════════════════════════

# Run full CI pipeline locally
# proof-check-all is FATAL if any prover toolchain is absent (idris2/lean/agda/coqc):
# the full CI gate must not pass on a machine that cannot verify the proofs.
ci: deps quality e2e proof-check-all
    @echo "Full local CI pipeline complete."

# Install git hooks
install-hooks:
    @mkdir -p .git/hooks
    @cat > .git/hooks/pre-commit << 'HOOKEOF'
    #!/bin/bash
    just fmt-check || exit 1
    just lint || exit 1
    just assail || exit 1
    HOOKEOF
    @chmod +x .git/hooks/pre-commit
    @echo "Git hooks installed"

# ═══════════════════════════════════════════════════════════════════════════════
# SECURITY
# ═══════════════════════════════════════════════════════════════════════════════

# Run locked dependency audit and filesystem vulnerability scan; fail if either tool is absent
security: deps-audit
    @command -v trivy >/dev/null 2>&1 || { echo "trivy is required for the filesystem scan" >&2; exit 1; }
    trivy fs --severity HIGH,CRITICAL --exit-code 1 --quiet .

# Generate an SPDX JSON SBOM; fail rather than report success when Syft is absent
sbom:
    @command -v syft >/dev/null 2>&1 || { echo "syft is required to generate the SBOM" >&2; exit 1; }
    @mkdir -p docs/security
    syft . -o spdx-json > docs/security/sbom.spdx.json

# ═══════════════════════════════════════════════════════════════════════════════
# VALIDATION & COMPLIANCE — see build/just/validate.just
# ═══════════════════════════════════════════════════════════════════════════════

import? "build/just/validate.just"

# ═══════════════════════════════════════════════════════════════════════════════
# STATE MANAGEMENT
# ═══════════════════════════════════════════════════════════════════════════════

# Update STATE.a2ml timestamp
state-touch:
    @if [ -f ".machine_readable/descriptiles/STATE.a2ml" ]; then \
        sed -i 's/last-updated = "[^"]*"/last-updated = "'"$(date +%Y-%m-%d)"'"/' .machine_readable/descriptiles/STATE.a2ml && \
        echo "STATE.a2ml timestamp updated"; \
    fi

# Show current phase from STATE.a2ml
state-phase:
    @grep -oP 'phase\s*=\s*"\K[^"]+' .machine_readable/descriptiles/STATE.a2ml 2>/dev/null | head -1 || echo "unknown"

# ═══════════════════════════════════════════════════════════════════════════════
# GUIX
# ═══════════════════════════════════════════════════════════════════════════════

# Enter Guix development shell (primary)
guix-shell:
    guix shell -D -f guix.scm

# Build with Guix
guix-build:
    guix build -f guix.scm

# ═══════════════════════════════════════════════════════════════════════════════
# HYBRID AUTOMATION
# ═══════════════════════════════════════════════════════════════════════════════

# Run local automation tasks
automate task="all":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{task}}" in
        all) just quality && just e2e && just docs && just state-touch ;;
        cleanup) just clean; echo "Backup files were left untouched; review and remove them explicitly if needed." ;;
        update) just deps && just validate ;;
        *) echo "Unknown: {{task}}. Use: all, cleanup, update" && exit 1 ;;
    esac

# ═══════════════════════════════════════════════════════════════════════════════
# COMBINATORIC MATRIX RECIPES
# ═══════════════════════════════════════════════════════════════════════════════

# Build default Rust members: [debug|release] x optional target x optional feature list
build-matrix mode="debug" target="" features="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=()
    case "{{mode}}" in
      debug) ;;
      release) args+=(--release) ;;
      *) echo "mode must be debug or release" >&2; exit 2 ;;
    esac
    [ -z "{{target}}" ] || args+=(--target "{{target}}")
    [ -z "{{features}}" ] || args+=(--features "{{features}}")
    cargo build --locked "${args[@]}"

# Run Rust unit/integration tests or the e2e/full gate. Verbosity and parallelism
# apply only to the unit and integration suites.
test-matrix suite="unit" verbosity="normal" parallel="true":
    #!/usr/bin/env bash
    set -euo pipefail
    test_args=()
    case "{{verbosity}}" in
      normal) ;;
      verbose) test_args+=(--nocapture) ;;
      *) echo "verbosity must be normal or verbose" >&2; exit 2 ;;
    esac
    case "{{parallel}}" in
      true) ;;
      false) test_args+=(--test-threads=1) ;;
      *) echo "parallel must be true or false" >&2; exit 2 ;;
    esac
    case "{{suite}}" in
      unit) cargo test --locked --lib -- "${test_args[@]}" ;;
      integration) cargo test --locked --tests -- "${test_args[@]}" ;;
      e2e|all)
        if [ "{{verbosity}}" != normal ] || [ "{{parallel}}" != true ]; then
          echo "verbosity and parallelism apply only to unit or integration suites" >&2
          exit 2
        fi
        if [ "{{suite}}" = e2e ]; then just e2e; else just test-all; fi ;;
      *) echo "suite must be unit, integration, e2e, or all" >&2; exit 2 ;;
    esac

# Run an explicit CI stage at quick (quality) or full (proof + E2E) depth.
ci-matrix stage="all" depth="quick":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{stage}}" in
      lint) just fmt-check lint aspect ;;
      test) just test ;;
      build) just build ;;
      security) just security ;;
      all)
        case "{{depth}}" in
          quick) just quality ;;
          full) just ci ;;
          *) echo "depth must be quick or full" >&2; exit 2 ;;
        esac ;;
      *) echo "stage must be lint, test, build, security, or all" >&2; exit 2 ;;
    esac

# Show supported matrix combinations
combinations:
    @echo "Build: just build-matrix [debug|release] [target] [features]"
    @echo "Test:  just test-matrix [unit|integration|e2e|all] [normal|verbose] [true|false]"
    @echo "CI:    just ci-matrix [lint|test|build|security|all] [quick|full]"

# ═══════════════════════════════════════════════════════════════════════════════
# VERSION CONTROL
# ═══════════════════════════════════════════════════════════════════════════════

# Show git status
status:
    @git status --short

# Show recent commits
log count="20":
    @git log --oneline -{{count}}

# Generate CHANGELOG.md with git-cliff
changelog:
    @command -v git-cliff >/dev/null || { echo "git-cliff not found — install: cargo install git-cliff"; exit 1; }
    git cliff --config .machine_readable/configs/git-cliff/cliff.toml --output CHANGELOG.md
    @echo "Generated CHANGELOG.md"

# Preview changelog for unreleased commits (does not write)
changelog-preview:
    @command -v git-cliff >/dev/null || { echo "git-cliff not found — install: cargo install git-cliff"; exit 1; }
    git cliff --config .machine_readable/configs/git-cliff/cliff.toml --unreleased --strip header

# Tag a new release (usage: just release-tag 1.2.3)
release-tag version:
    #!/usr/bin/env bash
    set -euo pipefail
    TAG="v{{version}}"
    if git rev-parse "$TAG" >/dev/null 2>&1; then
        echo "Tag $TAG already exists"
        exit 1
    fi
    just changelog
    git add CHANGELOG.md
    git commit -m "chore(release): prepare $TAG"
    git tag -a "$TAG" -m "Release $TAG"
    echo "Created tag $TAG. Publish it only from the reviewed release commit and follow the draft-release gate."

# ═══════════════════════════════════════════════════════════════════════════════
# UTILITIES
# ═══════════════════════════════════════════════════════════════════════════════

# Count lines of code
loc:
    @find . \( -name "*.rs" -o -name "*.ex" -o -name "*.exs" -o -name "*.res" -o -name "*.gleam" -o -name "*.zig" -o -name "*.idr" -o -name "*.hs" -o -name "*.ncl" -o -name "*.scm" -o -name "*.adb" -o -name "*.ads" \) -not -path './target/*' -not -path './_build/*' 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 || echo "0"

# Show TODO comments; scanner errors must not be reported as an empty result.
todos:
    #!/usr/bin/env bash
    set -euo pipefail
    if grep -rnE 'TODO|FIXME|HACK|XXX' \
        --include='*.rs' --include='*.ex' --include='*.res' --include='*.gleam' \
        --include='*.zig' --include='*.idr' --include='*.hs' \
        --exclude-dir=.git --exclude-dir=target .; then
        exit 0
    else
        status=$?
        if [ "$status" -eq 1 ]; then
            echo "No TODOs"
        else
            echo "TODO scan failed (grep exit $status)" >&2
            exit "$status"
        fi
    fi

# Open in editor
edit:
    ${EDITOR:-code} .

# Run high-rigor security assault using panic-attacker
maint-assault:
    @./.machine_readable/scripts/maintenance/maint-assault.sh

# Run panic-attack's static-analysis scan; missing scanner is a hard failure
assail:
    @command -v panic-attack >/dev/null 2>&1 || { echo "panic-attack is required: https://github.com/hyperpolymath/panic-attacker" >&2; exit 1; }
    panic-attack assail .


# Self-diagnostic — required tools and absolute developer-specific paths
doctor:
    #!/usr/bin/env bash
    set -euo pipefail
    failed=0
    echo "Running diagnostics for enaction-engine..."
    for tool in just git; do
        if command -v "$tool" >/dev/null 2>&1; then
            echo "  [OK] $tool"
        else
            echo "  [FAIL] $tool not found" >&2
            failed=1
        fi
    done
    echo "Checking for absolute developer-specific paths..."
    if grep -rnE '/home/[[:alnum:]_.-]+/|[A-Za-z]:\\Users\\' \
        --include='*.rs' --include='*.ex' --include='*.res' \
        --include='*.gleam' --include='*.sh' \
        --exclude-dir=.git --exclude-dir=target .; then
        echo "  [FAIL] developer-specific absolute path found" >&2
        failed=1
    else
        status=$?
        if [ "$status" -eq 1 ]; then
            echo "  [OK] no developer-specific absolute paths"
        else
            echo "  [FAIL] path scan failed (grep exit $status)" >&2
            failed=1
        fi
    fi
    echo "Diagnostics complete."
    exit "$failed"

# Guided tour of key features
tour:
    @echo "=== enaction-engine Tour ==="
    @echo ""
    @echo "1. Project structure:"
    @ls -la
    @echo ""
    @echo "2. Available commands: just --list"
    @echo ""
    @echo "3. Read README.adoc for full overview"
    @echo "4. Read EXPLAINME.adoc for architecture decisions"
    @echo "5. Run 'just doctor' to check your setup"
    @echo ""
    @echo "Tour complete! Try 'just --list' to see all available commands."

# Open feedback channel with diagnostic context
help-me:
    @echo "=== enaction-engine Help ==="
    @echo "Platform: $(uname -s) $(uname -m)"
    @echo "Shell: $SHELL"
    @echo ""
    @echo "To report an issue:"
    @echo "  https://github.com/metadatastician/enaction-engine/issues/new"
    @echo ""
    @echo "Include the output of 'just doctor' in your report."

# ═══════════════════════════════════════════════════════════════════════════════
# FORMAL VERIFICATION (PROOFS) — see build/just/proofs.just
# ═══════════════════════════════════════════════════════════════════════════════

import? "build/just/proofs.just"

# ═══════════════════════════════════════════════════════════════════════════════
# SESSION MANAGEMENT (THIN BINDINGS TO CENTRAL STANDARDS)
# ═══════════════════════════════════════════════════════════════════════════════

# Show canonical session-management command model
session-help:
    @echo "Canonical command model:"
    @echo "  intake repo <path>"
    @echo "  checkpoint change <path>"
    @echo "  verify maintenance <path>"
    @echo "  verify substantial <path>"
    @echo "  verify release <path>"
    @echo "  close planned <path>"
    @echo "  close urgent <path>"
    @echo "  recover repo <path>"
    @echo "  handover full <path>"
    @echo "  handover split <path>"
    @echo "  handover model <path>"
    @echo "  handover human <path>"
    @echo ""
    @echo "Use Just aliases below (thin wrappers around ./session/dispatch.sh)."

# Canonical aliases (friendly recipe names that map to canonical commands)
intake-repo path=".":
    @./session/dispatch.sh intake repo "{{path}}"

checkpoint-change path=".":
    @./session/dispatch.sh checkpoint change "{{path}}"

verify-maintenance path=".":
    @./session/dispatch.sh verify maintenance "{{path}}"

verify-substantial path=".":
    @./session/dispatch.sh verify substantial "{{path}}"

verify-release path=".":
    @./session/dispatch.sh verify release "{{path}}"

close-planned path=".":
    @./session/dispatch.sh close planned "{{path}}"

close-urgent path=".":
    @./session/dispatch.sh close urgent "{{path}}"

recover-repo path=".":
    @./session/dispatch.sh recover repo "{{path}}"

handover-full path=".":
    @./session/dispatch.sh handover full "{{path}}"

handover-split path=".":
    @./session/dispatch.sh handover split "{{path}}"

handover-model path=".":
    @./session/dispatch.sh handover model "{{path}}"

handover-human path=".":
    @./session/dispatch.sh handover human "{{path}}"

# Scan the working tree for verified secrets; missing tooling or findings are failures.
secret-scan-trufflehog:
    @command -v trufflehog >/dev/null 2>&1 || { echo "trufflehog is required: https://github.com/trufflesecurity/trufflehog" >&2; exit 1; }
    trufflehog filesystem . --only-verified
