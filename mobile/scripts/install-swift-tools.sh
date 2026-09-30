#!/usr/bin/env bash
# Install the pinned SwiftLint and SwiftFormat CLIs from mobile/tool-versions.toml.
# macOS only. Does not mutate app source.
set -euo pipefail

script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mobile_root="$(cd "$script_root/.." && pwd)"
versions_file="$mobile_root/tool-versions.toml"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "install-swift-tools.sh requires macOS (found $(uname -s))." >&2
    exit 1
fi

if ! command -v brew >/dev/null 2>&1; then
    echo "Homebrew is required to install SwiftLint and SwiftFormat." >&2
    exit 1
fi

read_version() {
    local key="$1"
    sed -n "s/^${key} *= *\"\\([^\"]*\\)\".*/\\1/p" "$versions_file" | head -n1
}

swiftlint_version="$(read_version swiftlint)"
swiftformat_version="$(read_version swiftformat)"

if [[ -z "$swiftlint_version" || -z "$swiftformat_version" ]]; then
    echo "Could not read pinned versions from $versions_file" >&2
    exit 1
fi

echo "Installing SwiftLint $swiftlint_version and SwiftFormat $swiftformat_version"
brew install "swiftlint@$swiftlint_version" 2>/dev/null \
    || brew install swiftlint
brew install "swiftformat@$swiftformat_version" 2>/dev/null \
    || brew install swiftformat

installed_lint="$(swiftlint version 2>/dev/null || true)"
installed_format="$(swiftformat --version 2>/dev/null || true)"
echo "swiftlint: ${installed_lint:-missing}"
echo "swiftformat: ${installed_format:-missing}"

if [[ "$installed_lint" != "$swiftlint_version" ]]; then
    echo "warning: expected SwiftLint $swiftlint_version, found ${installed_lint:-missing}" >&2
    echo "warning: Homebrew may have floated the bottle; record the drift in the PR." >&2
fi
if [[ "$installed_format" != "$swiftformat_version" ]]; then
    echo "warning: expected SwiftFormat $swiftformat_version, found ${installed_format:-missing}" >&2
    echo "warning: Homebrew may have floated the bottle; record the drift in the PR." >&2
fi
