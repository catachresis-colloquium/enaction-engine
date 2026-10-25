// SPDX-License-Identifier: AGPL-3.0-or-later
//! Synthetic advisory matmul measurement, shared by two existing-crate examples.
//! No new kernel, public contract, native layout, or simulation dependency.

use enaction_accelerator::{
    ACCELERATOR_CONTRACT_VERSION, Backend, Determinism, ExecutionEvidence, ExecutionLane,
    F32BinaryKernelBuffers, FallbackPolicy, KernelRequest, Layout, Operation, Registry,
    ScalarReferenceBackend, SupportLevel,
};
use serde_json::{Value, json};
use std::hint::black_box;
use std::io::{self, Write};
use std::time::Instant;

const SHAPES: [(usize, usize, usize); 4] = [(2, 3, 2), (1, 64, 1), (32, 64, 16), (64, 64, 64)];
const SEED: u64 = 0x4d31_2026;
const WARMUP: usize = 10;
const PHASES: [&str; 7] = [
    "allocation_zero_init_ns",
    "generation_ns",
    "conversion_ns",
    "packing_ns",
    "planning_ns",
    "compute_call_ns",
    "output_copy_ns",
];
type Shape = (usize, usize, usize);
type Error = Box<dyn std::error::Error>;

/// Build an advisory f32 matmul request for `(m, k, n)`, requiring the named
/// backend, resilient support, and at least tolerance-bounded determinism.
fn request(shape: Shape, backend: &dyn Backend) -> KernelRequest<'static> {
    KernelRequest {
        operation: Operation::TensorF32MatMul,
        version: ACCELERATOR_CONTRACT_VERSION,
        layout: Layout::MatMul {
            m: shape.0,
            k: shape.1,
            n: shape.2,
        },
        lane: ExecutionLane::Advisory,
        minimum_support: SupportLevel::Resilient,
        minimum_determinism: Determinism::ToleranceBounded,
        fallback: FallbackPolicy::RequireNamedBackend,
        named_backend: Some(backend.descriptor().id),
    }
}

/// Fill `values` deterministically in `[-1000/997, 1000/997]`, advancing `state`
/// once per element so subsequent calls can continue the same sequence.
/// Division deliberately makes most source f64 values inexact in f32.
fn generate(values: &mut [f64], state: &mut u64) {
    for value in values {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *value = (((*state >> 32) % 2001) as i32 - 1000) as f64 / 997.0;
    }
}

/// Copy a `k`-by-`n` matrix from column-major to row-major storage.
/// Both slices must hold at least `k * n` elements; zero extents write nothing.
fn pack_right(k: usize, n: usize, column_major: &[f32], row_major: &mut [f32]) {
    for row in 0..k {
        for column in 0..n {
            row_major[row * n + column] = column_major[column * k + row];
        }
    }
}

struct Run {
    source_left: Vec<f64>,
    source_right: Vec<f64>,
    left: Vec<f32>,
    right: Vec<f32>,
    result: Vec<f32>,
    phases: [u64; 7],
    total_ns: u64,
    payload_capacity_bytes: usize,
    evidence: ExecutionEvidence,
}

/// Return elapsed nanoseconds, panicking if the duration exceeds `u64::MAX`.
fn nanos(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).expect("measurement fits u64")
}

/// Generate, convert, and pack seeded inputs, then plan, execute, and copy a
/// matrix result. Return operands, the copied result, execution evidence, live
/// buffer capacity in bytes, and timings in nanoseconds.
///
/// With `profile`, record seven phase durations; otherwise leave them zero.
/// Total latency covers allocation through output copy, excluding validation
/// and buffer destruction. Both modes retain seven buffers through the copy.
/// Return an error for a non-matrix layout and propagate planning and execution
/// errors without retrying.
fn pipeline(
    registry: &Registry<'_>,
    req: &KernelRequest<'_>,
    seed: u64,
    profile: bool,
) -> Result<Run, Error> {
    let Layout::MatMul { m, k, n } = req.layout else {
        return Err("matrix required".into());
    };
    let total_start = Instant::now();
    let mut mark = total_start;
    let mut phases = [0; 7];
    let mut phase = |slot: usize| {
        if profile {
            phases[slot] = nanos(mark);
            mark = Instant::now();
        }
    };
    let mut source_left = vec![0.0_f64; m * k];
    let mut source_right = vec![0.0_f64; k * n];
    let mut left = vec![0.0_f32; m * k];
    let mut right_column = vec![0.0_f32; k * n];
    let mut right = vec![0.0_f32; k * n];
    let mut output = vec![0.0_f32; m * n];
    let mut result = vec![0.0_f32; m * n];
    // Make allocation/zero-init observable even when later loops overwrite it.
    black_box((
        &mut source_left,
        &mut source_right,
        &mut left,
        &mut right_column,
        &mut right,
        &mut output,
        &mut result,
    ));
    phase(0);
    let mut state = seed;
    generate(&mut source_left, &mut state);
    generate(&mut source_right, &mut state);
    black_box((&source_left, &source_right));
    phase(1);
    for (dest, &src) in left.iter_mut().zip(&source_left) {
        *dest = src as f32;
    }
    for (dest, &src) in right_column.iter_mut().zip(&source_right) {
        *dest = src as f32;
    }
    black_box((&left, &right_column));
    phase(2);
    pack_right(k, n, &right_column, &mut right);
    black_box(&right);
    phase(3);
    let planned = registry.plan(black_box(req))?;
    phase(4);
    let evidence = planned.execute_f32_binary(
        black_box(req),
        F32BinaryKernelBuffers {
            left: black_box(&left),
            right: black_box(&right),
            output: black_box(&mut output),
        },
    )?;
    black_box(&output);
    phase(5);
    result.copy_from_slice(black_box(&output));
    black_box(&result);
    phase(6);
    let total_ns = nanos(total_start);
    // Actual Vec capacity accounting, not an allocator/RSS estimate. The runner
    // also records whole-process peak RSS, including the untimed validator.
    let payload_capacity_bytes = 8 * (source_left.capacity() + source_right.capacity())
        + 4 * (left.capacity()
            + right_column.capacity()
            + right.capacity()
            + output.capacity()
            + result.capacity());
    Ok(Run {
        source_left,
        source_right,
        left,
        right,
        result,
        phases,
        total_ns,
        payload_capacity_bytes,
        evidence,
    })
}

