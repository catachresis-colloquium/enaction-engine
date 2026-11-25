// SPDX-License-Identifier: MPL-2.0
// hypatia: allow code_safety/zig_ptr_cast -- intentional test fixture aliasing check
//! Pure-Zig implementation of the Idris2-defined accelerator ABI.
//!
//! The C calling convention is the interoperability surface only. This module
//! contains no C implementation, uses no libc allocator, retains no caller
//! pointer, and never performs backend fallback.

const std = @import("std");
const abi = @import("abi");
const axiom_pointwise = @import("axiom_pointwise.zig");
const axiom_matmul = @import("axiom_matmul.zig");

comptime {
    // The normative Idris2 accelerator layout models every data pointer as an
    // eight-byte field.  Reject narrower targets here rather than allowing an
    // unrelated linker failure (or a coincidentally equal total struct size)
    // to masquerade as ABI validation.
    if (@bitSizeOf(usize) != 64)
        @compileError("Enaction accelerator ABI v1 requires 64-bit pointers");
    if (@sizeOf(abi.Request) != 56 or @alignOf(abi.Request) != 8)
        @compileError("Idris2 Request layout and Zig layout disagree");
    if (@offsetOf(abi.Request, "dim0") != 32 or @offsetOf(abi.Request, "dim2") != 48)
        @compileError("Idris2 Request field offsets and Zig offsets disagree");
    if (@sizeOf(abi.BufferI32) != 16 or @sizeOf(abi.BufferI64) != 16 or
        @sizeOf(abi.BufferF32In) != 16 or @sizeOf(abi.BufferF32Out) != 16)
        @compileError("Idris2 buffer layout and Zig layout disagree");
    if (@offsetOf(abi.BufferI32, "data") != 0 or @offsetOf(abi.BufferI32, "len") != 8 or
        @offsetOf(abi.BufferI64, "data") != 0 or @offsetOf(abi.BufferI64, "len") != 8 or
        @offsetOf(abi.BufferF32In, "data") != 0 or @offsetOf(abi.BufferF32In, "len") != 8 or
        @offsetOf(abi.BufferF32Out, "data") != 0 or @offsetOf(abi.BufferF32Out, "len") != 8)
        @compileError("Idris2 buffer field offsets and Zig offsets disagree");
    if (@sizeOf(abi.Capability) != 32 or @alignOf(abi.Capability) != 4)
        @compileError("Idris2 Capability layout and Zig layout disagree");
    if (@sizeOf(abi.Evidence) != 24 or @alignOf(abi.Evidence) != 4)
        @compileError("Idris2 Evidence layout and Zig layout disagree");
}

const capability_count: u32 = 7;
const capability_flags_authoritative: u32 = 1;
const capability_flags_advisory: u32 = 2;

const AddressRange = struct {
    start: usize,
    end: usize,

    fn overlaps(left: AddressRange, right: AddressRange) bool {
        if (left.start == left.end or right.start == right.end) return false;
        return left.start < right.end and right.start < left.end;
    }
};

fn checkedMulU64(left: u64, right: u64) ?u64 {
    const result = @mulWithOverflow(left, right);
    return if (result[1] == 0) result[0] else null;
}

fn addressRange(pointer: anytype, len: usize, element_size: usize) ?AddressRange {
    if (len == 0) return .{ .start = 0, .end = 0 };
    const start = @intFromPtr(pointer orelse return null);
    const byte_len = std.math.mul(usize, len, element_size) catch return null;
    const end = std.math.add(usize, start, byte_len) catch return null;
    return .{ .start = start, .end = end };
}

fn validLane(lane: u32) bool {
    return lane == abi.lane_authoritative or lane == abi.lane_advisory;
}

fn validateRequest(request: *const abi.Request) u32 {
    if (request.abi_major != abi.abi_major or request.abi_minor > abi.abi_minor)
        return abi.status_unsupported_abi;
    if (request.operation_major != abi.operation_major or request.operation_minor > abi.operation_minor)
        return abi.status_unsupported_operation_version;
    if (!validLane(request.lane)) return abi.status_invalid_lane;
    if (request.minimum_support < abi.support_declared or request.minimum_support > abi.support_production_supported)
        return abi.status_invalid_support;
    if (request.minimum_support > abi.support_resilient)
        return abi.status_unsupported_requirement;
    if (request.minimum_determinism < abi.determinism_advisory_only or request.minimum_determinism > abi.determinism_canonical_exact)
        return abi.status_invalid_determinism;
    if (request.reserved != 0) return abi.status_invalid_reserved_field;
    return switch (request.operation) {
        abi.operation_fixed_i32_dot => if (request.layout == abi.layout_dot) abi.status_ok else abi.status_layout_mismatch,
        abi.operation_fixed_i32_matmul => if (request.layout == abi.layout_matmul) abi.status_ok else abi.status_layout_mismatch,
        else => abi.status_unknown_operation,
    };
}

