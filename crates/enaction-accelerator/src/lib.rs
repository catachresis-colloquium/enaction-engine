// SPDX-License-Identifier: MPL-2.0
//! Operation-first accelerator capability and exact reference kernels.
//!
//! This crate implements ADR-0020's smallest non-speculative slice: a
//! capability registry, registration-order-independent planner, exact scalar
//! CPU fixed-point kernels, and explicit execution failures. It intentionally
//! contains no placeholder CUDA, TPU, NPU or other hardware implementation.

#![forbid(unsafe_code)]

use core::cmp::Ordering;
use core::fmt;

mod version;
pub use version::ContractVersion;

/// Stable version of the accelerator request contract.
pub const ACCELERATOR_CONTRACT_VERSION: ContractVersion = ContractVersion::new(1, 0);

/// Descriptive device class. A class is not a capability claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeviceClass {
    Cpu,
    Gpu,
    Tpu,
    Npu,
    Dsp,
    Ppu,
    Math,
    Fpga,
    Vpu,
    Qpu,
    Crypto,
}

/// Whether execution can participate directly in deterministic reduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionLane {
    Authoritative,
    Advisory,
    RemoteJob,
}

/// Semantic reproducibility established for an operation/backend pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Determinism {
    AdvisoryOnly,
    ToleranceBounded,
    CanonicalExact,
}

/// Highest evidenced support level for one operation/backend pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupportLevel {
    Declared,
    Discoverable,
    Loadable,
    Runnable,
    Conformant,
    Resilient,
    Deterministic,
    Benchmarked,
    ProductionSupported,
}

/// Planning-time fallback. Runtime kernel failure is never a fallback signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackPolicy {
    RequireNamedBackend,
    RequireDeterministicEquivalent,
    AllowReference,
    PreferAccelerated,
}

/// Operations with implemented reference semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// `sum(left[i] * right[i])`, checked into a signed 64-bit accumulator.
    FixedI32Dot,
    /// Row-major `(m × k) * (k × n)`, checked into signed 64-bit outputs.
    FixedI32MatMul,
    /// Element-wise `max(+0.0, x)` over finite IEEE-754 binary32 values.
    TensorF32Relu,
    /// Element-wise `min(6.0, max(+0.0, x))` over finite binary32 values.
    TensorF32Relu6,
    /// Row-major binary32 matrix multiplication in declared loop order.
    TensorF32MatMul,
    /// Element-wise binary32 addition.
    TensorF32Add,
    /// Element-wise binary32 multiplication.
    TensorF32Mul,
}

impl Operation {
    pub const fn id(self) -> &'static str {
        match self {
            Self::FixedI32Dot => "enaction.fixed.i32.dot",
            Self::FixedI32MatMul => "enaction.fixed.i32.matmul",
            Self::TensorF32Relu => "enaction.tensor.f32.relu",
            Self::TensorF32Relu6 => "enaction.tensor.f32.relu6",
            Self::TensorF32MatMul => "enaction.tensor.f32.matmul",
            Self::TensorF32Add => "enaction.tensor.f32.add",
            Self::TensorF32Mul => "enaction.tensor.f32.mul",
        }
    }
}

/// Shape of an operation. Cardinalities are checked before execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Dot {
        len: usize,
    },
    MatMul {
        m: usize,
        k: usize,
        n: usize,
    },
    /// A contiguous logical vector. Higher-rank consumers flatten in their
    /// declared canonical order before crossing this v1 operation boundary.
    Vector {
        len: usize,
    },
}

/// A request supplied to capability planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelRequest<'a> {
    pub operation: Operation,
    pub version: ContractVersion,
    pub layout: Layout,
    pub lane: ExecutionLane,
    pub minimum_support: SupportLevel,
    pub minimum_determinism: Determinism,
    pub fallback: FallbackPolicy,
    pub named_backend: Option<&'a str>,
}

/// Evidence attached to an operation implemented by a backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub operation: Operation,
    pub version: ContractVersion,
    pub support: SupportLevel,
    pub determinism: Determinism,
}

/// Stable facts about a backend implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendDescriptor {
    pub id: &'static str,
    pub implementation_version: ContractVersion,
    pub device_class: DeviceClass,
    /// Larger values are preferred. The backend id breaks ties stably.
    pub priority: i16,
    pub is_reference: bool,
    pub is_remote: bool,
    pub capabilities: &'static [Capability],
}

