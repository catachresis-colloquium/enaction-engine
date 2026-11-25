// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use enaction_accelerator::{
    ACCELERATOR_CONTRACT_VERSION, Backend, Determinism, ExecutionLane, F32BinaryKernelBuffers,
    F32KernelBuffers, FallbackPolicy, KernelBuffers, KernelRequest, Layout, Operation,
    ScalarReferenceBackend, SupportLevel,
};
use enaction_accelerator_native::ZigScalarBackend;
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

fn execute_f32(backend: &dyn Backend, operation: Operation, input: &[f32]) -> Vec<u32> {
    let request = KernelRequest {
        operation,
        version: ACCELERATOR_CONTRACT_VERSION,
        layout: Layout::Vector { len: input.len() },
        lane: ExecutionLane::Advisory,
        minimum_support: SupportLevel::Resilient,
        minimum_determinism: Determinism::ToleranceBounded,
        fallback: FallbackPolicy::PreferAccelerated,
        named_backend: None,
    };
    let mut output = vec![91.0; input.len()];
    backend
        .execute_f32(
            &request,
            F32KernelBuffers {
                input,
                output: &mut output,
            },
        )
        .expect("f32 fixture executes");
    output.into_iter().map(f32::to_bits).collect()
}

fn execute_f32_binary(
    backend: &dyn Backend,
    operation: Operation,
    layout: Layout,
    left: &[f32],
    right: &[f32],
) -> Vec<u32> {
    let mut output = vec![
        91.0;
        match layout {
            Layout::MatMul { m, n, .. } => m * n,
            Layout::Vector { len } => len,
            _ => unreachable!(),
        }
    ];
    backend
        .execute_f32_binary(
            &KernelRequest {
                operation,
                version: ACCELERATOR_CONTRACT_VERSION,
                layout,
                lane: ExecutionLane::Advisory,
                minimum_support: SupportLevel::Conformant,
                minimum_determinism: Determinism::ToleranceBounded,
                fallback: FallbackPolicy::PreferAccelerated,
                named_backend: None,
            },
            F32BinaryKernelBuffers {
                left,
                right,
                output: &mut output,
            },
        )
        .expect("f32 binary fixture executes");
    output.into_iter().map(f32::to_bits).collect()
}

fn request(operation: Operation, layout: Layout) -> KernelRequest<'static> {
    KernelRequest {
        operation,
        version: ACCELERATOR_CONTRACT_VERSION,
        layout,
        lane: ExecutionLane::Authoritative,
        minimum_support: SupportLevel::Conformant,
        minimum_determinism: Determinism::CanonicalExact,
        fallback: FallbackPolicy::PreferAccelerated,
        named_backend: None,
    }
}

fn execute(
    backend: &dyn Backend,
    request: &KernelRequest<'_>,
    left: &[i32],
    right: &[i32],
    output_len: usize,
) -> Vec<i64> {
    let mut output = vec![0_i64; output_len];
    backend
        .execute(
            request,
            KernelBuffers {
                left,
                right,
                output: &mut output,
            },
        )
        .expect("fixture executes");
    output
}