fn validateF32Request(request: *const abi.Request) u32 {
    if (request.abi_major != abi.abi_major or request.abi_minor > abi.abi_minor)
        return abi.status_unsupported_abi;
    if (request.operation_major != abi.operation_major or request.operation_minor > abi.operation_minor)
        return abi.status_unsupported_operation_version;
    if (!validLane(request.lane)) return abi.status_invalid_lane;
    if (request.lane != abi.lane_advisory) return abi.status_unsupported_requirement;
    if (request.minimum_support < abi.support_declared or request.minimum_support > abi.support_production_supported)
        return abi.status_invalid_support;
    if (request.minimum_support > abi.support_resilient) return abi.status_unsupported_requirement;
    if (request.minimum_determinism < abi.determinism_advisory_only or request.minimum_determinism > abi.determinism_canonical_exact)
        return abi.status_invalid_determinism;
    if (request.minimum_determinism > abi.determinism_tolerance_bounded)
        return abi.status_unsupported_requirement;
    if (request.reserved != 0) return abi.status_invalid_reserved_field;
    return switch (request.operation) {
        abi.operation_tensor_f32_relu, abi.operation_tensor_f32_relu6 => if (request.layout == abi.layout_vector) abi.status_ok else abi.status_layout_mismatch,
        else => abi.status_unknown_operation,
    };
}

fn validateF32BinaryRequest(request: *const abi.Request) u32 {
    if (request.abi_major != abi.abi_major or request.abi_minor > abi.abi_minor)
        return abi.status_unsupported_abi;
    if (request.operation_major != abi.operation_major or request.operation_minor > abi.operation_minor)
        return abi.status_unsupported_operation_version;
    if (!validLane(request.lane)) return abi.status_invalid_lane;
    if (request.lane != abi.lane_advisory) return abi.status_unsupported_requirement;
    if (request.minimum_support < abi.support_declared or request.minimum_support > abi.support_production_supported)
        return abi.status_invalid_support;
    if (request.minimum_support > abi.support_resilient) return abi.status_unsupported_requirement;
    if (request.minimum_determinism < abi.determinism_advisory_only or request.minimum_determinism > abi.determinism_canonical_exact)
        return abi.status_invalid_determinism;
    if (request.minimum_determinism > abi.determinism_tolerance_bounded)
        return abi.status_unsupported_requirement;
    if (request.reserved != 0) return abi.status_invalid_reserved_field;
    return switch (request.operation) {
        abi.operation_tensor_f32_matmul => if (request.layout == abi.layout_matmul) abi.status_ok else abi.status_layout_mismatch,
        abi.operation_tensor_f32_add, abi.operation_tensor_f32_mul => if (request.layout == abi.layout_vector) abi.status_ok else abi.status_layout_mismatch,
        else => abi.status_unknown_operation,
    };
}

fn fillEvidence(request: *const abi.Request, determinism: u32, evidence: *abi.Evidence) void {
    evidence.* = .{
        .abi_major = abi.abi_major,
        .abi_minor = abi.abi_minor,
        .operation_major = abi.operation_major,
        .operation_minor = abi.operation_minor,
        .operation = request.operation,
        .backend_id = abi.backend_zig_scalar,
        .support = abi.support_resilient,
        .determinism = determinism,
    };
}

fn capability(operation: u32, determinism: u32, flags: u32) abi.Capability {
    return .{
        .abi_major = abi.abi_major,
        .abi_minor = abi.abi_minor,
        .operation_major = abi.operation_major,
        .operation_minor = abi.operation_minor,
        .operation = operation,
        .support = abi.support_resilient,
        .determinism = determinism,
        .backend_id = abi.backend_zig_scalar,
        .device_class = abi.device_cpu,
        .flags = flags,
    };
}

