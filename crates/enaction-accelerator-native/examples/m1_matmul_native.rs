// SPDX-License-Identifier: AGPL-3.0-or-later
#[path = "../../../verification/benchmarks/m1/experiment.rs"]
mod experiment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        enaction_accelerator_native::native_abi_version(),
        enaction_accelerator::ACCELERATOR_CONTRACT_VERSION
    );
    eprintln!(
        "native_capabilities={:?}",
        enaction_accelerator_native::native_capabilities()?
    );
    experiment::run(&enaction_accelerator_native::ZigScalarBackend)
}

#[test]
fn benchmark_native_contract_checks() {
    experiment::contract_checks(&enaction_accelerator_native::ZigScalarBackend);
}
