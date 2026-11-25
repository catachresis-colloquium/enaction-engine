// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::path::{Path, PathBuf};

use enaction_accelerator::{
    ACCELERATOR_CONTRACT_VERSION, Backend, Determinism, ExecutionLane, F32BinaryKernelBuffers,
    F32KernelBuffers, FallbackPolicy, KernelBuffers, KernelRequest, Layout, Operation,
    ScalarReferenceBackend, SupportLevel,
};
use serde_json::Value;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/accelerator/v1")
}

fn load(path: impl AsRef<Path>) -> Value {
    let path = path.as_ref();
    serde_json::from_str(&fs::read_to_string(path).expect("read fixture")).expect("parse fixture")
}

fn numbers(value: &Value, field: &str) -> Vec<i32> {
    value[field]
        .as_array()
        .expect("integer array")
        .iter()
        .map(|number| i32::try_from(number.as_i64().expect("integer")).expect("i32"))
        .collect()
}

fn expected(value: &Value) -> Vec<i64> {
    value["output"]
        .as_array()
        .expect("output array")
        .iter()
        .map(|number| number.as_i64().expect("i64"))
        .collect()
}

fn f32_bits(value: &Value, field: &str) -> Vec<u32> {
    value[field]
        .as_array()
        .expect("bit array")
        .iter()
        .map(|bits| u32::from_str_radix(bits.as_str().expect("hex bits"), 16).expect("u32 bits"))
        .collect()
}

fn request(operation: Operation, layout: Layout) -> KernelRequest<'static> {
    KernelRequest {
        operation,
        version: ACCELERATOR_CONTRACT_VERSION,
        layout,
        lane: ExecutionLane::Authoritative,
        minimum_support: SupportLevel::Deterministic,
        minimum_determinism: Determinism::CanonicalExact,
        fallback: FallbackPolicy::AllowReference,
        named_backend: None,
    }
}

#[test]
fn scalar_reference_matches_v1_cross_language_fixtures() {
    let root = fixture_root();
    let backend = ScalarReferenceBackend;

    let dot = load(root.join("cases/fixed-i32-dot.json"));
    assert_eq!(dot["operation"], Operation::FixedI32Dot.id());
    let dot_left = numbers(&dot, "left");
    let dot_right = numbers(&dot, "right");
    let mut dot_output = vec![0_i64; 1];
    backend
        .execute(
            &request(
                Operation::FixedI32Dot,
                Layout::Dot {
                    len: usize::try_from(dot["layout"]["len"].as_u64().expect("len"))
                        .expect("usize"),
                },
            ),
            KernelBuffers {
                left: &dot_left,
                right: &dot_right,
                output: &mut dot_output,
            },
        )
        .expect("dot fixture executes");
    assert_eq!(
        dot_output,
        expected(&load(root.join("expected/fixed-i32-dot.json")))
    );

    let matrix = load(root.join("cases/fixed-i32-matmul.json"));
    assert_eq!(matrix["operation"], Operation::FixedI32MatMul.id());
    let matrix_left = numbers(&matrix, "left");
    let matrix_right = numbers(&matrix, "right");
    let mut matrix_output = vec![0_i64; 4];
    let dimension = |name: &str| {
        usize::try_from(matrix["layout"][name].as_u64().expect("dimension")).expect("usize")
    };
    backend
        .execute(
            &request(
                Operation::FixedI32MatMul,
                Layout::MatMul {
                    m: dimension("m"),
                    k: dimension("k"),
                    n: dimension("n"),
                },
            ),
            KernelBuffers {
                left: &matrix_left,
                right: &matrix_right,
                output: &mut matrix_output,
            },
        )
        .expect("matrix fixture executes");
    assert_eq!(
        matrix_output,
        expected(&load(root.join("expected/fixed-i32-matmul.json")))
    );

    for (operation, name) in [
        (Operation::TensorF32Relu, "tensor-f32-relu"),
        (Operation::TensorF32Relu6, "tensor-f32-relu6"),
    ] {
        let case = load(root.join(format!("cases/{name}.json")));
        assert_eq!(case["operation"], operation.id());
        let input_bits = f32_bits(&case, "input_bits");
        let input = input_bits
            .iter()
            .copied()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let mut output = vec![91.0; input.len()];
        backend
            .execute_f32(
                &KernelRequest {
                    operation,
                    version: ACCELERATOR_CONTRACT_VERSION,
                    layout: Layout::Vector { len: input.len() },
                    lane: ExecutionLane::Advisory,
                    minimum_support: SupportLevel::Deterministic,
                    minimum_determinism: Determinism::ToleranceBounded,
                    fallback: FallbackPolicy::AllowReference,
                    named_backend: None,
                },
                F32KernelBuffers {
                    input: &input,
                    output: &mut output,
                },
            )
            .expect("pointwise fixture executes");
        assert_eq!(
            output.into_iter().map(f32::to_bits).collect::<Vec<_>>(),
            f32_bits(
                &load(root.join(format!("expected/{name}.json"))),
                "output_bits"
            )
        );
    }

    for (operation, name) in [
        (Operation::TensorF32Add, "tensor-f32-add"),
        (Operation::TensorF32Mul, "tensor-f32-mul"),
    ] {
        let case = load(root.join(format!("cases/{name}.json")));
        assert_eq!(case["operation"], operation.id());
        let left = f32_bits(&case, "left_bits")
            .into_iter()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let right = f32_bits(&case, "right_bits")
            .into_iter()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let mut output = vec![91.0; left.len()];
        backend
            .execute_f32_binary(
                &KernelRequest {
                    operation,
                    version: ACCELERATOR_CONTRACT_VERSION,
                    layout: Layout::Vector { len: left.len() },
                    lane: ExecutionLane::Advisory,
                    minimum_support: SupportLevel::Deterministic,
                    minimum_determinism: Determinism::ToleranceBounded,
                    fallback: FallbackPolicy::AllowReference,
                    named_backend: None,
                },
                F32BinaryKernelBuffers {
                    left: &left,
                    right: &right,
                    output: &mut output,
                },
            )
            .expect("binary fixture executes");
        assert_eq!(
            output.into_iter().map(f32::to_bits).collect::<Vec<_>>(),
            f32_bits(
                &load(root.join(format!("expected/{name}.json"))),
                "output_bits"
            )
        );
    }

    let case = load(root.join("cases/tensor-f32-matmul.json"));
    let left = f32_bits(&case, "left_bits")
        .into_iter()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    let right = f32_bits(&case, "right_bits")
        .into_iter()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    let dimension = |name: &str| {
        usize::try_from(case["layout"][name].as_u64().expect("dimension")).expect("usize")
    };
    let mut output = vec![91.0; dimension("m") * dimension("n")];
    backend
        .execute_f32_binary(
            &KernelRequest {
                operation: Operation::TensorF32MatMul,
                version: ACCELERATOR_CONTRACT_VERSION,
                layout: Layout::MatMul {
                    m: dimension("m"),
                    k: dimension("k"),
                    n: dimension("n"),
                },
                lane: ExecutionLane::Advisory,
                minimum_support: SupportLevel::Deterministic,
                minimum_determinism: Determinism::ToleranceBounded,
                fallback: FallbackPolicy::AllowReference,
                named_backend: None,
            },
            F32BinaryKernelBuffers {
                left: &left,
                right: &right,
                output: &mut output,
            },
        )
        .expect("matmul fixture executes");
    assert_eq!(
        output.into_iter().map(f32::to_bits).collect::<Vec<_>>(),
        f32_bits(
            &load(root.join("expected/tensor-f32-matmul.json")),
            "output_bits"
        )
    );
}
