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

// Integer state evolution is platform-independent. Division deliberately makes
// most source f64 values inexact in f32, unlike all-integer toy fixtures.
fn generate(values: &mut [f64], state: &mut u64) {
    for value in values {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *value = (((*state >> 32) % 2001) as i32 - 1000) as f64 / 997.0;
    }
}

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

fn nanos(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).expect("measurement fits u64")
}

// The diagnostic pass takes inner timestamps; the total-latency pass does not.
// Both perform identical work and retain all seven buffers through output copy.
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

// Independent f64 arithmetic over the quantized operands. This is a numerical
// oracle, not a replacement backend or an exact real-arithmetic proof.
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

fn distribution(values: &[u64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    // Nearest-rank percentiles, with ceil(p*N)-1 indexing.
    let percentile = |p: usize| sorted[(p * sorted.len()).div_ceil(100).saturating_sub(1)];
    json!({"min": sorted[0], "p50": percentile(50), "p95": percentile(95),
        "max": sorted[sorted.len()-1], "mean": values.iter().map(|&v| v as f64).sum::<f64>() / values.len() as f64})
}

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
