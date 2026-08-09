#!/usr/bin/env bash
set -euo pipefail

: "${GATED_RESULTS:?GATED_RESULTS must be set}"
: "${PLAYWRIGHT_WINDOWS_RESULT:?PLAYWRIGHT_WINDOWS_RESULT must be set}"
: "${RUNTIME_SUITE_REQUIRED:?RUNTIME_SUITE_REQUIRED must be set}"
: "${RUNTIME_SUITE_RESULT:?RUNTIME_SUITE_RESULT must be set}"

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

case "$RUNTIME_SUITE_REQUIRED" in
    true)
        if [[ "$RUNTIME_SUITE_RESULT" != "success" ]]; then
            echo "::error::runtime correctness suite was required but ended with '$RUNTIME_SUITE_RESULT'"
            status=1
        fi
        ;;
    false)
        if [[ "$RUNTIME_SUITE_RESULT" != "skipped" ]]; then
            echo "::error::runtime correctness suite was not required but ended with '$RUNTIME_SUITE_RESULT' instead of 'skipped'"
            status=1
        fi
        ;;
    *)
        echo "::error::RUNTIME_SUITE_REQUIRED must be 'true' or 'false', got '$RUNTIME_SUITE_REQUIRED'"
        status=1
        ;;
esac

if [[ "$status" -ne 0 ]]; then
    echo "CI gate: FAIL — a gated job failed, was cancelled, or was skipped unexpectedly."
    exit 1
fi

echo "CI gate: PASS — all gated jobs succeeded or were legitimately skipped."
