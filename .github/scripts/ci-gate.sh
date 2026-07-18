#!/usr/bin/env bash
set -euo pipefail

: "${GATED_RESULTS:?GATED_RESULTS must be set}"
: "${PLAYWRIGHT_WINDOWS_RESULT:?PLAYWRIGHT_WINDOWS_RESULT must be set}"
: "${UI_E2E_REQUIRED:?UI_E2E_REQUIRED must be set}"
: "${UI_E2E_RESULT:?UI_E2E_RESULT must be set}"

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

case "$UI_E2E_REQUIRED" in
    true)
        if [[ "$UI_E2E_RESULT" != "success" ]]; then
            echo "::error::UI Playwright was required but ended with '$UI_E2E_RESULT'"
            status=1
        fi
        ;;
    false)
        if [[ "$UI_E2E_RESULT" != "skipped" ]]; then
            echo "::error::UI Playwright was not required but ended with '$UI_E2E_RESULT' instead of 'skipped'"
            status=1
        fi
        ;;
    *)
        echo "::error::UI_E2E_REQUIRED must be 'true' or 'false', got '$UI_E2E_REQUIRED'"
        status=1
        ;;
esac

if [[ "$status" -ne 0 ]]; then
    echo "CI gate: FAIL — a gated job failed, was cancelled, or was skipped unexpectedly."
    exit 1
fi

echo "CI gate: PASS — all gated jobs succeeded or were legitimately skipped."
