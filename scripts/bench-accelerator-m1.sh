#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
mode="${1:-scalar}"
output="${2:-target/m1-results}"
samples="${3:-101}"
rounds="${4:-3}"
[[ "$#" -le 4 && "$samples" =~ ^[1-9][0-9]*$ && "$rounds" =~ ^[1-9][0-9]*$ ]] || {
    echo "usage: $0 scalar|compare output-directory samples(1..10000) rounds(1..20)" >&2; exit 2;
}
[[ "$samples" -le 10000 && "$rounds" -le 20 ]] || exit 2
[[ "$mode" == scalar || "$mode" == compare ]] || { echo "unknown mode: $mode" >&2; exit 2; }
for tool in cargo rustc jq sha256sum git; do command -v "$tool" >/dev/null; done
[[ ! -e "$output" ]] || { echo "refusing to overwrite results: $output" >&2; exit 2; }
zig="${ZIG:-zig}"
if [[ "$mode" == compare ]]; then
    [[ "$("$zig" version)" == 0.16.0 ]] || { echo "comparison requires Zig 0.16.0" >&2; exit 1; }
fi
mkdir -p "$output"
output="$(cd "$output" && pwd)"
{
    date -u +%FT%TZ
    git rev-parse HEAD
    git status --short
    rustc -Vv
    rustc --print cfg
    cargo -V
    uname -srm
    printf 'mode=%s samples=%s rounds=%s\n' "$mode" "$samples" "$rounds"
    printf 'RUSTFLAGS=%s\nCARGO_ENCODED_RUSTFLAGS=%s\nCARGO_BUILD_TARGET=%s\n' \
        "${RUSTFLAGS-}" "${CARGO_ENCODED_RUSTFLAGS-}" "${CARGO_BUILD_TARGET-}"
    if [[ -r /proc/cpuinfo ]]; then
        awk '/model name/ { print; exit }' /proc/cpuinfo
        awk '/Cpus_allowed_list/ { print }' /proc/self/status
    fi
    if [[ "$mode" == compare ]]; then
        "$zig" version
        "$zig" targets | sed -n '/    \.native = /,$p'
    fi
} > "$output/environment.txt"
# Lock the actual implementation, ABI authority, fixture manifest and harness,
# including dirty working-tree content. The commit alone is insufficient.
sha256sum Cargo.toml Cargo.lock crates/enaction-accelerator/{Cargo.toml,src/lib.rs,src/version.rs} \
    crates/enaction-accelerator-native/{Cargo.toml,src/lib.rs,build.rs} \
    src/interface/Abi/*.idr src/interface/generated/abi/*.{rs,zig,h,jl} \
    src/interface/generated/abi/SOURCE.sha256 src/interface/ffi/src/*.zig \
    conformance/accelerator/v1/SHA256SUMS verification/benchmarks/m1/experiment.rs \
    crates/enaction-accelerator/examples/m1_matmul.rs \
    crates/enaction-accelerator-native/examples/m1_matmul_native.rs \
    scripts/bench-accelerator-m1.sh > "$output/SOURCE-SHA256SUMS"
(cd conformance/accelerator/v1 && sha256sum -c SHA256SUMS) > "$output/conformance-digests.txt"
sha256sum -c src/interface/generated/abi/SOURCE.sha256 >> "$output/conformance-digests.txt"
target_dir="$(cargo metadata --locked --format-version 1 --no-deps | jq -er '.target_directory')"
# Explicit --target fixes binary paths even with a Cargo build.target setting.
host="$(rustc -vV | awk '/^host:/ {print $2}')"
cargo test --locked -p enaction-accelerator --all-targets > "$output/rust-tests.txt" 2>&1
cargo build --locked --release --target "$host" -p enaction-accelerator --example m1_matmul > "$output/build.txt" 2>&1
binaries=(m1_matmul)
if [[ "$mode" == compare ]]; then
    ZIG="$zig" cargo test --locked -p enaction-accelerator-native --all-targets > "$output/native-tests.txt" 2>&1
    ZIG="$zig" cargo build --locked --release --target "$host" -p enaction-accelerator-native --example m1_matmul_native >> "$output/build.txt" 2>&1
    binaries+=(m1_matmul_native)
    nm -g --defined-only "$target_dir/$host/release/examples/m1_matmul_native" \
        | awk '$3 ~ /^enaction_accel_/ {print}' > "$output/native-symbols.txt"
    [[ -s "$output/native-symbols.txt" ]] || { echo "native symbols absent" >&2; exit 1; }
fi
for ((round=0; round<rounds; round++)); do
    order=("${binaries[@]}")
    if [[ "$mode" == compare && $((round % 2)) -eq 1 ]]; then order=(m1_matmul_native m1_matmul); fi
    for name in "${order[@]}"; do
        binary="$target_dir/$host/release/examples/$name"
        sha256sum "$binary" >> "$output/BINARY-SHA256SUMS"
        "$binary" "$samples" > "$output/$name-$round.jsonl" 2> "$output/$name-$round.stderr.txt"
    done
done
jq -s '[.[] | select(.kind == "summary" or .kind == "memory")]' "$output"/*.jsonl > "$output/summaries.json"
(cd "$output" && sha256sum ./* > SHA256SUMS)
printf 'Results: %s\n' "$output"