export fn enaction_accel_abi_version() u32 {
    return (@as(u32, abi.abi_major) << 16) | @as(u32, abi.abi_minor);
}

export fn enaction_accel_capability_count() u32 {
    return capability_count;
}

export fn enaction_accel_capability_at(index: u32, out: ?*abi.Capability) u32 {
    const destination = out orelse return abi.status_null_pointer;
    destination.* = switch (index) {
        0 => capability(abi.operation_fixed_i32_dot, abi.determinism_canonical_exact, capability_flags_authoritative | capability_flags_advisory),
        1 => capability(abi.operation_fixed_i32_matmul, abi.determinism_canonical_exact, capability_flags_authoritative | capability_flags_advisory),
        2 => capability(abi.operation_tensor_f32_relu, abi.determinism_tolerance_bounded, capability_flags_advisory),
        3 => capability(abi.operation_tensor_f32_relu6, abi.determinism_tolerance_bounded, capability_flags_advisory),
        4 => capability(abi.operation_tensor_f32_matmul, abi.determinism_tolerance_bounded, capability_flags_advisory),
        5 => capability(abi.operation_tensor_f32_add, abi.determinism_tolerance_bounded, capability_flags_advisory),
        6 => capability(abi.operation_tensor_f32_mul, abi.determinism_tolerance_bounded, capability_flags_advisory),
        else => return abi.status_index_out_of_range,
    };
    return abi.status_ok;
}

fn checkedDot(left: []const i32, right: []const i32) ?i64 {
    var sum: i64 = 0;
    for (left, right) |left_value, right_value| {
        const product = @mulWithOverflow(@as(i64, left_value), @as(i64, right_value));
        if (product[1] != 0) return null;
        const next = @addWithOverflow(sum, product[0]);
        if (next[1] != 0) return null;
        sum = next[0];
    }
    return sum;
}

fn checkedMatmulCell(
    row: usize,
    column: usize,
    k: usize,
    n: usize,
    left: []const i32,
    right: []const i32,
) ?i64 {
    var sum: i64 = 0;
    for (0..k) |inner| {
        const product = @mulWithOverflow(
            @as(i64, left[row * k + inner]),
            @as(i64, right[inner * n + column]),
        );
        if (product[1] != 0) return null;
        const next = @addWithOverflow(sum, product[0]);
        if (next[1] != 0) return null;
        sum = next[0];
    }
    return sum;
}

fn executeDot(
    request: *const abi.Request,
    left_buffer: *const abi.BufferI32,
    right_buffer: *const abi.BufferI32,
    output_buffer: *abi.BufferI64,
) u32 {
    if (request.dim1 != 0 or request.dim2 != 0) return abi.status_layout_mismatch;
    const len = std.math.cast(usize, request.dim0) orelse return abi.status_dimension_overflow;
    if (left_buffer.len != request.dim0 or right_buffer.len != request.dim0 or output_buffer.len != 1)
        return abi.status_length_mismatch;
    if (len != 0 and (left_buffer.data == null or right_buffer.data == null))
        return abi.status_null_pointer;
    const output_pointer = output_buffer.data orelse return abi.status_null_pointer;

    const left_range = addressRange(left_buffer.data, len, @sizeOf(i32)) orelse return abi.status_dimension_overflow;
    const right_range = addressRange(right_buffer.data, len, @sizeOf(i32)) orelse return abi.status_dimension_overflow;
    const output_range = addressRange(output_buffer.data, 1, @sizeOf(i64)) orelse return abi.status_dimension_overflow;
    if (output_range.overlaps(left_range) or output_range.overlaps(right_range))
        return abi.status_aliasing_violation;

    const left = if (len == 0) &[_]i32{} else left_buffer.data.?[0..len];
    const right = if (len == 0) &[_]i32{} else right_buffer.data.?[0..len];
    output_pointer[0] = checkedDot(left, right) orelse return abi.status_arithmetic_overflow;
    return abi.status_ok;
}