/// Check execution evidence and numerical error using independent f64 arithmetic
/// over the quantized operands. `shape` must match the run's buffer dimensions.
/// Return JSON error maxima, a checksum, and Rust-reference bit mismatch counts;
/// bit mismatches alone do not fail validation. Relative error uses a `1e-12`
/// denominator floor, and conversion error is measured against the f64 inputs.
///
/// Return an error for unexpected evidence, non-finite results, or error above
/// `2 * gamma_k * sum(abs(products)) + 1e-12`, where
/// `gamma_k = k * f32::EPSILON / (1 - k * f32::EPSILON)`.
/// Propagate errors from the scalar reference execution. This numerical oracle
/// is not an exact real-arithmetic proof.
fn validate(run: &Run, shape: Shape, backend: &dyn Backend) -> Result<Value, Error> {
    let (m, k, n) = shape;
    if run.evidence.backend_id != backend.descriptor().id
        || run.evidence.backend_version != backend.descriptor().implementation_version
        || run.evidence.operation != Operation::TensorF32MatMul
        || run.evidence.operation_version != ACCELERATOR_CONTRACT_VERSION
        || run.evidence.support < SupportLevel::Resilient
        || run.evidence.determinism != Determinism::ToleranceBounded
    {
        return Err("unexpected execution evidence; no fallback permitted".into());
    }
    let mut rust_output = vec![0.0; m * n];
    ScalarReferenceBackend.execute_f32_binary(
        &request(shape, &ScalarReferenceBackend),
        F32BinaryKernelBuffers {
            left: &run.left,
            right: &run.right,
            output: &mut rust_output,
        },
    )?;
    let mut max_abs = 0.0_f64;
    let mut max_relative = 0.0_f64;
    let mut max_conversion = 0.0_f64;
    let mut bit_mismatches = 0;
    let mut checksum = 0xcbf29ce484222325_u64;
    let gamma = (k as f64 * f64::from(f32::EPSILON)) / (1.0 - k as f64 * f64::from(f32::EPSILON));
    for (&source, &converted) in run.source_left.iter().zip(&run.left) {
        max_conversion = max_conversion.max((source - f64::from(converted)).abs());
    }
    for inner in 0..k {
        for col in 0..n {
            max_conversion = max_conversion.max(
                (run.source_right[col * k + inner] - f64::from(run.right[inner * n + col])).abs(),
            );
        }
    }
    for row in 0..m {
        for col in 0..n {
            let mut oracle = 0.0_f64;
            let mut sum_abs = 0.0_f64;
            for inner in 0..k {
                // Independent host-layout indexing also checks the packing.
                let term = f64::from(run.source_left[row * k + inner] as f32)
                    * f64::from(run.source_right[col * k + inner] as f32);
                oracle += term;
                sum_abs += term.abs();
            }
            let index = row * n + col;
            let actual = run.result[index];
            let error = (f64::from(actual) - oracle).abs();
            if !actual.is_finite() || error > 2.0 * gamma * sum_abs + 1e-12 {
                return Err(format!("numerical check failed at {row},{col}: error {error}").into());
            }
            max_abs = max_abs.max(error);
            max_relative = max_relative.max(error / oracle.abs().max(1e-12));
            bit_mismatches += usize::from(actual.to_bits() != rust_output[index].to_bits());
            for byte in actual.to_bits().to_le_bytes() {
                checksum = (checksum ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
    }
    Ok(
        json!({"max_abs_error_f64": max_abs, "max_relative_error_f64_floor_1e_12": max_relative,
        "max_input_conversion_abs_error": max_conversion, "rust_bit_mismatches": bit_mismatches,
        "output_fnv1a64": format!("{checksum:016x}")}),
    )
}

/// Return JSON min, max, mean, and nearest-rank p50/p95 in the input's units.
/// Panics if `values` is empty.
fn distribution(values: &[u64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    // Nearest-rank percentiles, with ceil(p*N)-1 indexing.
    let percentile = |p: usize| sorted[(p * sorted.len()).div_ceil(100).saturating_sub(1)];
    json!({"min": sorted[0], "p50": percentile(50), "p95": percentile(95),
        "max": sorted[sorted.len()-1], "mean": values.iter().map(|&v| v as f64).sum::<f64>() / values.len() as f64})
}

/// Run the four fixed matmul shapes using only `backend` and write metadata,
/// samples, summaries, and memory statistics as JSON Lines to stdout.
/// Read an optional sample count from process arguments (default 101, range
/// `1..=10000`), with ten warmups per shape and two timing passes per sample.
/// Report process peak RSS in KiB on Linux and null on other operating systems.
///
/// Propagate argument parsing, registration, planning, execution, validation,
/// stdout I/O, and Linux RSS read/parse errors. Out-of-range counts and extra
/// arguments return a usage error. Failure can leave partial stdout output.
pub fn run(backend: &dyn Backend) -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let samples: usize = args.next().as_deref().unwrap_or("101").parse()?;
    if !(1..=10000).contains(&samples) || args.next().is_some() {
        return Err("usage: m1_matmul[_native] [samples: 1..10000]".into());
    }
    let mut registry = Registry::new();
    registry.register(backend)?;
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    writeln!(
        out,
        "{}",
        json!({"kind":"metadata", "experiment":"m1-matmul-v1", "synthetic":true,
        "backend":backend.descriptor().id, "backend_version":format!("{:?}", backend.descriptor().implementation_version),
        "operation":Operation::TensorF32MatMul.id(), "contract":"1.0", "lane":"advisory",
        "fallback":"RequireNamedBackend", "descriptor":format!("{:?}",backend.descriptor()),
        "samples":samples, "warmup":WARMUP, "seed":SEED, "shapes":SHAPES})
    )?;
    for shape @ (m, k, n) in SHAPES {
        let req = request(shape, backend);
        for _ in 0..WARMUP {
            validate(&pipeline(&registry, &req, SEED, false)?, shape, backend)?;
        }
        let mut totals = Vec::with_capacity(samples);
        let mut phase_values: [Vec<u64>; 7] = std::array::from_fn(|_| Vec::with_capacity(samples));
        for sample in 0..samples {
            let seed = SEED.wrapping_add(sample as u64);
            // Alternate pass order to reduce systematic warm-cache/order bias.
            let mut record =
                json!({"kind":"sample", "shape":[m,k,n], "sample":sample, "seed":seed});
            for profile in if sample % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                let measured = pipeline(&registry, &req, seed, profile)?;
                let error = validate(&measured, shape, backend)?;
                if profile {
                    for (i, name) in PHASES.iter().enumerate() {
                        record[*name] = json!(measured.phases[i]);
                        phase_values[i].push(measured.phases[i]);
                    }
                    record["profiled_total_ns"] = json!(measured.total_ns);
                    record["profiled_error"] = error;
                } else {
                    totals.push(measured.total_ns);
                    record["total_ns"] = json!(measured.total_ns);
                    record["payload_capacity_bytes"] = json!(measured.payload_capacity_bytes);
                    record["error"] = error;
                }
                // Deallocation and validation occur outside all latency timers.
            }
            writeln!(out, "{record}")?;
        }
        let mut summary = json!({"kind":"summary", "backend":backend.descriptor().id, "shape":[m,k,n], "samples":samples,
            "total_ns": distribution(&totals), "total_matrices_per_second": samples as f64 * 1e9 / totals.iter().sum::<u64>() as f64,
            "semantic_gflops_total": (2*m*k*n*samples) as f64 / totals.iter().sum::<u64>() as f64,
            "semantic_gflops_compute": (2*m*k*n*samples) as f64 / phase_values[5].iter().sum::<u64>() as f64});
        for (i, name) in PHASES.iter().enumerate() {
            summary[*name] = distribution(&phase_values[i]);
        }
        writeln!(out, "{summary}")?;
    }
    // Linux reports a real process high-water mark, including code, allocator,
    // validation and serialization. It is not a per-kernel allocation count.
    let peak_rss_kib: Option<u64> = if cfg!(target_os = "linux") {
        let status = std::fs::read_to_string("/proc/self/status")?;
        Some(
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmHWM:"))
                .ok_or("VmHWM unavailable")?
                .split_whitespace()
                .next()
                .ok_or("VmHWM value unavailable")?
                .parse()?,
        )
    } else {
        None
    };
    writeln!(
        out,
        "{}",
        json!({"kind":"memory", "backend":backend.descriptor().id,
        "process_peak_rss_kib":peak_rss_kib, "method":"Linux /proc/self/status VmHWM; null on other OS"})
    )?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
pub fn contract_checks(backend: &dyn Backend) {
    let mut registry = Registry::new();
    registry.register(backend).unwrap();
    for shape in SHAPES
        .into_iter()
        .chain([(0, 3, 2), (2, 0, 3), (2, 3, 0), (3, 5, 7)])
    {
        let run = pipeline(&registry, &request(shape, backend), SEED, false).unwrap();
        validate(&run, shape, backend).unwrap();
        let (m, k, n) = shape;
        assert_eq!(
            run.payload_capacity_bytes,
            12 * m * k + 16 * k * n + 8 * m * n
        );
    }
    let mut authoritative = request((1, 1, 1), backend);
    authoritative.lane = ExecutionLane::Authoritative;
    assert!(registry.plan(&authoritative).is_err());
    let mut missing = request((1, 1, 1), backend);
    missing.named_backend = Some("missing.backend");
    assert!(registry.plan(&missing).is_err());
    for shape in [(usize::MAX, 2, 2), (2, 3, 2)] {
        let req = request(shape, backend);
        let mut output = [91.0];
        assert!(
            registry
                .plan(&req)
                .unwrap()
                .execute_f32_binary(
                    &req,
                    F32BinaryKernelBuffers {
                        left: &[1.0],
                        right: &[1.0],
                        output: &mut output,
                    }
                )
                .is_err()
        );
        assert_eq!(output, [91.0]);
    }
    for (left, right) in [(f32::INFINITY, 1.0), (f32::MAX, 2.0)] {
        let req = request((1, 1, 1), backend);
        let mut output = [91.0];
        assert!(
            registry
                .plan(&req)
                .unwrap()
                .execute_f32_binary(
                    &req,
                    F32BinaryKernelBuffers {
                        left: &[left],
                        right: &[right],
                        output: &mut output,
                    }
                )
                .is_err()
        );
        assert_eq!(output, [91.0], "failure must leave output unchanged");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enaction_accelerator::{
        AcceleratorError, BackendDescriptor, Capability, ContractVersion, DeviceClass,
        KernelBuffers,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn reference_run(shape: Shape, seed: u64, profile: bool) -> Run {
        let mut registry = Registry::new();
        registry.register(&ScalarReferenceBackend).unwrap();
        pipeline(
            &registry,
            &request(shape, &ScalarReferenceBackend),
            seed,
            profile,
        )
        .unwrap()
    }

    fn exact_run() -> Run {
        let mut run = reference_run((1, 1, 1), SEED, false);
        run.source_left[0] = 2.0;
        run.source_right[0] = 3.0;
        run.left[0] = 2.0;
        run.right[0] = 3.0;
        run.result[0] = 6.0;
        run
    }

    #[test]
    fn request_pins_the_advisory_operation_and_backend() {
        let req = request((3, 5, 7), &ScalarReferenceBackend);
        assert_eq!(req.operation, Operation::TensorF32MatMul);
        assert_eq!(req.version, ACCELERATOR_CONTRACT_VERSION);
        assert_eq!(req.layout, Layout::MatMul { m: 3, k: 5, n: 7 });
        assert_eq!(req.lane, ExecutionLane::Advisory);
        assert_eq!(req.minimum_support, SupportLevel::Resilient);
        assert_eq!(req.minimum_determinism, Determinism::ToleranceBounded);
        assert_eq!(req.fallback, FallbackPolicy::RequireNamedBackend);
        assert_eq!(
            req.named_backend,
            Some(ScalarReferenceBackend.descriptor().id)
        );
    }

    #[test]
    fn generator_matches_the_recorded_seed_sequence() {
        let mut state = SEED;
        let mut values = [0.0; 6];
        generate(&mut values, &mut state);
        // Pin the stream used by recorded experiments, including wraparound.
        assert_eq!(
            values,
            [345.0, 684.0, -106.0, -847.0, 678.0, -934.0].map(|v| v / 997.0)
        );
        assert_eq!(state, 0xfea1_ce6f_82bc_3a10);
    }

    #[test]
    fn generator_continues_across_buffers_and_empty_calls_do_not_advance() {
        let mut state = u64::MAX;
        generate(&mut [], &mut state);
        assert_eq!(state, u64::MAX);
        let mut whole = [0.0; 9];
        let mut whole_state = state;
        generate(&mut whole, &mut whole_state);
        let mut split = [0.0; 9];
        generate(&mut split[..4], &mut state);
        generate(&mut [], &mut state);
        generate(&mut split[4..], &mut state);
        assert_eq!(split, whole);
        assert_eq!(state, whole_state);
    }

    #[test]
    fn packing_handles_single_extents_and_preserves_unused_capacity() {
        for (k, n, expected) in [
            (1, 4, [1.0, 2.0, 3.0, 4.0]),
            (4, 1, [1.0, 2.0, 3.0, 4.0]),
            (2, 2, [1.0, 3.0, 2.0, 4.0]),
        ] {
            let source = [1.0, 2.0, 3.0, 4.0, 99.0];
            let mut packed = [91.0; 6];
            pack_right(k, n, &source, &mut packed);
            assert_eq!(&packed[..4], &expected, "shape {k}x{n}");
            assert_eq!(&packed[4..], &[91.0; 2]);
        }
    }

    #[test]
    fn packing_zero_extents_never_accesses_either_slice() {
        for (k, n) in [(0, 0), (0, 3), (3, 0)] {
            let mut packed = [91.0; 2];
            pack_right(k, n, &[], &mut packed);
            assert_eq!(packed, [91.0; 2]);
            pack_right(k, n, &[], &mut []);
        }
    }

    #[test]
    fn pipeline_uses_one_continuous_stream_for_both_operands() {
        let run = reference_run((1, 3, 1), SEED, false);
        assert_eq!(run.source_left, [345.0, 684.0, -106.0].map(|v| v / 997.0));
        assert_eq!(run.source_right, [-847.0, 678.0, -934.0].map(|v| v / 997.0));
        assert_eq!(
            run.left,
            run.source_left
                .iter()
                .map(|&v| v as f32)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            run.right,
            run.source_right
                .iter()
                .map(|&v| v as f32)
                .collect::<Vec<_>>()
        );
        validate(&run, (1, 3, 1), &ScalarReferenceBackend).unwrap();
    }

    #[test]
    fn profiling_preserves_operands_results_and_execution_evidence() {
        let shape = (3, 5, 7);
        let plain = reference_run(shape, SEED, false);
        let profiled = reference_run(shape, SEED, true);
        assert_eq!(plain.phases, [0; 7]);
        // Clock intervals are disjoint; no wall-clock performance threshold.
        assert!(profiled.phases.iter().sum::<u64>() <= profiled.total_ns);
        assert_eq!(plain.source_left, profiled.source_left);
        assert_eq!(plain.source_right, profiled.source_right);
        assert_eq!(plain.left, profiled.left);
        assert_eq!(plain.right, profiled.right);
        assert_eq!(plain.result, profiled.result);
        assert_eq!(plain.evidence, profiled.evidence);
        assert_eq!(
            plain.payload_capacity_bytes,
            profiled.payload_capacity_bytes
        );
        assert_eq!(
            validate(&plain, shape, &ScalarReferenceBackend).unwrap(),
            validate(&profiled, shape, &ScalarReferenceBackend).unwrap()
        );
        let next_seed = reference_run(shape, SEED + 1, false);
        assert_ne!(plain.source_left, next_seed.source_left);
        assert_ne!(plain.result, next_seed.result);
    }

    #[test]
    fn pipeline_zero_inner_extent_returns_a_full_zero_matrix() {
        let run = reference_run((2, 0, 3), SEED, false);
        assert!(run.source_left.is_empty());
        assert!(run.source_right.is_empty());
        assert!(run.left.is_empty());
        assert!(run.right.is_empty());
        assert_eq!(run.result, [0.0; 6]);
        assert_eq!(run.payload_capacity_bytes, 48);
        let report = validate(&run, (2, 0, 3), &ScalarReferenceBackend).unwrap();
        assert_eq!(report["max_abs_error_f64"], 0.0);
        assert_eq!(report["max_relative_error_f64_floor_1e_12"], 0.0);
        assert_eq!(report["max_input_conversion_abs_error"], 0.0);
        assert_eq!(report["rust_bit_mismatches"], 0);
    }

    #[test]
    fn pipeline_rejects_non_matrix_layouts_before_planning() {
        for layout in [Layout::Dot { len: 3 }, Layout::Vector { len: 3 }] {
            let mut req = request((1, 1, 1), &ScalarReferenceBackend);
            req.layout = layout;
            let error = pipeline(&Registry::new(), &req, SEED, false).err().unwrap();
            assert_eq!(error.to_string(), "matrix required");
        }
    }

    #[test]
    fn pipeline_propagates_missing_backend_errors() {
        let req = request((1, 1, 1), &ScalarReferenceBackend);
        let error = pipeline(&Registry::new(), &req, SEED, false).err().unwrap();
        assert_eq!(
            error.downcast_ref::<AcceleratorError>(),
            Some(&AcceleratorError::NamedBackendUnavailable(
                ScalarReferenceBackend.descriptor().id.to_owned()
            ))
        );
    }

    struct FailingBackend(AtomicUsize);

    impl Backend for FailingBackend {
        fn descriptor(&self) -> &'static BackendDescriptor {
            static DESCRIPTOR: BackendDescriptor = BackendDescriptor {
                id: "test.m1.failure",
                implementation_version: ACCELERATOR_CONTRACT_VERSION,
                device_class: DeviceClass::Cpu,
                priority: 100,
                is_reference: false,
                is_remote: false,
                capabilities: &[Capability {
                    operation: Operation::TensorF32MatMul,
                    version: ACCELERATOR_CONTRACT_VERSION,
                    support: SupportLevel::Resilient,
                    determinism: Determinism::ToleranceBounded,
                }],
            };
            &DESCRIPTOR
        }

        fn execute(
            &self,
            _request: &KernelRequest<'_>,
            _buffers: KernelBuffers<'_>,
        ) -> Result<ExecutionEvidence, AcceleratorError> {
            panic!("the benchmark must use binary32 buffers")
        }

        fn execute_f32_binary(
            &self,
            _request: &KernelRequest<'_>,
            _buffers: F32BinaryKernelBuffers<'_>,
        ) -> Result<ExecutionEvidence, AcceleratorError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Err(AcceleratorError::BackendFailure {
                backend: self.descriptor().id,
                message: "planted benchmark failure",
            })
        }
    }

    #[test]
    fn pipeline_does_not_retry_or_fall_back_after_execution_failure() {
        let failing = FailingBackend(AtomicUsize::new(0));
        let mut registry = Registry::new();
        registry.register(&ScalarReferenceBackend).unwrap();
        registry.register(&failing).unwrap();
        let req = request((2, 3, 2), &failing);
        let error = pipeline(&registry, &req, SEED, false).err().unwrap();
        assert_eq!(
            error.downcast_ref::<AcceleratorError>(),
            Some(&AcceleratorError::BackendFailure {
                backend: "test.m1.failure",
                message: "planted benchmark failure",
            })
        );
        assert_eq!(failing.0.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn validator_reports_exact_errors_and_little_endian_checksum() {
        let report = validate(&exact_run(), (1, 1, 1), &ScalarReferenceBackend).unwrap();
        assert_eq!(
            report,
            json!({
                "max_abs_error_f64": 0.0,
                "max_relative_error_f64_floor_1e_12": 0.0,
                "max_input_conversion_abs_error": 0.0,
                "rust_bit_mismatches": 0,
                // FNV-1a over 6.0f32's little-endian bytes [0, 0, 192, 64].
                "output_fnv1a64": "4a98b67f9ba34875",
            })
        );
    }

    #[test]
    fn validator_rejects_each_mismatched_evidence_field() {
        let mut run = exact_run();
        let valid = run.evidence;
        let mut invalid = [valid; 7];
        invalid[0].backend_id = "unexpected";
        invalid[1].backend_version = ContractVersion::new(2, 0);
        invalid[2].operation = Operation::TensorF32Add;
        invalid[3].operation_version = ContractVersion::new(1, 1);
        invalid[4].support = SupportLevel::Conformant;
        invalid[5].determinism = Determinism::AdvisoryOnly;
        invalid[6].determinism = Determinism::CanonicalExact;
        for evidence in invalid {
            run.evidence = evidence;
            let error = validate(&run, (1, 1, 1), &ScalarReferenceBackend).unwrap_err();
            assert_eq!(
                error.to_string(),
                "unexpected execution evidence; no fallback permitted",
                "{evidence:?}"
            );
        }
        for support in [SupportLevel::Resilient, SupportLevel::ProductionSupported] {
            run.evidence = ExecutionEvidence { support, ..valid };
            validate(&run, (1, 1, 1), &ScalarReferenceBackend).unwrap();
        }
    }

    #[test]
    fn validator_rejects_nonfinite_results() {
        let mut run = exact_run();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            run.result[0] = value;
            let error = validate(&run, (1, 1, 1), &ScalarReferenceBackend).unwrap_err();
            assert!(error.to_string().contains("numerical check failed at 0,0"));
        }
    }

    #[test]
    fn validator_allows_bit_mismatches_within_the_numerical_bound() {
        let mut run = exact_run();
        run.source_left[0] = 1.0;
        run.source_right[0] = 1.0;
        run.left[0] = 1.0;
        run.right[0] = 1.0;
        // For k=1, two ULPs above 1.0 fit 2*gamma_k; three do not.
        run.result[0] = f32::from_bits(1.0_f32.to_bits() + 2);
        let report = validate(&run, (1, 1, 1), &ScalarReferenceBackend).unwrap();
        assert_eq!(report["rust_bit_mismatches"], 1);
        assert_eq!(report["max_abs_error_f64"], 2.0 * f64::from(f32::EPSILON));
        assert_eq!(
            report["max_relative_error_f64_floor_1e_12"],
            report["max_abs_error_f64"]
        );
        run.result[0] = f32::from_bits(1.0_f32.to_bits() + 3);
        assert!(validate(&run, (1, 1, 1), &ScalarReferenceBackend).is_err());
    }

    #[test]
    fn validator_separates_conversion_error_from_quantized_arithmetic() {
        let mut run = reference_run((1, 1, 2), SEED, false);
        run.source_left = vec![1.0 + 2.0_f64.powi(-25)];
        run.source_right = vec![2.0, 3.0 + 2.0_f64.powi(-24)];
        run.left = vec![1.0];
        run.right = vec![2.0, 3.0];
        run.result = vec![2.0, 3.0];
        let report = validate(&run, (1, 1, 2), &ScalarReferenceBackend).unwrap();
        assert_eq!(report["max_abs_error_f64"], 0.0);
        assert_eq!(report["max_input_conversion_abs_error"], 2.0_f64.powi(-24));
    }

    #[test]
    fn validator_floors_relative_error_when_products_cancel() {
        let mut run = reference_run((1, 2, 1), SEED, false);
        run.source_left = vec![1.0, -1.0];
        run.source_right = vec![1.0, 1.0];
        run.left = vec![1.0, -1.0];
        run.right = vec![1.0, 1.0];
        run.result = vec![1e-14];
        let report = validate(&run, (1, 2, 1), &ScalarReferenceBackend).unwrap();
        let error = f64::from(run.result[0]);
        assert_eq!(report["max_abs_error_f64"], error);
        assert_eq!(report["max_relative_error_f64_floor_1e_12"], error / 1e-12);
        assert_eq!(report["rust_bit_mismatches"], 1);
    }

    #[test]
    fn validator_preserves_signed_zero_in_bit_counts_and_checksum() {
        let mut run = exact_run();
        run.source_left[0] = 0.0;
        run.left[0] = 0.0;
        run.result[0] = 0.0;
        let positive = validate(&run, (1, 1, 1), &ScalarReferenceBackend).unwrap();
        run.result[0] = -0.0;
        let negative = validate(&run, (1, 1, 1), &ScalarReferenceBackend).unwrap();
        assert_eq!(positive["rust_bit_mismatches"], 0);
        assert_eq!(negative["rust_bit_mismatches"], 1);
        assert_eq!(positive["output_fnv1a64"], "4d25767f9dce13f5");
        assert_eq!(negative["output_fnv1a64"], "4d24f67f9dcd3a75");
        assert_eq!(negative["max_abs_error_f64"], 0.0);
    }

    #[test]
    fn independent_oracle_rejects_a_result_computed_from_mispacked_inputs() {
        let shape = (2, 3, 2);
        let mut run = reference_run(shape, SEED, false);
        run.right.swap(0, 1);
        ScalarReferenceBackend
            .execute_f32_binary(
                &request(shape, &ScalarReferenceBackend),
                F32BinaryKernelBuffers {
                    left: &run.left,
                    right: &run.right,
                    output: &mut run.result,
                },
            )
            .unwrap();
        // Rust bit comparison alone would accept this incorrect packing.
        let error = validate(&run, shape, &ScalarReferenceBackend).unwrap_err();
        assert!(error.to_string().contains("numerical check failed"));
    }

    #[test]
    fn validator_checks_every_cell_of_a_rectangular_result() {
        let shape = (2, 3, 2);
        let mut run = reference_run(shape, SEED, false);
        run.source_left = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        // Host B is column-major; the compute operand is row-major.
        run.source_right = vec![7.0, 9.0, 11.0, 8.0, 10.0, 12.0];
        run.left = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        run.right = vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let expected = [58.0, 64.0, 139.0, 154.0];
        run.result.copy_from_slice(&expected);
        let report = validate(&run, shape, &ScalarReferenceBackend).unwrap();
        assert_eq!(report["max_abs_error_f64"], 0.0);
        assert_eq!(report["rust_bit_mismatches"], 0);

        for index in 0..expected.len() {
            run.result.copy_from_slice(&expected);
            run.result[index] += 1.0;
            let error = validate(&run, shape, &ScalarReferenceBackend).unwrap_err();
            assert!(error.to_string().starts_with(&format!(
                "numerical check failed at {},{}:",
                index / shape.2,
                index % shape.2
            )));
        }
    }

    #[test]
    fn validator_maps_conversion_error_through_rectangular_packing() {
        let shape = (2, 3, 2);
        let mut run = reference_run(shape, SEED, false);
        run.source_left.fill(1.0);
        let source = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        run.source_right.copy_from_slice(&source);
        run.left.fill(1.0);
        run.right.copy_from_slice(&[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
        run.result.copy_from_slice(&[6.0, 15.0, 6.0, 15.0]);
        // Every source position must contribute to the maximum, using the
        // column-major index even at positions that move during packing.
        for index in 0..run.source_right.len() {
            run.source_right.copy_from_slice(&source);
            run.source_right[index] += 2.0_f64.powi(-25);
            let report = validate(&run, shape, &ScalarReferenceBackend).unwrap();
            assert_eq!(report["max_abs_error_f64"], 0.0);
            assert_eq!(report["max_input_conversion_abs_error"], 2.0_f64.powi(-25));
        }
    }

    #[test]
    fn validator_zero_output_extents_keep_the_empty_checksum() {
        for shape in [(0, 3, 2), (2, 3, 0), (0, 0, 0)] {
            let run = reference_run(shape, SEED, false);
            assert!(run.result.is_empty());
            let report = validate(&run, shape, &ScalarReferenceBackend).unwrap();
            assert_eq!(report["output_fnv1a64"], "cbf29ce484222325");
            assert_eq!(report["max_abs_error_f64"], 0.0);
            assert_eq!(report["max_relative_error_f64_floor_1e_12"], 0.0);
            assert_eq!(report["rust_bit_mismatches"], 0);
        }
    }

    #[test]
    fn validator_enforces_the_absolute_error_floor_at_adjacent_f32_values() {
        let shape = (1, 0, 1);
        let mut run = reference_run(shape, SEED, false);
        // With no products, gamma contributes nothing. These adjacent f32
        // values lie on opposite sides of the documented f64 1e-12 floor.
        let below = 1e-12_f32;
        let above = f32::from_bits(below.to_bits() + 1);
        assert!(f64::from(below) < 1e-12);
        assert!(f64::from(above) > 1e-12);
        for sign in [-1.0, 1.0] {
            run.result[0] = sign * below;
            validate(&run, shape, &ScalarReferenceBackend).unwrap();
            run.result[0] = sign * above;
            let error = validate(&run, shape, &ScalarReferenceBackend).unwrap_err();
            assert!(
                error
                    .to_string()
                    .starts_with("numerical check failed at 0,0:")
            );
        }
    }

    #[test]
    fn distribution_reports_all_statistics_without_mutating_samples() {
        let values = [9, 1, 4, 2, 4];
        assert_eq!(
            distribution(&values),
            json!({
                "min": 1, "p50": 4, "p95": 9, "max": 9, "mean": 4.0,
            })
        );
        assert_eq!(values, [9, 1, 4, 2, 4]);
        assert_eq!(
            distribution(&[7]),
            json!({
                "min": 7, "p50": 7, "p95": 7, "max": 7, "mean": 7.0,
            })
        );
        assert_eq!(
            distribution(&[0; 3]),
            json!({
                "min": 0, "p50": 0, "p95": 0, "max": 0, "mean": 0.0,
            })
        );
    }

    #[test]
    fn distribution_uses_nearest_rank_at_percentile_boundaries() {
        let twenty: Vec<u64> = (1..=20).rev().collect();
        let twenty_one: Vec<u64> = (1..=21).rev().collect();
        assert_eq!(distribution(&twenty)["p50"], 10);
        assert_eq!(distribution(&twenty)["p95"], 19);
        assert_eq!(distribution(&twenty_one)["p50"], 11);
        assert_eq!(distribution(&twenty_one)["p95"], 20);
    }

    #[test]
    fn distribution_mean_does_not_overflow_integer_accumulators() {
        let report = distribution(&[u64::MAX; 2]);
        assert_eq!(report["min"], u64::MAX);
        assert_eq!(report["max"], u64::MAX);
        assert_eq!(report["mean"].as_f64(), Some(u64::MAX as f64));
    }

    #[test]
    #[should_panic]
    fn distribution_requires_at_least_one_sample() {
        distribution(&[]);
    }

    #[test]
    fn rectangular_packing_has_known_orientation() {
        let mut packed = [0.0; 6];
        pack_right(3, 2, &[7.0, 9.0, 11.0, 8.0, 10.0, 12.0], &mut packed);
        assert_eq!(packed, [7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
    }
    #[test]
    fn validator_rejects_corruption_and_wrong_backend() {
        let backend = ScalarReferenceBackend;
        let mut registry = Registry::new();
        registry.register(&backend).unwrap();
        let mut run = pipeline(&registry, &request((2, 3, 2), &backend), SEED, false).unwrap();
        run.result[0] += 1.0;
        assert!(validate(&run, (2, 3, 2), &backend).is_err());
        let mut valid = pipeline(&registry, &request((2, 3, 2), &backend), SEED, false).unwrap();
        valid.evidence.backend_id = "unexpected";
        assert!(validate(&valid, (2, 3, 2), &backend).is_err());
    }
    #[test]
    fn generator_repeats_and_percentiles_are_nearest_rank() {
        let (mut a, mut b) = ([0.0; 32], [0.0; 32]);
        generate(&mut a, &mut SEED.clone());
        generate(&mut b, &mut SEED.clone());
        assert_eq!(a, b);
        assert!(a.iter().all(|v| v.abs() <= 1000.0 / 997.0));
        assert_eq!(distribution(&[4, 1, 3, 2])["p50"], 2);
        assert_eq!(distribution(&[4, 1, 3, 2])["p95"], 4);
    }
}
