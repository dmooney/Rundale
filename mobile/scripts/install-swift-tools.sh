#!/usr/bin/env bash
# Install the pinned SwiftLint and SwiftFormat release binaries into
# mobile/.build/swift-tools/bin. Each archive is checked against its SHA-256
# before it is unpacked; a mismatch fails the install. Rerunning is a no-op
# while the installed pins match. `just mobile-verify --fast` runs this first.
# Pinned for macOS and x86_64 Linux (CI's container); other hosts fail here.
set -euo pipefail

# To bump a tool, change its version and every platform's digest together.
# The digest is the release asset's SHA-256 as GitHub lists it:
#   gh release view <tag> -R realm/SwiftLint --json assets
swiftlint_version="0.65.1"
swiftformat_version="0.63.1"
swiftlint_release="https://github.com/realm/SwiftLint/releases/download/${swiftlint_version}"
swiftformat_release="https://github.com/nicklockwood/SwiftFormat/releases/download/${swiftformat_version}"

# Each asset: URL, SHA-256, and the binary's name inside the archive.
case "$(uname -s)-$(uname -m)" in
    Darwin-*)
        swiftlint_asset=("$swiftlint_release/portable_swiftlint.zip" c1e429b0599cf1b516f369a2d9ec04eaf0e436f3c12b637df8851fa52ff694d0 swiftlint)
        swiftformat_asset=("$swiftformat_release/swiftformat.zip" 385ef1a263ba28685157b98c5536b9c9105e124518f28b7ef8a2bee4b167eaeb swiftformat)
        ;;
    Linux-x86_64)
        # CI's Linux Swift lane (.github/workflows/ci.yml, swift-quality).
        swiftlint_asset=("$swiftlint_release/swiftlint_linux_amd64.zip" caeed6f4a679c35539ffaf124f6c4ab4a8416917f7d8796279dc52b74026059d swiftlint)
        swiftformat_asset=("$swiftformat_release/swiftformat_linux.zip" 9e10090e0213378c68cedb42adc2c6461916a9c8e457cf452b5585b84096c333 swiftformat_linux)
        ;;
    *)
        echo "install-swift-tools.sh has no pinned tools for $(uname -s) $(uname -m)" >&2
        exit 1
        ;;
esac

sha256() {
    if command -v shasum >/dev/null; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        sha256sum "$1" | cut -d ' ' -f 1
    fi
}

mobile_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tools_root="$mobile_root/.build/swift-tools"
bin_dir="$tools_root/bin"
mkdir -p "$bin_dir"

install_tool() {
    local name="$1" version="$2" url="$3" sha256="$4" member="$5"
    local stamp="$tools_root/$name.sha256"
    if [[ -x "$bin_dir/$name" && -f "$stamp" && "$(cat "$stamp")" == "$sha256" ]]; then
        echo "$name $version already installed"
        return
    fi
    local scratch
    scratch="$(mktemp -d)"
    curl --fail --silent --show-error --location --retry 3 --output "$scratch/$name.zip" "$url"
    local actual
    actual="$(sha256 "$scratch/$name.zip")"
    if [[ "$actual" != "$sha256" ]]; then
        echo "$name $version digest mismatch: expected $sha256, got $actual" >&2
        rm -rf "$scratch"
        exit 1
    fi
    unzip -q -o "$scratch/$name.zip" -d "$scratch/unpacked"
    install -m 0755 "$scratch/unpacked/$member" "$bin_dir/$name"
    echo "$sha256" >"$stamp"
    rm -rf "$scratch"
    echo "$name $version installed"
}

install_tool swiftlint "$swiftlint_version" "${swiftlint_asset[@]}"
install_tool swiftformat "$swiftformat_version" "${swiftformat_asset[@]}"

installed_lint="$("$bin_dir/swiftlint" version)"
installed_format="$("$bin_dir/swiftformat" --version)"
if [[ "$installed_lint" != "$swiftlint_version" || "$installed_format" != "$swiftformat_version" ]]; then
    echo "installed versions do not match the pins: swiftlint $installed_lint, swiftformat $installed_format" >&2
    exit 1
fi