fn executeMatmul(
    request: *const abi.Request,
    left_buffer: *const abi.BufferI32,
    right_buffer: *const abi.BufferI32,
    output_buffer: *abi.BufferI64,
) u32 {
    const left_len_u64 = checkedMulU64(request.dim0, request.dim1) orelse return abi.status_dimension_overflow;
    const right_len_u64 = checkedMulU64(request.dim1, request.dim2) orelse return abi.status_dimension_overflow;
    const output_len_u64 = checkedMulU64(request.dim0, request.dim2) orelse return abi.status_dimension_overflow;
    if (left_buffer.len != left_len_u64 or right_buffer.len != right_len_u64 or output_buffer.len != output_len_u64)
        return abi.status_length_mismatch;

    const m = std.math.cast(usize, request.dim0) orelse return abi.status_dimension_overflow;
    const k = std.math.cast(usize, request.dim1) orelse return abi.status_dimension_overflow;
    const n = std.math.cast(usize, request.dim2) orelse return abi.status_dimension_overflow;
    const left_len = std.math.cast(usize, left_len_u64) orelse return abi.status_dimension_overflow;
    const right_len = std.math.cast(usize, right_len_u64) orelse return abi.status_dimension_overflow;
    const output_len = std.math.cast(usize, output_len_u64) orelse return abi.status_dimension_overflow;
    if (left_len != 0 and left_buffer.data == null) return abi.status_null_pointer;
    if (right_len != 0 and right_buffer.data == null) return abi.status_null_pointer;
    if (output_len != 0 and output_buffer.data == null) return abi.status_null_pointer;

    const left_range = addressRange(left_buffer.data, left_len, @sizeOf(i32)) orelse return abi.status_dimension_overflow;
    const right_range = addressRange(right_buffer.data, right_len, @sizeOf(i32)) orelse return abi.status_dimension_overflow;
    const output_range = addressRange(output_buffer.data, output_len, @sizeOf(i64)) orelse return abi.status_dimension_overflow;
    if (output_range.overlaps(left_range) or output_range.overlaps(right_range))
        return abi.status_aliasing_violation;

    const left = if (left_len == 0) &[_]i32{} else left_buffer.data.?[0..left_len];
    const right = if (right_len == 0) &[_]i32{} else right_buffer.data.?[0..right_len];

    // First pass proves the entire result is representable. No output byte is
    // changed until every cell has succeeded.
    for (0..m) |row| {
        for (0..n) |column| {
            _ = checkedMatmulCell(row, column, k, n, left, right) orelse return abi.status_arithmetic_overflow;
        }
    }
    if (output_len != 0) {
        const output = output_buffer.data.?[0..output_len];
        for (0..m) |row| {
            for (0..n) |column| {
                output[row * n + column] = checkedMatmulCell(row, column, k, n, left, right).?;
            }
        }
    }
    return abi.status_ok;
}

export fn enaction_accel_execute(
    request_pointer: ?*const abi.Request,
    left_pointer: ?*const abi.BufferI32,
    right_pointer: ?*const abi.BufferI32,
    output_pointer: ?*abi.BufferI64,
    evidence_pointer: ?*abi.Evidence,
) u32 {
    const request = request_pointer orelse return abi.status_null_pointer;
    const left = left_pointer orelse return abi.status_null_pointer;
    const right = right_pointer orelse return abi.status_null_pointer;
    const output = output_pointer orelse return abi.status_null_pointer;
    const evidence = evidence_pointer orelse return abi.status_null_pointer;
    const request_status = validateRequest(request);
    if (request_status != abi.status_ok) return request_status;

    const execution_status = switch (request.operation) {
        abi.operation_fixed_i32_dot => executeDot(request, left, right, output),
        abi.operation_fixed_i32_matmul => executeMatmul(request, left, right, output),
        else => unreachable,
    };
    if (execution_status != abi.status_ok) return execution_status;
    fillEvidence(request, abi.determinism_canonical_exact, evidence);
    return abi.status_ok;
}

