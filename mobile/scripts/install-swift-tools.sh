#!/usr/bin/env bash
# Install the pinned SwiftLint and SwiftFormat release binaries into
# mobile/.build/swift-tools/bin. Each archive is checked against its SHA-256
# before it is unpacked; a mismatch fails the install. Rerunning is a no-op
# while the installed pins match. `just mobile-verify` runs this first.
set -euo pipefail

# To bump a tool, change its version and digest together. The digest is the
# release asset's SHA-256 as GitHub lists it:
#   gh release view <tag> -R realm/SwiftLint --json assets
swiftlint_version="0.65.1"
swiftlint_url="https://github.com/realm/SwiftLint/releases/download/${swiftlint_version}/portable_swiftlint.zip"
swiftlint_sha256="c1e429b0599cf1b516f369a2d9ec04eaf0e436f3c12b637df8851fa52ff694d0"
swiftformat_version="0.63.1"
swiftformat_url="https://github.com/nicklockwood/SwiftFormat/releases/download/${swiftformat_version}/swiftformat.zip"
swiftformat_sha256="385ef1a263ba28685157b98c5536b9c9105e124518f28b7ef8a2bee4b167eaeb"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "install-swift-tools.sh needs macOS; found $(uname -s)" >&2
    exit 1
fi

mobile_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tools_root="$mobile_root/.build/swift-tools"
bin_dir="$tools_root/bin"
mkdir -p "$bin_dir"

install_tool() {
    local name="$1" version="$2" url="$3" sha256="$4"
    local stamp="$tools_root/$name.sha256"
    if [[ -x "$bin_dir/$name" && -f "$stamp" && "$(cat "$stamp")" == "$sha256" ]]; then
        echo "$name $version already installed"
        return
    fi
    local scratch
    scratch="$(mktemp -d)"
    curl --fail --silent --show-error --location --retry 3 --output "$scratch/$name.zip" "$url"
    local actual
    actual="$(shasum -a 256 "$scratch/$name.zip" | cut -d ' ' -f 1)"
    if [[ "$actual" != "$sha256" ]]; then
        echo "$name $version digest mismatch: expected $sha256, got $actual" >&2
        rm -rf "$scratch"
        exit 1
    fi
    unzip -q -o "$scratch/$name.zip" -d "$scratch/unpacked"
    install -m 0755 "$scratch/unpacked/$name" "$bin_dir/$name"
    echo "$sha256" >"$stamp"
    rm -rf "$scratch"
    echo "$name $version installed"
}

install_tool swiftlint "$swiftlint_version" "$swiftlint_url" "$swiftlint_sha256"
install_tool swiftformat "$swiftformat_version" "$swiftformat_url" "$swiftformat_sha256"

installed_lint="$("$bin_dir/swiftlint" version)"
installed_format="$("$bin_dir/swiftformat" --version)"
if [[ "$installed_lint" != "$swiftlint_version" || "$installed_format" != "$swiftformat_version" ]]; then
    echo "installed versions do not match the pins: swiftlint $installed_lint, swiftformat $installed_format" >&2
    exit 1
fi
