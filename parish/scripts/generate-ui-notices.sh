#!/usr/bin/env bash
#
# Generate the UI third-party notice from the locked production dependency
# tree without ever exposing the checked-in destination to partial output.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
parish_dir="$(cd "$script_dir/.." && pwd)"
ui_dir="${PARISH_UI_NOTICES_UI_DIR:-$parish_dir/apps/ui}"
destination="${PARISH_UI_NOTICES_DESTINATION:-$parish_dir/THIRD_PARTY_NOTICES.ui.md}"
npm_bin="${PARISH_UI_NOTICES_NPM_BIN:-npm}"
npx_bin="${PARISH_UI_NOTICES_NPX_BIN:-npx}"
node_bin="${PARISH_UI_NOTICES_NODE_BIN:-node}"
generator="license-checker-rseidelsohn@4.4.2"

for required in package.json package-lock.json license-clarifications.json; do
    if [[ ! -f "$ui_dir/$required" ]]; then
        echo "generate-ui-notices: missing prerequisite $ui_dir/$required" >&2
        exit 1
    fi
done

for command_path in "$npm_bin" "$npx_bin" "$node_bin"; do
    if ! command -v "$command_path" >/dev/null 2>&1; then
        echo "generate-ui-notices: required command not found: $command_path" >&2
        exit 1
    fi
done

destination_dir="$(dirname "$destination")"
if [[ ! -d "$destination_dir" ]]; then
    echo "generate-ui-notices: destination directory does not exist: $destination_dir" >&2
    exit 1
fi

# Always materialize package-lock.json exactly. A merely present node_modules
# directory may be partial or stale, and license-checker otherwise exits 0 with
# incomplete output.
echo "Establishing locked UI dependencies..."
(
    cd "$ui_dir"
    "$npm_bin" ci
)

tmp_dir="$(mktemp -d "$destination_dir/.ui-notices.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT
dependency_tree="$tmp_dir/production-dependencies.json"
candidate="$tmp_dir/THIRD_PARTY_NOTICES.ui.md"

(
    cd "$ui_dir"
    "$npm_bin" ls --omit=dev --all --json >"$dependency_tree"
    "$npx_bin" --yes "$generator" \
        --production \
        --excludePrivatePackages \
        --clarificationsFile ./license-clarifications.json \
        --markdown \
        --out "$candidate"
)

# Validate both the Markdown envelope and coverage of the installed production
# tree. This rejects blank output, malformed/duplicate rows, and partial scans
# before the checked-in destination is touched.
"$node_bin" - "$dependency_tree" "$candidate" <<'NODE'
const fs = require('node:fs');

const [, , treePath, noticePath] = process.argv;
const tree = JSON.parse(fs.readFileSync(treePath, 'utf8'));
const content = fs.readFileSync(noticePath, 'utf8');
if (content.trim().length === 0) {
    throw new Error('generated UI notice is blank');
}

const lines = content.split(/\r?\n/).filter((line) => line.trim().length > 0);
const found = new Set();
for (const line of lines) {
    const match = line.match(/^- \[([^\]]+)\]\(.+\) - \S.*$/);
    if (!match) {
        throw new Error(`malformed UI notice row: ${line}`);
    }
    if (found.has(match[1])) {
        throw new Error(`duplicate UI notice row: ${match[1]}`);
    }
    found.add(match[1]);
}

const expected = new Set();
function collect(dependencies) {
    for (const [name, dependency] of Object.entries(dependencies ?? {})) {
        if (!dependency || typeof dependency !== 'object') {
            throw new Error(`installed dependency is missing a version: ${name}`);
        }
        // npm represents omitted optional/platform packages as empty objects.
        // They are not installed and therefore must not appear in the notice.
        if (Object.keys(dependency).length === 0) continue;
        if (!dependency.version) {
            throw new Error(`installed dependency is missing a version: ${name}`);
        }
        expected.add(`${name}@${dependency.version}`);
        collect(dependency.dependencies);
    }
}
collect(tree.dependencies);
if (expected.size === 0) {
    throw new Error('installed production dependency tree is empty');
}

const missing = [...expected].filter((dependency) => !found.has(dependency));
if (missing.length > 0) {
    throw new Error(`UI notice is missing installed dependencies: ${missing.join(', ')}`);
}
const unexpected = [...found].filter((dependency) => !expected.has(dependency));
if (unexpected.length > 0) {
    throw new Error(`UI notice contains uninstalled dependencies: ${unexpected.join(', ')}`);
}
NODE

package_count="$(grep -c '^- \[' "$candidate")"
chmod 0644 "$candidate"
echo "Validated $candidate ($package_count packages); replacing $destination atomically."
# Keep the same-filesystem rename as the final fallible operation: if any
# earlier command fails, the checked-in destination remains byte-identical.
mv -f "$candidate" "$destination"
