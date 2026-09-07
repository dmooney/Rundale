#!/usr/bin/env bash
set -euo pipefail

spike_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
build_root="$spike_root/build"
target_root="$spike_root/target"

mkdir -p "$build_root/module-cache"

# The repository pins Rust 1.98.0, while the Homebrew rustc/cargo on PATH is
# 1.95.0. Keep the spike reproducible and avoid the shared sccache path in the
# host environment; this script does not install targets or invoke Xcode.
CARGO_TARGET_DIR="$target_root" RUSTC_WRAPPER= \
    rustup run 1.98.0 cargo build --manifest-path "$spike_root/Cargo.toml"

swiftc \
    -swift-version 6 \
    -warnings-as-errors \
    -parse-as-library \
    -module-cache-path "$build_root/module-cache" \
    -I "$spike_root/include" \
    -L "$target_root/debug" \
    -lrundale_binding_spike \
    "$spike_root/swift/ContractHarness.swift" \
    -o "$build_root/binding-spike-harness"

"$build_root/binding-spike-harness"
