# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# FORMAL VERIFICATION (PROOFS)
#
# Imported by ../../Justfile via `import? "build/just/proofs.just"`.
# Recipes here check formal proofs across Idris2, Lean4, Agda, and Coq, plus
# scan for dangerous/unsafe constructs and report status. Run via
# `just proof-check-all` for the full sweep.

# Check all formal proofs (production Idris2 ABI + proof manifests + Rust/Creusot)
proof-check-all: proof-check-idris2-abi proof-check-idris2 proof-check-lean4 proof-check-agda proof-check-coq proof-check-coq-assumptions proof-check-creusot proof-scan-dangerous
    @echo "=== All proof checks complete ==="

# Each proof-check-<prover> delegates to scripts/check-proofs.sh, the single
# source of truth. A missing toolchain is FATAL (never a skip); every module is
# checked from its own source root; a proof file absent from the prover's
# manifest (verification/proofs/<prover>/MANIFEST) is an error. See the script
# header for the four "checks that could not fail" this replaces.

# Check Idris2 proofs (per MANIFEST; fatal if idris2 absent)
proof-check-idris2-abi:
    @command -v idris2 >/dev/null 2>&1 || { echo "FAIL: idris2 not found." >&2; exit 1; }
    @idris2 --typecheck abi.ipkg

# Check the generic Idris2 proof manifest separately. Quarantined templates do
# not become product proofs merely because the production ABI package passes.
proof-check-idris2:
    @bash scripts/check-proofs.sh idris2

# Check Lean4 proofs (per MANIFEST; fatal if lean absent)
proof-check-lean4:
    @bash scripts/check-proofs.sh lean4

# Check Agda proofs (per MANIFEST; fatal if agda absent)
proof-check-agda:
    @bash scripts/check-proofs.sh agda

# Check Coq proofs (per MANIFEST; fatal if coqc absent)
proof-check-coq:
    @bash scripts/check-proofs.sh coq

# Require every theorem in verification/proofs/coq/OBLIGATIONS to be closed
# under the global context (Print Assumptions). Empty or missing list = FAIL.
proof-check-coq-assumptions:
    @bash scripts/check-coq-assumptions.sh

# Both-ways tests: each proof gate must be seen to fail on a planted violation.
proof-gate-tests:
    @bash tests/check_proofs_gate_test.sh
    @bash tests/check_coq_assumptions_test.sh

# Check the exact production Rust modules admitted by the narrow Creusot
# harness. Tool/version/prover absence is fatal; scaffolding cannot pass.
proof-check-creusot:
    @bash scripts/check-creusot.sh

# Scan for dangerous constructs USED in proof code (believe_me, sorry, Admitted,
# postulate, ...). Comments are ignored — a comment naming a banned construct
# must not trip the gate. See scripts/scan-dangerous.sh.
proof-scan-dangerous:
    @bash scripts/scan-dangerous.sh

# Show proof status summary
proof-status:
    #!/usr/bin/env bash
    echo "=== Proof Status ==="
    echo ""
    echo "Idris2: $(find verification/proofs/idris2 -name '*.idr' 2>/dev/null | wc -l) files"
    echo "Lean4:  $(find verification/proofs/lean4 -name '*.lean' 2>/dev/null | wc -l) files"
    echo "Agda:   $(find verification/proofs/agda -name '*.agda' 2>/dev/null | wc -l) files"
    echo "Coq:    $(find verification/proofs/coq -name '*.v' 2>/dev/null | wc -l) files"
    echo "TLA+:   $(find verification/proofs/tlaplus -name '*.tla' 2>/dev/null | wc -l) files"
    echo ""
    # PROOF-STATUS may live at root, docs/status/ (post-#20), .md or .adoc (post-#23)
    for candidate in docs/status/PROOF-STATUS.adoc docs/status/PROOF-STATUS.md PROOF-STATUS.adoc PROOF-STATUS.md; do
        if [ -f "$candidate" ]; then
            grep -E "^\| \*\*Total\*\*|^\| \*Total\*" "$candidate" 2>/dev/null || echo "(No summary row in $candidate)"
            exit 0
        fi
    done
    echo "(No PROOF-STATUS file found at root or docs/status/)"
