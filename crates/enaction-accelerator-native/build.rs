// SPDX-License-Identifier: AGPL-3.0-or-later

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let repository = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crate must remain under crates/");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let zig_source = repository.join("src/interface/ffi/src/accelerator.zig");
    let zig_abi = repository.join("src/interface/generated/abi/enaction_accelerator.zig");
    let library = output.join("libenaction_accelerator.a");

    println!("cargo:rerun-if-changed={}", zig_source.display());
    // Imported kernels are compilation inputs too; otherwise a warm Cargo
    // build can benchmark stale Zig code after either implementation changes.
    for imported in ["axiom_matmul.zig", "axiom_pointwise.zig"] {
        println!(
            "cargo:rerun-if-changed={}",
            zig_source.with_file_name(imported).display()
        );
    }
    println!("cargo:rerun-if-changed={}", zig_abi.display());
    println!(
        "cargo:rerun-if-changed={}",
        repository
            .join("src/interface/generated/abi/enaction_accelerator.rs")
            .display()
    );

    // `ZIG` overrides the toolchain path; the default is a plain PATH lookup
    // (the pin itself lives in mise.toml / .tool-versions: zig 0.16.0).
    let zig = env::var("ZIG").unwrap_or_else(|_| "zig".to_string());
    println!("cargo:rerun-if-env-changed=ZIG");
    let status = Command::new(&zig)
        .current_dir(repository)
        .env("ZIG_GLOBAL_CACHE_DIR", output.join("zig-global-cache"))
        .env("ZIG_LOCAL_CACHE_DIR", output.join("zig-local-cache"))
        .args([
            "build-lib",
            "-OReleaseSafe",
            "-static",
            "-fPIC",
            "-fcompiler-rt",
            "--dep",
            "abi",
            &format!("-Mroot={}", zig_source.display()),
            &format!("-Mabi={}", zig_abi.display()),
            &format!("-femit-bin={}", library.display()),
        ])
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "failed to launch Zig (`{zig}`): {e}. Install the pinned \
                 toolchain (zig 0.16.0 via mise.toml / .tool-versions) or set \
                 ZIG=/path/to/zig. In CI this crate builds only in the \
                 dedicated `accelerator` job of rust-ci.yml, which installs \
                 Zig; it is excluded from the workspace default-members for \
                 exactly this reason."
            )
        });
    assert!(
        status.success(),
        "pure-Zig accelerator library failed to build"
    );

    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=enaction_accelerator");
}