impl BackendDescriptor {
    fn capability_for(&self, request: &KernelRequest<'_>) -> Option<&Capability> {
        self.capabilities.iter().find(|capability| {
            capability.operation == request.operation
                && capability.version.accepts(request.version)
                && capability.support >= request.minimum_support
                && capability.determinism >= request.minimum_determinism
                && (request.lane != ExecutionLane::Authoritative
                    || capability.determinism == Determinism::CanonicalExact)
        })
    }
}

/// Caller-owned buffers. Backends do not allocate result storage.
pub struct KernelBuffers<'a> {
    pub left: &'a [i32],
    pub right: &'a [i32],
    pub output: &'a mut [i64],
}

/// Caller-owned binary32 buffers for unary tensor operations. Inputs must be
/// finite. Backends validate the complete input before changing output.
pub struct F32KernelBuffers<'a> {
    pub input: &'a [f32],
    pub output: &'a mut [f32],
}

/// Caller-owned binary32 buffers for two-input tensor operations. Inputs and
/// every intermediate/result must be finite; failure leaves output unchanged.
pub struct F32BinaryKernelBuffers<'a> {
    pub left: &'a [f32],
    pub right: &'a [f32],
    pub output: &'a mut [f32],
}

/// Successful execution evidence. Timing is deliberately absent: it is
/// telemetry, not deterministic state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionEvidence {
    pub backend_id: &'static str,
    pub backend_version: ContractVersion,
    pub operation: Operation,
    pub operation_version: ContractVersion,
    pub determinism: Determinism,
    pub support: SupportLevel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcceleratorError {
    DuplicateBackendId(&'static str),
    NoCompatibleBackend {
        operation: &'static str,
    },
    NamedBackendUnavailable(String),
    LaneMismatch {
        backend: &'static str,
    },
    LayoutMismatch,
    LengthMismatch {
        buffer: &'static str,
        expected: usize,
        actual: usize,
    },
    DimensionOverflow,
    ArithmeticOverflow,
    BackendFailure {
        backend: &'static str,
        message: &'static str,
    },
    NativeAbiFailure {
        backend: &'static str,
        status: u32,
    },
    InvalidExecutionEvidence {
        backend: &'static str,
    },
    UnsupportedBufferDomain {
        backend: &'static str,
    },
    NonFiniteInput {
        index: usize,
    },
}

impl fmt::Display for AcceleratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateBackendId(id) => write!(formatter, "duplicate backend id `{id}`"),
            Self::NoCompatibleBackend { operation } => {
                write!(formatter, "no compatible backend for `{operation}`")
            }
            Self::NamedBackendUnavailable(id) => {
                write!(formatter, "required backend `{id}` is unavailable")
            }
            Self::LaneMismatch { backend } => {
                write!(
                    formatter,
                    "backend `{backend}` is incompatible with the execution lane"
                )
            }
            Self::LayoutMismatch => formatter.write_str("operation and layout do not match"),
            Self::LengthMismatch {
                buffer,
                expected,
                actual,
            } => write!(
                formatter,
                "{buffer} buffer length mismatch: expected {expected}, got {actual}"
            ),
            Self::DimensionOverflow => formatter.write_str("layout dimensions overflow usize"),
            Self::ArithmeticOverflow => formatter.write_str("fixed-point arithmetic overflow"),
            Self::BackendFailure { backend, message } => {
                write!(formatter, "backend `{backend}` failed: {message}")
            }
            Self::NativeAbiFailure { backend, status } => {
                write!(
                    formatter,
                    "backend `{backend}` returned ABI status {status}"
                )
            }
            Self::InvalidExecutionEvidence { backend } => {
                write!(
                    formatter,
                    "backend `{backend}` returned invalid execution evidence"
                )
            }
            Self::UnsupportedBufferDomain { backend } => {
                write!(
                    formatter,
                    "backend `{backend}` does not implement this buffer domain"
                )
            }
            Self::NonFiniteInput { index } => {
                write!(formatter, "non-finite binary32 input at index {index}")
            }
        }
    }
}

impl std::error::Error for AcceleratorError {}

/// Small universal backend interface. Domain types never cross it.
pub trait Backend: Send + Sync {
    fn descriptor(&self) -> &'static BackendDescriptor;

    fn execute(
        &self,
        request: &KernelRequest<'_>,
        buffers: KernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError>;

    fn execute_f32(
        &self,
        _request: &KernelRequest<'_>,
        _buffers: F32KernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        Err(AcceleratorError::UnsupportedBufferDomain {
            backend: self.descriptor().id,
        })
    }

    fn execute_f32_binary(
        &self,
        _request: &KernelRequest<'_>,
        _buffers: F32BinaryKernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        Err(AcceleratorError::UnsupportedBufferDomain {
            backend: self.descriptor().id,
        })
    }
}

/// Registry construction may allocate; planning and kernel execution do not.
#[derive(Default)]
pub struct Registry<'a> {
    backends: Vec<&'a dyn Backend>,
}