fn executeF32Unary(
    request: *const abi.Request,
    input_buffer: *const abi.BufferF32In,
    output_buffer: *abi.BufferF32Out,
) u32 {
    if (request.dim1 != 0 or request.dim2 != 0) return abi.status_layout_mismatch;
    const len = std.math.cast(usize, request.dim0) orelse return abi.status_dimension_overflow;
    if (input_buffer.len != request.dim0 or output_buffer.len != request.dim0)
        return abi.status_length_mismatch;
    if (len != 0 and (input_buffer.data == null or output_buffer.data == null))
        return abi.status_null_pointer;
    const input_range = addressRange(input_buffer.data, len, @sizeOf(f32)) orelse return abi.status_dimension_overflow;
    const output_range = addressRange(output_buffer.data, len, @sizeOf(f32)) orelse return abi.status_dimension_overflow;
    if (input_range.overlaps(output_range)) return abi.status_aliasing_violation;
    const input = if (len == 0) &[_]f32{} else input_buffer.data.?[0..len];

    // Preflight every value so failure cannot expose partial output.
    for (input) |value| if (!std.math.isFinite(value)) return abi.status_non_finite_input;
    if (len != 0) {
        const output = output_buffer.data.?[0..len];
        switch (request.operation) {
            abi.operation_tensor_f32_relu => axiom_pointwise.relu(input, output),
            abi.operation_tensor_f32_relu6 => axiom_pointwise.relu6(input, output),
            else => unreachable,
        }
    }
    return abi.status_ok;
}

export fn enaction_accel_execute_f32(
    request_pointer: ?*const abi.Request,
    input_pointer: ?*const abi.BufferF32In,
    output_pointer: ?*abi.BufferF32Out,
    evidence_pointer: ?*abi.Evidence,
) u32 {
    const request = request_pointer orelse return abi.status_null_pointer;
    const input = input_pointer orelse return abi.status_null_pointer;
    const output = output_pointer orelse return abi.status_null_pointer;
    const evidence = evidence_pointer orelse return abi.status_null_pointer;
    const request_status = validateF32Request(request);
    if (request_status != abi.status_ok) return request_status;
    const execution_status = executeF32Unary(request, input, output);
    if (execution_status != abi.status_ok) return execution_status;
    fillEvidence(request, abi.determinism_tolerance_bounded, evidence);
    return abi.status_ok;
}

fn f32MatmulCell(row: usize, column: usize, k: usize, n: usize, left: []const f32, right: []const f32) ?f32 {
    var sum: f32 = 0.0;
    for (0..k) |inner| {
        sum += left[row * k + inner] * right[inner * n + column];
        if (!std.math.isFinite(sum)) return null;
    }
    return sum;
}

fn executeF32Binary(
    request: *const abi.Request,
    left_buffer: *const abi.BufferF32In,
    right_buffer: *const abi.BufferF32In,
    output_buffer: *abi.BufferF32Out,
) u32 {
    var left_len_u64 = request.dim0;
    var right_len_u64 = request.dim0;
    var output_len_u64 = request.dim0;
    if (request.operation == abi.operation_tensor_f32_matmul) {
        left_len_u64 = checkedMulU64(request.dim0, request.dim1) orelse return abi.status_dimension_overflow;
        right_len_u64 = checkedMulU64(request.dim1, request.dim2) orelse return abi.status_dimension_overflow;
        output_len_u64 = checkedMulU64(request.dim0, request.dim2) orelse return abi.status_dimension_overflow;
    } else if (request.dim1 != 0 or request.dim2 != 0) return abi.status_layout_mismatch;
    if (left_buffer.len != left_len_u64 or right_buffer.len != right_len_u64 or output_buffer.len != output_len_u64)
        return abi.status_length_mismatch;

    const left_len = std.math.cast(usize, left_len_u64) orelse return abi.status_dimension_overflow;
    const right_len = std.math.cast(usize, right_len_u64) orelse return abi.status_dimension_overflow;
    const output_len = std.math.cast(usize, output_len_u64) orelse return abi.status_dimension_overflow;
    if (left_len != 0 and left_buffer.data == null) return abi.status_null_pointer;
    if (right_len != 0 and right_buffer.data == null) return abi.status_null_pointer;
    if (output_len != 0 and output_buffer.data == null) return abi.status_null_pointer;
    const left_range = addressRange(left_buffer.data, left_len, @sizeOf(f32)) orelse return abi.status_dimension_overflow;
    const right_range = addressRange(right_buffer.data, right_len, @sizeOf(f32)) orelse return abi.status_dimension_overflow;
    const output_range = addressRange(output_buffer.data, output_len, @sizeOf(f32)) orelse return abi.status_dimension_overflow;
    if (output_range.overlaps(left_range) or output_range.overlaps(right_range)) return abi.status_aliasing_violation;

    const left = if (left_len == 0) &[_]f32{} else left_buffer.data.?[0..left_len];
    const right = if (right_len == 0) &[_]f32{} else right_buffer.data.?[0..right_len];
    for (left) |value| if (!std.math.isFinite(value)) return abi.status_non_finite_input;
    for (right) |value| if (!std.math.isFinite(value)) return abi.status_non_finite_input;

    if (request.operation == abi.operation_tensor_f32_matmul) {
        const m = std.math.cast(usize, request.dim0) orelse return abi.status_dimension_overflow;
        const k = std.math.cast(usize, request.dim1) orelse return abi.status_dimension_overflow;
        const n = std.math.cast(usize, request.dim2) orelse return abi.status_dimension_overflow;
        for (0..m) |row| for (0..n) |column| {
            _ = f32MatmulCell(row, column, k, n, left, right) orelse return abi.status_arithmetic_overflow;
        };
        if (output_len != 0) axiom_matmul.matmul(m, k, n, left, right, output_buffer.data.?[0..output_len]);
    } else {
        for (left, right) |left_value, right_value| {
            const result = if (request.operation == abi.operation_tensor_f32_add) left_value + right_value else left_value * right_value;
            if (!std.math.isFinite(result)) return abi.status_arithmetic_overflow;
        }
        if (output_len != 0) {
            const output = output_buffer.data.?[0..output_len];
            if (request.operation == abi.operation_tensor_f32_add)
                axiom_pointwise.add(left, right, output)
            else
                axiom_pointwise.mul(left, right, output);
        }
    }
    return abi.status_ok;
}

