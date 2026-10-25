// SPDX-License-Identifier: AGPL-3.0-or-later
#[path = "../../../verification/benchmarks/m1/experiment.rs"]
mod experiment;

/// Run the shared matrix benchmark with the Rust scalar reference backend,
/// propagating benchmark errors to the process entry point.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    experiment::run(&enaction_accelerator::ScalarReferenceBackend)
}

#[test]
fn benchmark_reference_contract_checks() {
    experiment::contract_checks(&enaction_accelerator::ScalarReferenceBackend);
}