impl<'a> Registry<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, backend: &'a dyn Backend) -> Result<(), AcceleratorError> {
        let id = backend.descriptor().id;
        if self
            .backends
            .iter()
            .any(|registered| registered.descriptor().id == id)
        {
            return Err(AcceleratorError::DuplicateBackendId(id));
        }
        self.backends.push(backend);
        Ok(())
    }

    pub fn plan(&self, request: &KernelRequest<'_>) -> Result<PlannedKernel<'a>, AcceleratorError> {
        let mut candidates = self.backends.iter().copied().filter(|backend| {
            let descriptor = backend.descriptor();
            descriptor.capability_for(request).is_some()
                && lane_compatible(descriptor, request.lane)
                && fallback_compatible(descriptor, request)
        });

        let selected = candidates
            .next()
            .into_iter()
            .chain(candidates)
            .max_by(|left, right| compare_backends(*left, *right));

        match selected {
            Some(backend) => Ok(PlannedKernel { backend }),
            None if request.named_backend.is_some() => {
                Err(AcceleratorError::NamedBackendUnavailable(
                    request.named_backend.unwrap_or_default().to_owned(),
                ))
            }
            None => Err(AcceleratorError::NoCompatibleBackend {
                operation: request.operation.id(),
            }),
        }
    }
}

fn compare_backends(left: &dyn Backend, right: &dyn Backend) -> Ordering {
    let left = left.descriptor();
    let right = right.descriptor();
    left.priority
        .cmp(&right.priority)
        .then_with(|| right.id.cmp(left.id))
}

fn lane_compatible(descriptor: &BackendDescriptor, lane: ExecutionLane) -> bool {
    match lane {
        ExecutionLane::Authoritative => !descriptor.is_remote,
        ExecutionLane::Advisory => !descriptor.is_remote,
        ExecutionLane::RemoteJob => descriptor.is_remote,
    }
}

fn fallback_compatible(descriptor: &BackendDescriptor, request: &KernelRequest<'_>) -> bool {
    if let Some(required) = request.named_backend {
        return descriptor.id == required;
    }
    match request.fallback {
        FallbackPolicy::RequireNamedBackend => false,
        FallbackPolicy::RequireDeterministicEquivalent => descriptor
            .capability_for(request)
            .is_some_and(|capability| capability.determinism == Determinism::CanonicalExact),
        FallbackPolicy::AllowReference => descriptor.is_reference,
        FallbackPolicy::PreferAccelerated => true,
    }
}

/// A fixed backend selection. Execution returns the selected backend's failure;
/// it never consults the registry again or silently retries.
#[derive(Clone, Copy)]
pub struct PlannedKernel<'a> {
    backend: &'a dyn Backend,
}

impl PlannedKernel<'_> {
    pub fn backend_id(&self) -> &'static str {
        self.backend.descriptor().id
    }

    pub fn execute(
        &self,
        request: &KernelRequest<'_>,
        buffers: KernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        self.backend.execute(request, buffers)
    }

    pub fn execute_f32(
        &self,
        request: &KernelRequest<'_>,
        buffers: F32KernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        self.backend.execute_f32(request, buffers)
    }

    pub fn execute_f32_binary(
        &self,
        request: &KernelRequest<'_>,
        buffers: F32BinaryKernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        self.backend.execute_f32_binary(request, buffers)
    }
}

const REFERENCE_CAPABILITIES: [Capability; 7] = [
    Capability {
        operation: Operation::FixedI32Dot,
        version: ACCELERATOR_CONTRACT_VERSION,
        support: SupportLevel::Deterministic,
        determinism: Determinism::CanonicalExact,
    },
    Capability {
        operation: Operation::TensorF32Relu,
        version: ACCELERATOR_CONTRACT_VERSION,
        support: SupportLevel::Deterministic,
        determinism: Determinism::ToleranceBounded,
    },
    Capability {
        operation: Operation::TensorF32Relu6,
        version: ACCELERATOR_CONTRACT_VERSION,
        support: SupportLevel::Deterministic,
        determinism: Determinism::ToleranceBounded,
    },
    Capability {
        operation: Operation::FixedI32MatMul,
        version: ACCELERATOR_CONTRACT_VERSION,
        support: SupportLevel::Deterministic,
        determinism: Determinism::CanonicalExact,
    },
    Capability {
        operation: Operation::TensorF32MatMul,
        version: ACCELERATOR_CONTRACT_VERSION,
        support: SupportLevel::Deterministic,
        determinism: Determinism::ToleranceBounded,
    },
    Capability {
        operation: Operation::TensorF32Add,
        version: ACCELERATOR_CONTRACT_VERSION,
        support: SupportLevel::Deterministic,
        determinism: Determinism::ToleranceBounded,
    },
    Capability {
        operation: Operation::TensorF32Mul,
        version: ACCELERATOR_CONTRACT_VERSION,
        support: SupportLevel::Deterministic,
        determinism: Determinism::ToleranceBounded,
    },
];

