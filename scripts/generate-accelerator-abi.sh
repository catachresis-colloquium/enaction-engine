#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
output_dir="${repo_root}/src/interface/generated/abi"
scratch_dir="$(mktemp -d)"
trap 'rm -rf "${scratch_dir}"' EXIT

cd "${repo_root}"
idris2_version="$(idris2 --version 2>&1)"
case "${idris2_version}" in
  *0.7.0*) ;;
  *) echo "generate-accelerator-abi: expected Idris2 0.7.0, got: ${idris2_version}" >&2; exit 1 ;;
esac
idris2 --build abi.ipkg
generator="${repo_root}/build/exec/accelerator-abi-gen"

"${generator}" c > "${scratch_dir}/enaction_accelerator.h"
"${generator}" zig > "${scratch_dir}/enaction_accelerator.zig"
"${generator}" rust > "${scratch_dir}/enaction_accelerator.rs"
"${generator}" julia > "${scratch_dir}/enaction_accelerator.jl"
"${generator}" symbols > "${scratch_dir}/SYMBOLS"
zig fmt "${scratch_dir}/enaction_accelerator.zig" >/dev/null
sha256sum abi.ipkg src/interface/Abi/Layout.idr \
  src/interface/Abi/Accelerator.idr src/interface/Abi/Generate.idr \
  > "${scratch_dir}/SOURCE.sha256"

if [ "${1:-}" = "--check" ]; then
  cmp "${scratch_dir}/enaction_accelerator.h" "${output_dir}/enaction_accelerator.h"
  cmp "${scratch_dir}/enaction_accelerator.zig" "${output_dir}/enaction_accelerator.zig"
  cmp "${scratch_dir}/enaction_accelerator.rs" "${output_dir}/enaction_accelerator.rs"
  cmp "${scratch_dir}/enaction_accelerator.jl" "${output_dir}/enaction_accelerator.jl"
  cmp "${scratch_dir}/SYMBOLS" "${output_dir}/SYMBOLS"
  cmp "${scratch_dir}/SOURCE.sha256" "${output_dir}/SOURCE.sha256"
else
  install -m 0644 "${scratch_dir}/enaction_accelerator.h" "${output_dir}/enaction_accelerator.h"
  install -m 0644 "${scratch_dir}/enaction_accelerator.zig" "${output_dir}/enaction_accelerator.zig"
  install -m 0644 "${scratch_dir}/enaction_accelerator.rs" "${output_dir}/enaction_accelerator.rs"
  install -m 0644 "${scratch_dir}/enaction_accelerator.jl" "${output_dir}/enaction_accelerator.jl"
  install -m 0644 "${scratch_dir}/SYMBOLS" "${output_dir}/SYMBOLS"
  install -m 0644 "${scratch_dir}/SOURCE.sha256" "${output_dir}/SOURCE.sha256"
fi