export fn enaction_accel_execute_f32_binary(
    request_pointer: ?*const abi.Request,
    left_pointer: ?*const abi.BufferF32In,
    right_pointer: ?*const abi.BufferF32In,
    output_pointer: ?*abi.BufferF32Out,
    evidence_pointer: ?*abi.Evidence,
) u32 {
    const request = request_pointer orelse return abi.status_null_pointer;
    const left = left_pointer orelse return abi.status_null_pointer;
    const right = right_pointer orelse return abi.status_null_pointer;
    const output = output_pointer orelse return abi.status_null_pointer;
    const evidence = evidence_pointer orelse return abi.status_null_pointer;
    const request_status = validateF32BinaryRequest(request);
    if (request_status != abi.status_ok) return request_status;
    const execution_status = executeF32Binary(request, left, right, output);
    if (execution_status != abi.status_ok) return execution_status;
    fillEvidence(request, abi.determinism_tolerance_bounded, evidence);
    return abi.status_ok;
}

fn requestFor(operation: u32, layout: u32, dim0: u64, dim1: u64, dim2: u64) abi.Request {
    return .{
        .abi_major = abi.abi_major,
        .abi_minor = abi.abi_minor,
        .operation_major = abi.operation_major,
        .operation_minor = abi.operation_minor,
        .operation = operation,
        .lane = abi.lane_authoritative,
        .minimum_support = abi.support_resilient,
        .minimum_determinism = abi.determinism_canonical_exact,
        .layout = layout,
        .reserved = 0,
        .dim0 = dim0,
        .dim1 = dim1,
        .dim2 = dim2,
    };
}

test "exact dot and evidence" {
    const left_data = [_]i32{ 2, -3, 4 };
    const right_data = [_]i32{ 5, 7, -2 };
    var output_data = [_]i64{777};
    const left = abi.BufferI32{ .data = &left_data, .len = left_data.len };
    const right = abi.BufferI32{ .data = &right_data, .len = right_data.len };
    var output = abi.BufferI64{ .data = &output_data, .len = output_data.len };
    var evidence: abi.Evidence = undefined;
    const request = requestFor(abi.operation_fixed_i32_dot, abi.layout_dot, 3, 0, 0);
    try std.testing.expectEqual(abi.status_ok, enaction_accel_execute(&request, &left, &right, &output, &evidence));
    try std.testing.expectEqual(@as(i64, -19), output_data[0]);
    try std.testing.expectEqual(abi.backend_zig_scalar, evidence.backend_id);
    try std.testing.expectEqual(abi.determinism_canonical_exact, evidence.determinism);
}

