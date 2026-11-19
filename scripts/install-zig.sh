#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Install the pinned Zig toolchain from ziglang.org, verified by SHA-256.
#
# This replaces the mlugg/setup-zig action. GitHub refused to start every
# workflow that used that action (startup_failure, zero jobs, since
# 2026-11-04), most likely because the repository's allowed-actions policy
# does not admit mlugg/*. A plain download needs no third-party action and
# no actions.lock entry.
#
# The version and hash are pinned here, once, for every workflow. The hash is
# the one ziglang.org publishes in https://ziglang.org/download/index.json for
# 0.16.0 x86_64-linux; it was checked against a fresh download on 2026-11-14.
# A mismatch fails the job: no unverified toolchain runs.
#
# Usage: bash scripts/install-zig.sh [DEST]
#   DEST defaults to $RUNNER_TEMP (or a temporary directory). When
#   $GITHUB_PATH is set, the Zig directory is appended to it.

set -euo pipefail

ZIG_VERSION="0.16.0"
ZIG_TARGET="x86_64-linux"
ZIG_SHA256="70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# One pin for the whole repository: .tool-versions and mise.toml must agree.
for pin_file in .tool-versions mise.toml; do
  if ! grep -Eq "^zig[[:space:]=\"]+${ZIG_VERSION//./\\.}\"?([[:space:]]|$)" "$ROOT/$pin_file"; then
    echo "install-zig: FATAL: $pin_file does not pin zig $ZIG_VERSION" >&2
    exit 1
  fi
done

arch="$(uname -m)"
if [[ "$arch" != "x86_64" ]]; then
  echo "install-zig: FATAL: only $ZIG_TARGET is pinned; this machine is $arch" >&2
  exit 1
fi

dest="${1:-${RUNNER_TEMP:-$(mktemp -d)}}"
mkdir -p "$dest"
tarball="zig-${ZIG_TARGET}-${ZIG_VERSION}.tar.xz"
url="https://ziglang.org/download/${ZIG_VERSION}/${tarball}"

curl -fsSL --retry 3 --retry-delay 5 -o "$dest/$tarball" "$url"
echo "${ZIG_SHA256}  $dest/$tarball" | sha256sum -c -
tar -xJf "$dest/$tarball" -C "$dest"
rm -f "$dest/$tarball"

zig_dir="$dest/zig-${ZIG_TARGET}-${ZIG_VERSION}"
reported="$("$zig_dir/zig" version)"
if [[ "$reported" != "$ZIG_VERSION" ]]; then
  echo "install-zig: FATAL: installed zig reports $reported, expected $ZIG_VERSION" >&2
  exit 1
fi

if [[ -n "${GITHUB_PATH:-}" ]]; then
  echo "$zig_dir" >>"$GITHUB_PATH"
fi
echo "install-zig: zig $reported at $zig_dir"
