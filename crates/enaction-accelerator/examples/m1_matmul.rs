// SPDX-License-Identifier: AGPL-3.0-or-later
#[path = "../../../verification/benchmarks/m1/experiment.rs"]
mod experiment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    experiment::run(&enaction_accelerator::ScalarReferenceBackend)
}

#[test]
fn benchmark_reference_contract_checks() {
    experiment::contract_checks(&enaction_accelerator::ScalarReferenceBackend);
}
