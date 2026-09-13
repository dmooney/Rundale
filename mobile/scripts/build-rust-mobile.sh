#!/usr/bin/env bash
set -euo pipefail

script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_root/../.." && pwd)"
parish_root="$repo_root/parish"
ffi_header_root="$repo_root/parish/crates/parish-mobile-ffi/include"
build_root="${RundaleRustBuildRoot:-$repo_root/mobile/.build/rust-mobile}"
target_root="$build_root/cargo-target"
headers_root="$build_root/headers"
xcframework_root="$build_root/ParishMobileFFI.xcframework"
rust_toolchain="${RundaleRustToolchain:-1.98.0}"
minimum_ios="${RundaleMinimumIOS:-17.0}"
rustc_path=""

device_target="aarch64-apple-ios"
simulator_target="aarch64-apple-ios-sim"

if ! command -v rustup >/dev/null 2>&1; then
    echo "rustup is required to build ParishMobileFFI" >&2
    exit 1
fi
if ! rustup run "$rust_toolchain" rustc --version >/dev/null 2>&1; then
    echo "Rust toolchain $rust_toolchain is not installed; install it before running this script" >&2
    exit 1
fi
rustc_path="$(rustup which --toolchain "$rust_toolchain" rustc)"
for required_target in "$device_target" "$simulator_target"; do
    if ! rustup target list --toolchain "$rust_toolchain" --installed | grep -Fxq "$required_target"; then
        echo "Rust target $required_target is not installed for $rust_toolchain; install it before running this script" >&2
        exit 1
    fi
done
if ! command -v xcodebuild >/dev/null 2>&1; then
    echo "xcodebuild is required to package ParishMobileFFI.xcframework" >&2
    exit 1
fi

mkdir -p "$build_root" "$headers_root"
cp "$ffi_header_root/parish_mobile_ffi.h" "$headers_root/"
# RundaleBridge supplies the Clang module declarations. Publishing a second
# module map beside the binary makes Xcode discover that module twice.
rm -f "$headers_root/module.modulemap"

build_target() {
    local target="$1"
    local minimum_flag="$2"
    echo "Building parish-mobile-ffi for $target with Rust $rust_toolchain"
    CARGO_TARGET_DIR="$target_root" \
        RUSTC_WRAPPER='' \
        RUSTC="$rustc_path" \
        RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=$minimum_flag" \
        rustup run "$rust_toolchain" cargo build \
        --manifest-path "$parish_root/Cargo.toml" \
        --package parish-mobile-ffi \
        --no-default-features \
        --features engine-api \
        --target "$target" \
        --release \
        --locked
}

build_target "$device_target" "-mios-version-min=$minimum_ios"
build_target "$simulator_target" "-mios-simulator-version-min=$minimum_ios"

rm -rf "$xcframework_root"
xcodebuild -create-xcframework \
    -library "$target_root/$device_target/release/libparish_mobile_ffi.a" \
    -headers "$headers_root" \
    -library "$target_root/$simulator_target/release/libparish_mobile_ffi.a" \
    -headers "$headers_root" \
    -output "$xcframework_root"

echo "Created $xcframework_root"