const REFERENCE_DESCRIPTOR: BackendDescriptor = BackendDescriptor {
    id: "enaction.cpu.scalar.reference",
    implementation_version: ACCELERATOR_CONTRACT_VERSION,
    device_class: DeviceClass::Cpu,
    priority: 0,
    is_reference: true,
    is_remote: false,
    capabilities: &REFERENCE_CAPABILITIES,
};

/// Exact, allocation-free scalar reference implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScalarReferenceBackend;

impl Backend for ScalarReferenceBackend {
    fn descriptor(&self) -> &'static BackendDescriptor {
        &REFERENCE_DESCRIPTOR
    }

    fn execute(
        &self,
        request: &KernelRequest<'_>,
        buffers: KernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        let capability = self.descriptor().capability_for(request).ok_or(
            AcceleratorError::NoCompatibleBackend {
                operation: request.operation.id(),
            },
        )?;
        match (request.operation, request.layout) {
            (Operation::FixedI32Dot, Layout::Dot { len }) => {
                validate_length("left", buffers.left.len(), len)?;
                validate_length("right", buffers.right.len(), len)?;
                validate_length("output", buffers.output.len(), 1)?;
                buffers.output[0] = checked_dot(buffers.left, buffers.right)?;
            }
            (Operation::FixedI32MatMul, Layout::MatMul { m, k, n }) => {
                let left_len = m
                    .checked_mul(k)
                    .ok_or(AcceleratorError::DimensionOverflow)?;
                let right_len = k
                    .checked_mul(n)
                    .ok_or(AcceleratorError::DimensionOverflow)?;
                let output_len = m
                    .checked_mul(n)
                    .ok_or(AcceleratorError::DimensionOverflow)?;
                validate_length("left", buffers.left.len(), left_len)?;
                validate_length("right", buffers.right.len(), right_len)?;
                validate_length("output", buffers.output.len(), output_len)?;
                checked_matmul(m, k, n, buffers.left, buffers.right, buffers.output)?;
            }
            _ => return Err(AcceleratorError::LayoutMismatch),
        }
        Ok(ExecutionEvidence {
            backend_id: self.descriptor().id,
            backend_version: self.descriptor().implementation_version,
            operation: request.operation,
            operation_version: request.version,
            determinism: capability.determinism,
            support: capability.support,
        })
    }

    fn execute_f32(
        &self,
        request: &KernelRequest<'_>,
        buffers: F32KernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        let capability = self.descriptor().capability_for(request).ok_or(
            AcceleratorError::NoCompatibleBackend {
                operation: request.operation.id(),
            },
        )?;
        let len = match (request.operation, request.layout) {
            (Operation::TensorF32Relu | Operation::TensorF32Relu6, Layout::Vector { len }) => len,
            _ => return Err(AcceleratorError::LayoutMismatch),
        };
        validate_length("input", buffers.input.len(), len)?;
        validate_length("output", buffers.output.len(), len)?;
        if let Some((index, _)) = buffers
            .input
            .iter()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(AcceleratorError::NonFiniteInput { index });
        }
        match request.operation {
            Operation::TensorF32Relu => {
                for (&input, output) in buffers.input.iter().zip(buffers.output.iter_mut()) {
                    *output = if input > 0.0 { input } else { 0.0 };
                }
            }
            Operation::TensorF32Relu6 => {
                for (&input, output) in buffers.input.iter().zip(buffers.output.iter_mut()) {
                    *output = if input.is_sign_negative() || input == 0.0 {
                        0.0
                    } else if input > 6.0 {
                        6.0
                    } else {
                        input
                    };
                }
            }
            _ => return Err(AcceleratorError::LayoutMismatch),
        }
        Ok(ExecutionEvidence {
            backend_id: self.descriptor().id,
            backend_version: self.descriptor().implementation_version,
            operation: request.operation,
            operation_version: request.version,
            determinism: capability.determinism,
            support: capability.support,
        })
    }

    fn execute_f32_binary(
        &self,
        request: &KernelRequest<'_>,
        buffers: F32BinaryKernelBuffers<'_>,
    ) -> Result<ExecutionEvidence, AcceleratorError> {
        let capability = self.descriptor().capability_for(request).ok_or(
            AcceleratorError::NoCompatibleBackend {
                operation: request.operation.id(),
            },
        )?;
        match (request.operation, request.layout) {
            (Operation::TensorF32MatMul, Layout::MatMul { m, k, n }) => {
                let left_len = m
                    .checked_mul(k)
                    .ok_or(AcceleratorError::DimensionOverflow)?;
                let right_len = k
                    .checked_mul(n)
                    .ok_or(AcceleratorError::DimensionOverflow)?;
                let output_len = m
                    .checked_mul(n)
                    .ok_or(AcceleratorError::DimensionOverflow)?;
                validate_length("left", buffers.left.len(), left_len)?;
                validate_length("right", buffers.right.len(), right_len)?;
                validate_length("output", buffers.output.len(), output_len)?;
                validate_finite(buffers.left)?;
                validate_finite(buffers.right)?;
                f32_matmul(m, k, n, buffers.left, buffers.right, buffers.output)?;
            }
            (Operation::TensorF32Add | Operation::TensorF32Mul, Layout::Vector { len }) => {
                validate_length("left", buffers.left.len(), len)?;
                validate_length("right", buffers.right.len(), len)?;
                validate_length("output", buffers.output.len(), len)?;
                validate_finite(buffers.left)?;
                validate_finite(buffers.right)?;
                for (&left, &right) in buffers.left.iter().zip(buffers.right) {
                    let result = if request.operation == Operation::TensorF32Add {
                        left + right
                    } else {
                        left * right
                    };
                    if !result.is_finite() {
                        return Err(AcceleratorError::ArithmeticOverflow);
                    }
                }
                for ((&left, &right), output) in buffers
                    .left
                    .iter()
                    .zip(buffers.right)
                    .zip(buffers.output.iter_mut())
                {
                    *output = if request.operation == Operation::TensorF32Add {
                        left + right
                    } else {
                        left * right
                    };
                }
            }
            _ => return Err(AcceleratorError::LayoutMismatch),
        }
        Ok(ExecutionEvidence {
            backend_id: self.descriptor().id,
            backend_version: self.descriptor().implementation_version,
            operation: request.operation,
            operation_version: request.version,
            determinism: capability.determinism,
            support: capability.support,
        })
    }
}

