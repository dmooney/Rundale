#!/usr/bin/env bash
set -euo pipefail

: "${GATED_RESULTS:?GATED_RESULTS must be set}"
: "${PLAYWRIGHT_WINDOWS_RESULT:?PLAYWRIGHT_WINDOWS_RESULT must be set}"
: "${RUNTIME_SUITE_REQUIRED:?RUNTIME_SUITE_REQUIRED must be set}"
: "${RUNTIME_SUITE_RESULT:?RUNTIME_SUITE_RESULT must be set}"
: "${DIFFERENTIAL_REQUIRED:?DIFFERENTIAL_REQUIRED must be set}"
: "${DIFFERENTIAL_RESULT:?DIFFERENTIAL_RESULT must be set}"

echo "gated job results: $GATED_RESULTS"
read -ra results <<<"$GATED_RESULTS"
status=0

for result in "${results[@]}"; do
    case "$result" in
        success | skipped) ;;
        *)
            echo "::error::a required CI job ended with '$result'"
            status=1
            ;;
    esac
done

if [[ "$PLAYWRIGHT_WINDOWS_RESULT" != "success" ]]; then
    echo "::error::Windows Playwright launcher lifecycle was required but ended with '$PLAYWRIGHT_WINDOWS_RESULT'"
    status=1
fi

# A conditional job must succeed when its path filter selected it and be
# skipped otherwise; anything else means the condition and the filter drifted.
check_conditional() {
    local label="$1" required="$2" result="$3"
    case "$required" in
        true)
            if [[ "$result" != "success" ]]; then
                echo "::error::$label was required but ended with '$result'"
                status=1
            fi
            ;;
        false)
            if [[ "$result" != "skipped" ]]; then
                echo "::error::$label was not required but ended with '$result' instead of 'skipped'"
                status=1
            fi
            ;;
        *)
            echo "::error::required flag for $label must be 'true' or 'false', got '$required'"
            status=1
            ;;
    esac
}

check_conditional "runtime correctness suite" "$RUNTIME_SUITE_REQUIRED" "$RUNTIME_SUITE_RESULT"
check_conditional "differential proof" "$DIFFERENTIAL_REQUIRED" "$DIFFERENTIAL_RESULT"

if [[ "$status" -ne 0 ]]; then
    echo "CI gate: FAIL — a gated job failed, was cancelled, or was skipped unexpectedly."
    exit 1
fi

echo "CI gate: PASS — all gated jobs succeeded or were legitimately skipped."