test "matrix overflow is output atomic" {
    const left_data = [_]i32{std.math.maxInt(i32)} ** 3;
    const right_data = [_]i32{ 1, std.math.maxInt(i32), 1, std.math.maxInt(i32), 1, std.math.maxInt(i32) };
    var output_data = [_]i64{ 71, 72 };
    const left = abi.BufferI32{ .data = &left_data, .len = left_data.len };
    const right = abi.BufferI32{ .data = &right_data, .len = right_data.len };
    var output = abi.BufferI64{ .data = &output_data, .len = output_data.len };
    var evidence = std.mem.zeroes(abi.Evidence);
    const request = requestFor(abi.operation_fixed_i32_matmul, abi.layout_matmul, 1, 3, 2);
    try std.testing.expectEqual(abi.status_arithmetic_overflow, enaction_accel_execute(&request, &left, &right, &output, &evidence));
    try std.testing.expectEqualSlices(i64, &[_]i64{ 71, 72 }, &output_data);
    try std.testing.expectEqual(@as(u32, 0), evidence.backend_id);
}

test "capabilities and malformed requests fail explicitly" {
    try std.testing.expectEqual(@as(u32, 0x00010000), enaction_accel_abi_version());
    try std.testing.expectEqual(@as(u32, 7), enaction_accel_capability_count());
    var found = std.mem.zeroes(abi.Capability);
    try std.testing.expectEqual(abi.status_ok, enaction_accel_capability_at(1, &found));
    try std.testing.expectEqual(abi.operation_fixed_i32_matmul, found.operation);
    try std.testing.expectEqual(abi.status_ok, enaction_accel_capability_at(3, &found));
    try std.testing.expectEqual(abi.operation_tensor_f32_relu6, found.operation);
    try std.testing.expectEqual(abi.determinism_tolerance_bounded, found.determinism);
    try std.testing.expectEqual(abi.status_ok, enaction_accel_capability_at(6, &found));
    try std.testing.expectEqual(abi.operation_tensor_f32_mul, found.operation);
    try std.testing.expectEqual(abi.status_index_out_of_range, enaction_accel_capability_at(7, &found));

    var request = requestFor(abi.operation_fixed_i32_dot, abi.layout_dot, 0, 0, 0);
    request.abi_major = 9;
    const empty = abi.BufferI32{ .data = null, .len = 0 };
    var output_data = [_]i64{91};
    var output = abi.BufferI64{ .data = &output_data, .len = 1 };
    var evidence = std.mem.zeroes(abi.Evidence);
    try std.testing.expectEqual(abi.status_unsupported_abi, enaction_accel_execute(&request, &empty, &empty, &output, &evidence));
    try std.testing.expectEqual(@as(i64, 91), output_data[0]);
}

test "Axiom-derived f32 relu family is advisory, bit-stable, and failure atomic" {
    const input_data = [_]f32{ -3.5, -0.0, 0.0, 2.25, 9.0 };
    const input = abi.BufferF32In{ .data = &input_data, .len = input_data.len };
    var output_data = [_]f32{91.0} ** input_data.len;
    var output = abi.BufferF32Out{ .data = &output_data, .len = output_data.len };
    var evidence = std.mem.zeroes(abi.Evidence);
    var request = requestFor(abi.operation_tensor_f32_relu, abi.layout_vector, input_data.len, 0, 0);
    request.lane = abi.lane_advisory;
    request.minimum_determinism = abi.determinism_tolerance_bounded;
    try std.testing.expectEqual(abi.status_ok, enaction_accel_execute_f32(&request, &input, &output, &evidence));
    try std.testing.expectEqualSlices(f32, &[_]f32{ 0.0, 0.0, 0.0, 2.25, 9.0 }, &output_data);
    try std.testing.expectEqual(abi.determinism_tolerance_bounded, evidence.determinism);

    request.operation = abi.operation_tensor_f32_relu6;
    try std.testing.expectEqual(abi.status_ok, enaction_accel_execute_f32(&request, &input, &output, &evidence));
    try std.testing.expectEqualSlices(f32, &[_]f32{ 0.0, 0.0, 0.0, 2.25, 6.0 }, &output_data);

    request.lane = abi.lane_authoritative;
    try std.testing.expectEqual(abi.status_unsupported_requirement, enaction_accel_execute_f32(&request, &input, &output, &evidence));

    const invalid_data = [_]f32{ 1.0, std.math.nan(f32), 3.0 };
    const invalid = abi.BufferF32In{ .data = &invalid_data, .len = invalid_data.len };
    var untouched_data = [_]f32{ 71.0, 72.0, 73.0 };
    var untouched = abi.BufferF32Out{ .data = &untouched_data, .len = untouched_data.len };
    request.lane = abi.lane_advisory;
    request.dim0 = invalid_data.len;
    try std.testing.expectEqual(abi.status_non_finite_input, enaction_accel_execute_f32(&request, &invalid, &untouched, &evidence));
    try std.testing.expectEqualSlices(f32, &[_]f32{ 71.0, 72.0, 73.0 }, &untouched_data);
}