fn validate_finite(values: &[f32]) -> Result<(), AcceleratorError> {
    if let Some((index, _)) = values
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        Err(AcceleratorError::NonFiniteInput { index })
    } else {
        Ok(())
    }
}

fn f32_matmul(
    m: usize,
    k: usize,
    n: usize,
    left: &[f32],
    right: &[f32],
    output: &mut [f32],
) -> Result<(), AcceleratorError> {
    for row in 0..m {
        for column in 0..n {
            f32_matmul_cell(row, column, k, n, left, right)?;
        }
    }
    for row in 0..m {
        for column in 0..n {
            output[row * n + column] = f32_matmul_cell(row, column, k, n, left, right)?;
        }
    }
    Ok(())
}

fn f32_matmul_cell(
    row: usize,
    column: usize,
    k: usize,
    n: usize,
    left: &[f32],
    right: &[f32],
) -> Result<f32, AcceleratorError> {
    let mut sum = 0.0_f32;
    for inner in 0..k {
        sum += left[row * k + inner] * right[inner * n + column];
        if !sum.is_finite() {
            return Err(AcceleratorError::ArithmeticOverflow);
        }
    }
    Ok(sum)
}

fn validate_length(
    buffer: &'static str,
    actual: usize,
    expected: usize,
) -> Result<(), AcceleratorError> {
    if actual == expected {
        Ok(())
    } else {
        Err(AcceleratorError::LengthMismatch {
            buffer,
            expected,
            actual,
        })
    }
}

fn checked_dot(left: &[i32], right: &[i32]) -> Result<i64, AcceleratorError> {
    left.iter()
        .zip(right)
        .try_fold(0_i64, |sum, (&left, &right)| {
            let product = i64::from(left)
                .checked_mul(i64::from(right))
                .ok_or(AcceleratorError::ArithmeticOverflow)?;
            sum.checked_add(product)
                .ok_or(AcceleratorError::ArithmeticOverflow)
        })
}