#[test]
fn rust_and_zig_match_v1_canonical_fixtures_byte_for_byte() {
    let root = fixture_root();
    let rust = ScalarReferenceBackend;
    let zig = ZigScalarBackend;

    let dot = load(root.join("cases/fixed-i32-dot.json"));
    let dot_request = request(
        Operation::FixedI32Dot,
        Layout::Dot {
            len: usize::try_from(dot["layout"]["len"].as_u64().expect("len")).expect("usize"),
        },
    );
    let dot_left = numbers(&dot, "left");
    let dot_right = numbers(&dot, "right");
    let rust_dot = execute(&rust, &dot_request, &dot_left, &dot_right, 1);
    let zig_dot = execute(&zig, &dot_request, &dot_left, &dot_right, 1);
    assert_eq!(zig_dot, rust_dot);
    assert_eq!(
        zig_dot,
        expected(&load(root.join("expected/fixed-i32-dot.json")))
    );
    assert_eq!(
        zig_dot[0].to_le_bytes(),
        rust_dot[0].to_le_bytes(),
        "canonical serialized scalar bytes differ"
    );

    let matrix = load(root.join("cases/fixed-i32-matmul.json"));
    let dimension = |name: &str| {
        usize::try_from(matrix["layout"][name].as_u64().expect("dimension")).expect("usize")
    };
    let matrix_request = request(
        Operation::FixedI32MatMul,
        Layout::MatMul {
            m: dimension("m"),
            k: dimension("k"),
            n: dimension("n"),
        },
    );
    let matrix_left = numbers(&matrix, "left");
    let matrix_right = numbers(&matrix, "right");
    let rust_matrix = execute(&rust, &matrix_request, &matrix_left, &matrix_right, 4);
    let zig_matrix = execute(&zig, &matrix_request, &matrix_left, &matrix_right, 4);
    assert_eq!(zig_matrix, rust_matrix);
    assert_eq!(
        zig_matrix,
        expected(&load(root.join("expected/fixed-i32-matmul.json")))
    );
    assert_eq!(
        zig_matrix
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>(),
        rust_matrix
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>(),
        "canonical serialized matrix bytes differ"
    );

    for (operation, name) in [
        (Operation::TensorF32Relu, "tensor-f32-relu"),
        (Operation::TensorF32Relu6, "tensor-f32-relu6"),
    ] {
        let case = load(root.join(format!("cases/{name}.json")));
        let input = f32_bits(&case, "input_bits")
            .into_iter()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let rust_bits = execute_f32(&rust, operation, &input);
        let zig_bits = execute_f32(&zig, operation, &input);
        assert_eq!(zig_bits, rust_bits);
        assert_eq!(
            zig_bits,
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
        let left = f32_bits(&case, "left_bits")
            .into_iter()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let right = f32_bits(&case, "right_bits")
            .into_iter()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let rust_bits = execute_f32_binary(
            &rust,
            operation,
            Layout::Vector { len: left.len() },
            &left,
            &right,
        );
        let zig_bits = execute_f32_binary(
            &zig,
            operation,
            Layout::Vector { len: left.len() },
            &left,
            &right,
        );
        assert_eq!(zig_bits, rust_bits);
        assert_eq!(
            zig_bits,
            f32_bits(
                &load(root.join(format!("expected/{name}.json"))),
                "output_bits"
            )
        );
    }
    let case = load(root.join("cases/tensor-f32-matmul.json"));
    let dimension = |name: &str| {
        usize::try_from(case["layout"][name].as_u64().expect("dimension")).expect("usize")
    };
    let left = f32_bits(&case, "left_bits")
        .into_iter()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    let right = f32_bits(&case, "right_bits")
        .into_iter()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    let layout = Layout::MatMul {
        m: dimension("m"),
        k: dimension("k"),
        n: dimension("n"),
    };
    let rust_bits = execute_f32_binary(&rust, Operation::TensorF32MatMul, layout, &left, &right);
    let zig_bits = execute_f32_binary(&zig, Operation::TensorF32MatMul, layout, &left, &right);
    assert_eq!(zig_bits, rust_bits);
    assert_eq!(
        zig_bits,
        f32_bits(
            &load(root.join("expected/tensor-f32-matmul.json")),
            "output_bits"
        )
    );
}

fn canonical_zig_bytes() -> String {
    let root = fixture_root();
    let zig = ZigScalarBackend;
    let dot = load(root.join("cases/fixed-i32-dot.json"));
    let dot_request = request(
        Operation::FixedI32Dot,
        Layout::Dot {
            len: usize::try_from(dot["layout"]["len"].as_u64().expect("len")).expect("usize"),
        },
    );
    let dot_output = execute(
        &zig,
        &dot_request,
        &numbers(&dot, "left"),
        &numbers(&dot, "right"),
        1,
    );
    let matrix = load(root.join("cases/fixed-i32-matmul.json"));
    let dimension = |name: &str| {
        usize::try_from(matrix["layout"][name].as_u64().expect("dimension")).expect("usize")
    };
    let matrix_output = execute(
        &zig,
        &request(
            Operation::FixedI32MatMul,
            Layout::MatMul {
                m: dimension("m"),
                k: dimension("k"),
                n: dimension("n"),
            },
        ),
        &numbers(&matrix, "left"),
        &numbers(&matrix, "right"),
        4,
    );
    let relu = load(root.join("cases/tensor-f32-relu.json"));
    let relu_input = f32_bits(&relu, "input_bits")
        .into_iter()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    let relu_output = execute_f32(&zig, Operation::TensorF32Relu, &relu_input);
    let relu6_output = execute_f32(&zig, Operation::TensorF32Relu6, &relu_input);
    let fixed_bytes = dot_output
        .iter()
        .chain(&matrix_output)
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    fixed_bytes
        .into_iter()
        .chain(
            relu_output
                .into_iter()
                .chain(relu6_output)
                .flat_map(u32::to_le_bytes),
        )
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn fresh_process_worker_emits_canonical_bytes() {
    if std::env::var_os("ENACTION_ACCELERATOR_FRESH_PROCESS_WORKER").is_some() {
        println!("ENACTION_CANONICAL={}", canonical_zig_bytes());
    }
}

#[test]
fn zig_canonical_bytes_repeat_across_fresh_processes() {
    let executable = std::env::current_exe().expect("current test executable");
    let mut observations = Vec::new();
    for _ in 0..3 {
        let child = Command::new(&executable)
            .args([
                "--exact",
                "fresh_process_worker_emits_canonical_bytes",
                "--nocapture",
            ])
            .env("ENACTION_ACCELERATOR_FRESH_PROCESS_WORKER", "1")
            .output()
            .expect("launch fresh conformance process");
        assert!(
            child.status.success(),
            "fresh process failed: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        let stdout = String::from_utf8(child.stdout).expect("UTF-8 test output");
        let observation = stdout
            .lines()
            .find_map(|line| line.strip_prefix("ENACTION_CANONICAL="))
            .expect("canonical byte marker")
            .to_owned();
        observations.push(observation);
    }
    assert!(observations.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(observations[0], canonical_zig_bytes());
}
