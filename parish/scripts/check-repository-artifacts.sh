#!/usr/bin/env bash
# Validate generated-output paths, large tracked files, and documentation
# screenshot reachability. This script is intentionally dependency-free beyond
# Git and a platform SHA-256 utility so it runs in local hooks and GitHub CI.
set -euo pipefail

repo_root="${REPOSITORY_ARTIFACT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$repo_root"

policy_file="${REPOSITORY_ARTIFACT_POLICY:-parish/scripts/repository-artifact-exceptions.txt}"
hard_limit=$((8 * 1024 * 1024))
advisory_limit=$((2 * 1024 * 1024))
failed=0
tracked_count=0
large_exception_count=0
orphan_exception_count=0
advisory_count=0
advisory_file="$(mktemp)"
trap 'rm -f "$advisory_file"' EXIT

if [[ ! -f "$policy_file" ]]; then
    echo "repository-artifacts: missing policy file: $policy_file" >&2
    exit 1
fi

sha256_file() {
    local path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$path" | awk '{print $1}'
    else
        shasum -a 256 "$path" | awk '{print $1}'
    fi
}

policy_lookup() {
    local kind="$1"
    local path="$2"
    awk -F '|' -v kind="$kind" -v path="$path" '
        $0 !~ /^#/ && $1 == kind && $2 == path { print; found = 1 }
        END { if (!found) exit 1 }
    ' "$policy_file"
}

report_error() {
    local path="$1"
    local message="$2"
    echo "::error file=$path::$message" >&2
    failed=$((failed + 1))
}

is_screenshot_referenced() {
    local path="$1"
    local filename="${path##*/}"
    git grep -I -F -q "$filename" -- \
        '*.md' '*.html' '*.json' '*.jsonc' '*.yaml' '*.yml' '*.toml' \
        '*.ts' '*.tsx' '*.js' '*.jsx' '*.mjs' '*.svelte' \
        ':!parish/scripts/repository-artifact-exceptions.txt'
}

duplicate_records="$(
    awk -F '|' '
        $0 !~ /^#/ && NF > 0 {
            key = $1 "|" $2
            seen[key]++
            if (seen[key] == 2) print key
        }
    ' "$policy_file"
)"
if [[ -n "$duplicate_records" ]]; then
    while IFS= read -r record; do
        report_error "$policy_file" "duplicate exception record: $record"
    done <<<"$duplicate_records"
fi

