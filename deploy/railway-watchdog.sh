#!/usr/bin/env bash
# Railway deployment watchdog — see issue #562.
#
# Queries Railway for the latest deployment status of SERVICE in ENVIRONMENT.
# On FAILED/CRASHED: opens a GitHub issue with build-log excerpt (idempotent
# per deployment ID — re-running with the same failed deployment is a no-op).
# On SUCCESS: closes any open issues labeled `railway-failure` (the next green
# deploy self-heals old notifications).
#
# Requires: railway CLI, gh CLI, jq, RAILWAY_TOKEN env, GH_TOKEN env.
# Intended to run from .github/workflows/railway-watchdog.yml on a schedule.

set -euo pipefail

SERVICE="${RAILWAY_SERVICE:-Parish}"
ENVIRONMENT="${RAILWAY_ENVIRONMENT:-production}"
PROJECT="${RAILWAY_PROJECT:-Rundale}"
LABEL="railway-failure"
LOG_LINES="${RAILWAY_LOG_LINES:-120}"

require() {
  command -v "$1" >/dev/null || { echo "missing dependency: $1" >&2; exit 2; }
}
require railway
require gh
require jq

# In CI both tokens are mandatory. Locally we fall back to whatever the
# `railway` and `gh` CLIs already have cached so an operator can dry-run.
if [ "${CI:-}" = "true" ]; then
  : "${RAILWAY_TOKEN:?RAILWAY_TOKEN must be set in CI}"
  : "${GH_TOKEN:?GH_TOKEN must be set in CI}"
fi

# Resolve and link the project/env so subsequent --service calls work. `link`
# writes to the current directory, which on Actions runners is scratch space.
railway link --project "$PROJECT" --environment "$ENVIRONMENT" --service "$SERVICE" >/dev/null

status_json=$(railway service status --service "$SERVICE" --environment "$ENVIRONMENT" --json)
deployment_id=$(jq -r '.deploymentId // empty' <<<"$status_json")
status=$(jq -r '.status // "UNKNOWN"' <<<"$status_json")

echo "service=$SERVICE env=$ENVIRONMENT deployment=$deployment_id status=$status"

case "$status" in
  FAILED|CRASHED)
    : # proceed to ensure-issue-exists below
    ;;
  SUCCESS)
    # Close every stale railway-failure issue. The current deploy is green,
    # so any older failure report is by definition resolved.
    mapfile -t stale < <(gh issue list --label "$LABEL" --state open --json number --jq '.[].number')
    for n in "${stale[@]:-}"; do
      [ -z "$n" ] && continue
      gh issue close "$n" --comment "Auto-closed: latest Parish deployment \`$deployment_id\` is SUCCESS."
      echo "closed stale issue #$n"
    done
    exit 0
    ;;
  BUILDING|DEPLOYING|QUEUED|INITIALIZING|REMOVING)
    echo "transient state, skipping"
    exit 0
    ;;
  *)
    echo "unhandled status '$status', skipping" >&2
    exit 0
    ;;
esac

if [ -z "$deployment_id" ]; then
  echo "no deploymentId in status payload; cannot dedupe" >&2
  exit 1
fi

# Idempotence: one issue per deployment ID. `gh issue list --search` matches
# on title+body, so the ID in the title is enough.
existing=$(gh issue list --label "$LABEL" --state open \
  --search "$deployment_id in:title" --json number --jq '.[0].number // empty')
if [ -n "$existing" ]; then
  echo "issue #$existing already tracks deployment $deployment_id"
  exit 0
fi

# Pick the log stream that actually contains the failure. FAILED is a build
# failure; CRASHED means the container started and exited at runtime, so the
# diagnostic signal is in deploy logs, not build logs.
case "$status" in
  CRASHED) log_flag="--deployment"; log_kind="deploy" ;;
  *)       log_flag="--build";      log_kind="build"  ;;
esac

log_tmp=$(mktemp)
trap 'rm -f "$log_tmp"' EXIT
if ! railway logs "$log_flag" --service "$SERVICE" --environment "$ENVIRONMENT" \
    "$deployment_id" -n "$LOG_LINES" >"$log_tmp" 2>&1; then
  echo "(warning: failed to fetch $log_kind logs)" >&2
fi

error_excerpt=$(grep -iE 'error|failed|cannot|panic|exit code' "$log_tmp" | tail -n 30 || true)
tail_excerpt=$(tail -n 30 "$log_tmp" || true)

title="railway: deployment $deployment_id $status"
body=$(cat <<EOF
Automated report from \`.github/workflows/railway-watchdog.yml\` — see #562.

- **Service:** \`$SERVICE\` (\`$ENVIRONMENT\`)
- **Deployment:** \`$deployment_id\`
- **Status:** \`$status\`
- **Detected:** $(date -u +%Y-%m-%dT%H:%M:%SZ)

### Error lines from $log_kind log

\`\`\`
${error_excerpt:-(no explicit error markers found — see tail below)}
\`\`\`

### Last $LOG_LINES lines of $log_kind log (tail)

\`\`\`
${tail_excerpt:-(log fetch failed)}
\`\`\`

---

**Next steps:** inspect the log excerpt above, push a fix or revert the offending commit, and the next green deploy will auto-close this issue.
EOF
)

issue_url=$(gh issue create \
  --title "$title" \
  --label "$LABEL,bug,requires-human" \
  --body "$body")
echo "opened $issue_url"