fn checked_matmul(
    m: usize,
    k: usize,
    n: usize,
    left: &[i32],
    right: &[i32],
    output: &mut [i64],
) -> Result<(), AcceleratorError> {
    // Validate every cell before changing caller-owned output. This makes an
    // arithmetic failure atomic without allocating scratch storage.
    for row in 0..m {
        for column in 0..n {
            checked_matmul_cell(row, column, k, n, left, right)?;
        }
    }
    for row in 0..m {
        for column in 0..n {
            output[row * n + column] = checked_matmul_cell(row, column, k, n, left, right)?;
        }
    }
    Ok(())
}

fn checked_matmul_cell(
    row: usize,
    column: usize,
    k: usize,
    n: usize,
    left: &[i32],
    right: &[i32],
) -> Result<i64, AcceleratorError> {
    (0..k).try_fold(0_i64, |sum, inner| {
        let product = i64::from(left[row * k + inner])
            .checked_mul(i64::from(right[inner * n + column]))
            .ok_or(AcceleratorError::ArithmeticOverflow)?;
        sum.checked_add(product)
            .ok_or(AcceleratorError::ArithmeticOverflow)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOCK_CAPABILITIES: [Capability; 2] = [
        Capability {
            operation: Operation::FixedI32Dot,
            version: ACCELERATOR_CONTRACT_VERSION,
            support: SupportLevel::Deterministic,
            determinism: Determinism::CanonicalExact,
        },
        Capability {
            operation: Operation::FixedI32MatMul,
            version: ACCELERATOR_CONTRACT_VERSION,
            support: SupportLevel::Deterministic,
            determinism: Determinism::CanonicalExact,
        },
    ];

    const FAILING_DESCRIPTOR: BackendDescriptor = BackendDescriptor {
        id: "test.accelerated.failure",
        implementation_version: ACCELERATOR_CONTRACT_VERSION,
        device_class: DeviceClass::Gpu,
        priority: 100,
        is_reference: false,
        is_remote: false,
        capabilities: &MOCK_CAPABILITIES,
    };

    struct FailingBackend;

    impl Backend for FailingBackend {
        fn descriptor(&self) -> &'static BackendDescriptor {
            &FAILING_DESCRIPTOR
        }

        fn execute(
            &self,
            _request: &KernelRequest<'_>,
            _buffers: KernelBuffers<'_>,
        ) -> Result<ExecutionEvidence, AcceleratorError> {
            Err(AcceleratorError::BackendFailure {
                backend: self.descriptor().id,
                message: "planted execution failure",
            })
        }
    }

    fn dot_request(fallback: FallbackPolicy) -> KernelRequest<'static> {
        KernelRequest {
            operation: Operation::FixedI32Dot,
            version: ACCELERATOR_CONTRACT_VERSION,
            layout: Layout::Dot { len: 3 },
            lane: ExecutionLane::Authoritative,
            minimum_support: SupportLevel::Conformant,
            minimum_determinism: Determinism::CanonicalExact,
            fallback,
            named_backend: None,
        }
    }

    #[test]
    fn exact_dot_conformance() {
        let backend = ScalarReferenceBackend;
        let mut registry = Registry::new();
        registry.register(&backend).unwrap();
        let request = dot_request(FallbackPolicy::AllowReference);
        let planned = registry.plan(&request).unwrap();
        let mut output = [0];
        let evidence = planned
            .execute(
                &request,
                KernelBuffers {
                    left: &[2, -3, 4],
                    right: &[5, 7, -2],
                    output: &mut output,
                },
            )
            .unwrap();
        assert_eq!(output, [-19]);
        assert_eq!(evidence.backend_id, REFERENCE_DESCRIPTOR.id);
        assert_eq!(evidence.determinism, Determinism::CanonicalExact);
    }

    #[test]
    fn exact_matrix_conformance() {
        let backend = ScalarReferenceBackend;
        let request = KernelRequest {
            operation: Operation::FixedI32MatMul,
            version: ACCELERATOR_CONTRACT_VERSION,
            layout: Layout::MatMul { m: 2, k: 3, n: 2 },
            lane: ExecutionLane::Authoritative,
            minimum_support: SupportLevel::Deterministic,
            minimum_determinism: Determinism::CanonicalExact,
            fallback: FallbackPolicy::AllowReference,
            named_backend: None,
        };
        let mut output = [0; 4];
        backend
            .execute(
                &request,
                KernelBuffers {
                    left: &[1, 2, 3, 4, 5, 6],
                    right: &[7, 8, 9, 10, 11, 12],
                    output: &mut output,
                },
            )
            .unwrap();
        assert_eq!(output, [58, 64, 139, 154]);
    }

    #[test]
    fn f32_pointwise_is_advisory_and_nonfinite_failure_is_atomic() {
        let backend = ScalarReferenceBackend;
        let request = KernelRequest {
            operation: Operation::TensorF32Relu6,
            version: ACCELERATOR_CONTRACT_VERSION,
            layout: Layout::Vector { len: 4 },
            lane: ExecutionLane::Advisory,
            minimum_support: SupportLevel::Deterministic,
            minimum_determinism: Determinism::ToleranceBounded,
            fallback: FallbackPolicy::AllowReference,
            named_backend: None,
        };
        let mut output = [91.0; 4];
        let evidence = backend
            .execute_f32(
                &request,
                F32KernelBuffers {
                    input: &[-1.0, -0.0, 2.5, 9.0],
                    output: &mut output,
                },
            )
            .unwrap();
        assert_eq!(output.map(f32::to_bits), [0, 0, 0x4020_0000, 0x40c0_0000]);
        assert_eq!(evidence.determinism, Determinism::ToleranceBounded);

        let mut untouched = [71.0, 72.0, 73.0, 74.0];
        assert_eq!(
            backend.execute_f32(
                &request,
                F32KernelBuffers {
                    input: &[1.0, f32::NAN, 3.0, 4.0],
                    output: &mut untouched,
                },
            ),
            Err(AcceleratorError::NonFiniteInput { index: 1 })
        );
        assert_eq!(untouched, [71.0, 72.0, 73.0, 74.0]);

        let authoritative = KernelRequest {
            lane: ExecutionLane::Authoritative,
            ..request
        };
        let mut registry = Registry::new();
        registry.register(&backend).unwrap();
        assert!(matches!(
            registry.plan(&authoritative),
            Err(AcceleratorError::NoCompatibleBackend { .. })
        ));
    }

    #[test]
    fn planner_is_independent_of_registration_order() {
        let reference = ScalarReferenceBackend;
        let failing = FailingBackend;
        let request = dot_request(FallbackPolicy::PreferAccelerated);

        let mut first = Registry::new();
        first.register(&reference).unwrap();
        first.register(&failing).unwrap();
        let mut second = Registry::new();
        second.register(&failing).unwrap();
        second.register(&reference).unwrap();

        assert_eq!(
            first.plan(&request).unwrap().backend_id(),
            FAILING_DESCRIPTOR.id
        );
        assert_eq!(
            second.plan(&request).unwrap().backend_id(),
            FAILING_DESCRIPTOR.id
        );
    }

    #[test]
    fn planted_runtime_failure_does_not_silently_retry_reference() {
        let reference = ScalarReferenceBackend;
        let failing = FailingBackend;
        let mut registry = Registry::new();
        registry.register(&reference).unwrap();
        registry.register(&failing).unwrap();
        let request = dot_request(FallbackPolicy::PreferAccelerated);
        let planned = registry.plan(&request).unwrap();
        assert_eq!(planned.backend_id(), FAILING_DESCRIPTOR.id);

        let mut output = [777];
        let error = planned
            .execute(
                &request,
                KernelBuffers {
                    left: &[1, 2, 3],
                    right: &[4, 5, 6],
                    output: &mut output,
                },
            )
            .unwrap_err();
        assert_eq!(
            error,
            AcceleratorError::BackendFailure {
                backend: FAILING_DESCRIPTOR.id,
                message: "planted execution failure"
            }
        );
        assert_eq!(output, [777], "the reference backend must not have run");
    }

    #[test]
    fn malformed_shapes_and_overflow_fail_loudly() {
        let backend = ScalarReferenceBackend;
        let request = dot_request(FallbackPolicy::AllowReference);
        let mut output = [0];
        assert_eq!(
            backend.execute(
                &request,
                KernelBuffers {
                    left: &[1, 2],
                    right: &[3, 4, 5],
                    output: &mut output,
                }
            ),
            Err(AcceleratorError::LengthMismatch {
                buffer: "left",
                expected: 3,
                actual: 2
            })
        );

        let overflow_request = KernelRequest {
            layout: Layout::Dot { len: 3 },
            ..request
        };
        assert_eq!(
            backend.execute(
                &overflow_request,
                KernelBuffers {
                    left: &[i32::MAX; 3],
                    right: &[i32::MAX; 3],
                    output: &mut output,
                }
            ),
            Err(AcceleratorError::ArithmeticOverflow)
        );
        assert_eq!(output, [0], "overflow must leave output unchanged");

        let matrix_request = KernelRequest {
            operation: Operation::FixedI32MatMul,
            layout: Layout::MatMul { m: 1, k: 3, n: 2 },
            ..request
        };
        let mut matrix_output = [71, 72];
        assert_eq!(
            backend.execute(
                &matrix_request,
                KernelBuffers {
                    left: &[i32::MAX; 3],
                    right: &[1, i32::MAX, 1, i32::MAX, 1, i32::MAX],
                    output: &mut matrix_output,
                }
            ),
            Err(AcceleratorError::ArithmeticOverflow)
        );
        assert_eq!(
            matrix_output,
            [71, 72],
            "a late-cell overflow must not expose partial matrix output"
        );
    }

    #[test]
    fn registry_rejects_duplicate_identity_and_unsupported_claims() {
        let reference = ScalarReferenceBackend;
        let mut registry = Registry::new();
        registry.register(&reference).unwrap();
        assert_eq!(
            registry.register(&reference),
            Err(AcceleratorError::DuplicateBackendId(
                REFERENCE_DESCRIPTOR.id
            ))
        );

        let request = KernelRequest {
            minimum_support: SupportLevel::ProductionSupported,
            ..dot_request(FallbackPolicy::AllowReference)
        };
        assert!(matches!(
            registry.plan(&request),
            Err(AcceleratorError::NoCompatibleBackend { .. })
        ));
    }

    #[test]
    fn authoritative_lane_refuses_remote_backend_shape() {
        const REMOTE_DESCRIPTOR: BackendDescriptor = BackendDescriptor {
            id: "test.remote.qpu",
            implementation_version: ACCELERATOR_CONTRACT_VERSION,
            device_class: DeviceClass::Qpu,
            priority: 200,
            is_reference: false,
            is_remote: true,
            capabilities: &MOCK_CAPABILITIES,
        };
        struct RemoteBackend;
        impl Backend for RemoteBackend {
            fn descriptor(&self) -> &'static BackendDescriptor {
                &REMOTE_DESCRIPTOR
            }
            fn execute(
                &self,
                _request: &KernelRequest<'_>,
                _buffers: KernelBuffers<'_>,
            ) -> Result<ExecutionEvidence, AcceleratorError> {
                unreachable!("authoritative planning must refuse remote execution")
            }
        }

        let remote = RemoteBackend;
        let mut registry = Registry::new();
        registry.register(&remote).unwrap();
        assert!(matches!(
            registry.plan(&dot_request(FallbackPolicy::PreferAccelerated)),
            Err(AcceleratorError::NoCompatibleBackend { .. })
        ));
    }

    #[test]
    fn f32_binary_family_is_correct_and_failure_atomic() {
        let backend = ScalarReferenceBackend;
        let base = KernelRequest {
            operation: Operation::TensorF32MatMul,
            version: ACCELERATOR_CONTRACT_VERSION,
            layout: Layout::MatMul { m: 2, k: 3, n: 2 },
            lane: ExecutionLane::Advisory,
            minimum_support: SupportLevel::Conformant,
            minimum_determinism: Determinism::ToleranceBounded,
            fallback: FallbackPolicy::AllowReference,
            named_backend: None,
        };
        let mut matrix = [91.0; 4];
        backend
            .execute_f32_binary(
                &base,
                F32BinaryKernelBuffers {
                    left: &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                    right: &[7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
                    output: &mut matrix,
                },
            )
            .unwrap();
        assert_eq!(matrix, [58.0, 64.0, 139.0, 154.0]);

        let add = KernelRequest {
            operation: Operation::TensorF32Add,
            layout: Layout::Vector { len: 3 },
            ..base
        };
        let mut output = [91.0; 3];
        backend
            .execute_f32_binary(
                &add,
                F32BinaryKernelBuffers {
                    left: &[1.5, -2.0, 4.0],
                    right: &[2.0, 3.0, -0.5],
                    output: &mut output,
                },
            )
            .unwrap();
        assert_eq!(output, [3.5, 1.0, 3.5]);

        let mul = KernelRequest {
            operation: Operation::TensorF32Mul,
            ..add
        };
        backend
            .execute_f32_binary(
                &mul,
                F32BinaryKernelBuffers {
                    left: &[1.5, -2.0, 4.0],
                    right: &[2.0, 3.0, -0.5],
                    output: &mut output,
                },
            )
            .unwrap();
        assert_eq!(output, [3.0, -6.0, -2.0]);
        let mut untouched = [71.0];
        let overflow = KernelRequest {
            layout: Layout::Vector { len: 1 },
            ..mul
        };
        assert_eq!(
            backend.execute_f32_binary(
                &overflow,
                F32BinaryKernelBuffers {
                    left: &[f32::MAX],
                    right: &[2.0],
                    output: &mut untouched,
                }
            ),
            Err(AcceleratorError::ArithmeticOverflow)
        );
        assert_eq!(untouched, [71.0]);
    }
}
