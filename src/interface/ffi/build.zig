// SPDX-License-Identifier: MPL-2.0
// Copyright (c) Jonathan D.A. Jewell <j.d.a.jewell@open.ac.uk>
//! Pure-Zig FFI build: real library and real tests, with the Idris2-rendered
//! ABI declarations injected as a module.

const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const abi_module = b.addModule("enaction_accelerator_abi", .{
        .root_source_file = b.path("../generated/abi/enaction_accelerator.zig"),
        .target = target,
        .optimize = optimize,
    });
    const accelerator_module = b.addModule("enaction_accelerator", .{
        .root_source_file = b.path("src/accelerator.zig"),
        .target = target,
        .optimize = optimize,
    });
    accelerator_module.addImport("abi", abi_module);

    const library = b.addLibrary(.{
        .name = "enaction_accelerator",
        .root_module = accelerator_module,
    });
    b.installArtifact(library);

    // Julia and other dynamic hosts consume the same pure-Zig implementation.
    // This is a second linkage form, not a C shim or a separate kernel.
    const shared_abi_module = b.createModule(.{
        .root_source_file = b.path("../generated/abi/enaction_accelerator.zig"),
        .target = target,
        .optimize = optimize,
    });
    const shared_accelerator_module = b.createModule(.{
        .root_source_file = b.path("src/accelerator.zig"),
        .target = target,
        .optimize = optimize,
    });
    shared_accelerator_module.addImport("abi", shared_abi_module);
    const shared_library = b.addLibrary(.{
        .name = "enaction_accelerator_shared",
        .root_module = shared_accelerator_module,
        .linkage = .dynamic,
    });
    b.installArtifact(shared_library);

    const accelerator_tests = b.addTest(.{ .root_module = accelerator_module });
    const run_accelerator = b.addRunArtifact(accelerator_tests);
    const test_step = b.step("test", "Run the pure-Zig accelerator FFI tests");
    test_step.dependOn(&run_accelerator.step);
}