while IFS= read -r -d '' path; do
    tracked_count=$((tracked_count + 1))

    case "$path" in
        bug-reports/*)
            report_error "$path" "bug-report screenshots belong in the stable bug-evidence release; keep runtime bundles under resolved user-data paths."
            ;;
        graphify-out/* | */graphify-out/*)
            report_error "$path" "Graphify output is generated; keep it local under an ignored graphify-out directory."
            ;;
        docs/graphics-v2/pipeline-experiments/*.png)
            report_error "$path" "Graphics V2 experiment PNGs are archived output; keep them untracked or promote an approved input outside pipeline-experiments."
            ;;
        parish/docs/screenshots/*)
            report_error "$path" "legacy parish/docs screenshots are retired; use Playwright baselines or a referenced docs/screenshots image."
            ;;
        docs/screenshots/quality-harness-static-ui.png | \
            bug-reports/2f5e408f-87a2-4a1e-8659-0de613aa5632.png | \
            bug-reports/a903ed0f-c926-41c5-b083-d2f02dba95cd.png | \
            parish/apps/ui/static/rundale/notebook-ui/scene-kilteevan-village.png | \
            parish/apps/ui/static/rundale/notebook-ui/scene-murphys-farm.png)
            report_error "$path" "this retired Wave 1 artifact must not be reintroduced."
            ;;
    esac

    [[ -f "$path" ]] || continue
    size="$(wc -c <"$path" | tr -d '[:space:]')"

    if ((size > advisory_limit)); then
        advisory_count=$((advisory_count + 1))
        printf '%s\t%s\n' "$size" "$path" >>"$advisory_file"
    fi

    if ((size > hard_limit)); then
        record="$(policy_lookup large "$path" || true)"
        if [[ -z "$record" ]]; then
            report_error "$path" "tracked file is $size bytes; files over $hard_limit bytes require an exact reviewed exception."
            continue
        fi

        IFS='|' read -r kind record_path expected_size expected_sha owner purpose extra <<<"$record"
        actual_sha="$(sha256_file "$path")"
        if [[ "$kind" != "large" || "$record_path" != "$path" || -n "${extra:-}" ||
            -z "$owner" || -z "$purpose" || "$expected_size" != "$size" ||
            "$expected_sha" != "$actual_sha" ]]; then
            report_error "$path" "large-file exception does not match its exact size/hash/owner/purpose."
        else
            large_exception_count=$((large_exception_count + 1))
        fi
    fi
done < <(git ls-files -z)

runtime_manifest="parish/apps/ui/static/rundale/notebook-ui/asset-manifest.json"
if [[ -f "$runtime_manifest" ]]; then
    for retired_scene in scene-kilteevan-village.png scene-murphys-farm.png; do
        if grep -Fq "$retired_scene" "$runtime_manifest"; then
            report_error "$runtime_manifest" "manifest still references retired scene plate $retired_scene."
        fi
    done
fi

while IFS= read -r -d '' screenshot; do
    if is_screenshot_referenced "$screenshot"; then
        continue
    fi

    record="$(policy_lookup orphan "$screenshot" || true)"
    if [[ -z "$record" ]]; then
        report_error "$screenshot" "documentation screenshot is not referenced by tracked source or documentation."
    else
        orphan_exception_count=$((orphan_exception_count + 1))
    fi
done < <(git ls-files -z 'docs/screenshots/*.png' 'docs/screenshots/**/*.png')

while IFS='|' read -r kind path expected_size expected_sha owner purpose extra; do
    [[ -z "$kind" || "$kind" == \#* ]] && continue

    if [[ "$kind" != "large" && "$kind" != "orphan" ]]; then
        report_error "$policy_file" "unknown exception kind '$kind' for $path."
        continue
    fi
    if [[ -z "$path" || -z "$expected_size" || -z "$expected_sha" || -z "$owner" ||
        -z "$purpose" || -n "${extra:-}" ]]; then
        report_error "$policy_file" "malformed exception record for ${path:-unknown path}."
        continue
    fi
    if ! git ls-files --error-unmatch -- "$path" >/dev/null 2>&1 || [[ ! -f "$path" ]]; then
        report_error "$policy_file" "stale exception references an untracked or missing file: $path."
        continue
    fi

    actual_size="$(wc -c <"$path" | tr -d '[:space:]')"
    actual_sha="$(sha256_file "$path")"
    if [[ "$actual_size" != "$expected_size" || "$actual_sha" != "$expected_sha" ]]; then
        report_error "$policy_file" "exception size/hash drifted for $path."
    fi
    if [[ "$kind" == "large" && "$actual_size" -le "$hard_limit" ]]; then
        report_error "$policy_file" "large-file exception is no longer needed for $path."
    fi
    if [[ "$kind" == "orphan" ]]; then
        if [[ "$path" != docs/screenshots/*.png && "$path" != docs/screenshots/**/*.png ]]; then
            report_error "$policy_file" "orphan exception is outside docs/screenshots: $path."
        elif is_screenshot_referenced "$path"; then
            report_error "$policy_file" "orphan exception is stale because $path now has a reference."
        fi
    fi
done <"$policy_file"

if ((advisory_count > 0)); then
    echo "::warning::$advisory_count tracked files exceed the 2 MiB advisory threshold; largest ten follow." >&2
    while IFS=$'\t' read -r size path; do
        echo "::warning file=$path::$size bytes" >&2
    done < <(sort -nr "$advisory_file" | sed -n '1,10p')
fi

if ((failed > 0)); then
    echo "repository-artifacts FAILED: $failed policy violation(s)." >&2
    exit 1
fi

echo "OK: repository artifact policy passed ($tracked_count tracked files; $large_exception_count large-file exceptions; $orphan_exception_count orphan exception; $advisory_count advisory files)."