test "Axiom-derived f32 binary family is correct and failure atomic" {
    const left_data = [_]f32{ 1, 2, 3, 4, 5, 6 };
    const right_data = [_]f32{ 7, 8, 9, 10, 11, 12 };
    const left = abi.BufferF32In{ .data = &left_data, .len = left_data.len };
    const right = abi.BufferF32In{ .data = &right_data, .len = right_data.len };
    var output_data = [_]f32{91} ** 4;
    var output = abi.BufferF32Out{ .data = &output_data, .len = output_data.len };
    var evidence = std.mem.zeroes(abi.Evidence);
    var request = requestFor(abi.operation_tensor_f32_matmul, abi.layout_matmul, 2, 3, 2);
    request.lane = abi.lane_advisory;
    request.minimum_determinism = abi.determinism_tolerance_bounded;
    try std.testing.expectEqual(abi.status_ok, enaction_accel_execute_f32_binary(&request, &left, &right, &output, &evidence));
    try std.testing.expectEqualSlices(f32, &[_]f32{ 58, 64, 139, 154 }, &output_data);

    const huge_data = [_]f32{std.math.floatMax(f32)};
    const two_data = [_]f32{2};
    const huge = abi.BufferF32In{ .data = &huge_data, .len = 1 };
    const two = abi.BufferF32In{ .data = &two_data, .len = 1 };
    var untouched_data = [_]f32{71};
    var untouched = abi.BufferF32Out{ .data = &untouched_data, .len = 1 };
    request = requestFor(abi.operation_tensor_f32_mul, abi.layout_vector, 1, 0, 0);
    request.lane = abi.lane_advisory;
    request.minimum_determinism = abi.determinism_tolerance_bounded;
    try std.testing.expectEqual(abi.status_arithmetic_overflow, enaction_accel_execute_f32_binary(&request, &huge, &two, &untouched, &evidence));
    try std.testing.expectEqual(@as(f32, 71), untouched_data[0]);
}

test "null dimensions and aliasing are refused without mutation" {
    var request = requestFor(abi.operation_fixed_i32_dot, abi.layout_dot, 1, 0, 0);
    const missing = abi.BufferI32{ .data = null, .len = 1 };
    const right_data = [_]i32{3};
    const right = abi.BufferI32{ .data = &right_data, .len = 1 };
    var output_data = [_]i64{81};
    var output = abi.BufferI64{ .data = &output_data, .len = 1 };
    var evidence = std.mem.zeroes(abi.Evidence);
    try std.testing.expectEqual(
        abi.status_null_pointer,
        enaction_accel_execute(&request, &missing, &right, &output, &evidence),
    );
    try std.testing.expectEqual(@as(i64, 81), output_data[0]);

    var aliased_storage = [_]i64{ 11, 12 };
    const aliased_left = abi.BufferI32{
        .data = @as(?[*]const i32, @ptrFromInt(@intFromPtr(&aliased_storage))),
        .len = 2,
    };
    const alias_right_data = [_]i32{ 2, 3 };
    const alias_right = abi.BufferI32{ .data = &alias_right_data, .len = 2 };
    var aliased_output = abi.BufferI64{ .data = &aliased_storage, .len = 1 };
    request.dim0 = 2;
    try std.testing.expectEqual(
        abi.status_aliasing_violation,
        enaction_accel_execute(
            &request,
            &aliased_left,
            &alias_right,
            &aliased_output,
            &evidence,
        ),
    );
    try std.testing.expectEqualSlices(i64, &[_]i64{ 11, 12 }, &aliased_storage);

    const empty = abi.BufferI32{ .data = null, .len = 0 };
    var empty_output = abi.BufferI64{ .data = null, .len = 0 };
    request = requestFor(
        abi.operation_fixed_i32_matmul,
        abi.layout_matmul,
        std.math.maxInt(u64),
        2,
        0,
    );
    try std.testing.expectEqual(
        abi.status_dimension_overflow,
        enaction_accel_execute(&request, &empty, &empty, &empty_output, &evidence),
    );
}
