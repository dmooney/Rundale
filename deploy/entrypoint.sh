#!/usr/bin/env bash
# deploy/entrypoint.sh
#
# PR environments:  creates a per-PR Cloudflare Named Tunnel via API, then
#                   runs cloudflared + parish concurrently.
# Production:       execs parish directly (no tunnel).
#
# Required Railway service variables (PR mode only):
#   CF_API_TOKEN   – Cloudflare API token (Tunnel:Edit + Zone DNS:Edit)
#   CF_ACCOUNT_ID  – Cloudflare account ID
#   CF_ZONE_ID     – Zone ID for your domain
#   PREVIEW_DOMAIN – e.g. "preview.yourdomain.com"
#
# Railway supplies automatically:
#   RAILWAY_ENVIRONMENT – named "pr-<N>" for PR environments
#   PORT                – port parish should listen on
set -euo pipefail

APP_PORT="${PORT:-3001}"
CF_API="https://api.cloudflare.com/client/v4"
LOG="[entrypoint]"

# ── Derive PR number ───────────────────────────────────────────────────────────
# Checks RAILWAY_PR_NUMBER (explicit override) first, then parses
# RAILWAY_ENVIRONMENT (Railway names PR envs "pr-<N>"), then RAILWAY_GIT_BRANCH.
pr_number() {
    [[ -n "${RAILWAY_PR_NUMBER:-}" ]] && { printf '%s' "$RAILWAY_PR_NUMBER"; return; }

    local env="${RAILWAY_ENVIRONMENT:-}"
    [[ "$env" =~ ^[Pp][Rr]-([0-9]+)$ ]] && { printf '%s' "${BASH_REMATCH[1]}"; return; }

    local branch="${RAILWAY_GIT_BRANCH:-}"
    [[ "$branch" =~ [Pp][Rr][/-]([0-9]+) ]] && { printf '%s' "${BASH_REMATCH[1]}"; return; }

    printf ''
}

PR_NUM="$(pr_number)"

# ── PR mode: one named tunnel per PR (isolated routing) ───────────────────────
if [[ -n "$PR_NUM" && -n "${CF_API_TOKEN:-}" ]]; then
    : "${CF_ACCOUNT_ID:?CF_ACCOUNT_ID must be set in Railway service variables}"
    : "${CF_ZONE_ID:?CF_ZONE_ID must be set in Railway service variables}"
    : "${PREVIEW_DOMAIN:?PREVIEW_DOMAIN must be set (e.g. preview.yourdomain.com)}"

    TUNNEL_NAME="parish-pr-${PR_NUM}"
    HOSTNAME="pr-${PR_NUM}.${PREVIEW_DOMAIN}"

    echo "$LOG PR #${PR_NUM} — configuring tunnel '${TUNNEL_NAME}' → ${HOSTNAME}"

    # Helper: Cloudflare API call with bearer auth
    cf_api() { curl -fsSL -H "Authorization: Bearer ${CF_API_TOKEN}" "$@"; }

    # ── Get or create the named tunnel ────────────────────────────────────────
    TUNNEL_ID="$(cf_api \
        "${CF_API}/accounts/${CF_ACCOUNT_ID}/cfd_tunnel?name=${TUNNEL_NAME}&is_deleted=false" \
        | jq -r '.result[0].id // empty')"

    if [[ -n "$TUNNEL_ID" ]]; then
        echo "$LOG Reusing existing tunnel ${TUNNEL_ID}"
        TUNNEL_TOKEN="$(cf_api \
            "${CF_API}/accounts/${CF_ACCOUNT_ID}/cfd_tunnel/${TUNNEL_ID}/token" \
            | jq -r '.result')"
    else
        echo "$LOG Creating new tunnel ${TUNNEL_NAME}"
        RESP="$(cf_api -X POST \
            -H "Content-Type: application/json" \
            -d "{\"name\":\"${TUNNEL_NAME}\",\"config_src\":\"cloudflare\"}" \
            "${CF_API}/accounts/${CF_ACCOUNT_ID}/cfd_tunnel")"
        TUNNEL_ID="$(echo "$RESP" | jq -r '.result.id')"
        TUNNEL_TOKEN="$(echo "$RESP" | jq -r '.result.token')"

        # Configure remotely-managed ingress: this hostname → localhost
        cf_api -X PUT \
            -H "Content-Type: application/json" \
            -d "{\"config\":{\"ingress\":[
                  {\"hostname\":\"${HOSTNAME}\",\"service\":\"http://localhost:${APP_PORT}\"},
                  {\"service\":\"http_status:404\"}
                ]}}" \
            "${CF_API}/accounts/${CF_ACCOUNT_ID}/cfd_tunnel/${TUNNEL_ID}/configurations" \
            > /dev/null
        echo "$LOG Ingress rules configured"
    fi

    # ── Upsert DNS CNAME (idempotent — runs on every deploy) ─────────────────
    CNAME_TARGET="${TUNNEL_ID}.cfargotunnel.com"
    DNS_ID="$(cf_api \
        "${CF_API}/zones/${CF_ZONE_ID}/dns_records?name=pr-${PR_NUM}.${PREVIEW_DOMAIN}&type=CNAME" \
        | jq -r '.result[0].id // empty')"

    if [[ -n "$DNS_ID" ]]; then
        cf_api -X PATCH \
            -H "Content-Type: application/json" \
            -d "{\"content\":\"${CNAME_TARGET}\"}" \
            "${CF_API}/zones/${CF_ZONE_ID}/dns_records/${DNS_ID}" > /dev/null
    else
        cf_api -X POST \
            -H "Content-Type: application/json" \
            -d "{\"type\":\"CNAME\",\"name\":\"pr-${PR_NUM}.${PREVIEW_DOMAIN}\",\"content\":\"${CNAME_TARGET}\",\"proxied\":true,\"ttl\":1}" \
            "${CF_API}/zones/${CF_ZONE_ID}/dns_records" > /dev/null
    fi
    echo "$LOG DNS: ${HOSTNAME} → ${CNAME_TARGET}"

    # ── Start cloudflared ──────────────────────────────────────────────────────
    cloudflared tunnel --no-autoupdate run --token "$TUNNEL_TOKEN" &
    CF_PID=$!
    echo "$LOG cloudflared started (PID ${CF_PID})"

    # ── Start Parish ───────────────────────────────────────────────────────────
    ./parish --web "$APP_PORT" &
    APP_PID=$!
    echo "$LOG parish started (PID ${APP_PID})"

    # ── Supervise: kill both if either exits, triggering a Railway restart ─────
    _cleanup() {
        echo "$LOG Shutting down"
        kill "$CF_PID" "$APP_PID" 2>/dev/null || true
    }
    trap _cleanup EXIT INT TERM

    # wait -n: returns when the first of the listed PIDs exits (bash 4.3+)
    wait -n "$CF_PID" "$APP_PID"
    echo "$LOG A child process exited — triggering container restart"
    exit 1

else
    if [[ -n "$PR_NUM" && -z "${CF_API_TOKEN:-}" ]]; then
        echo "$LOG WARNING: PR #${PR_NUM} detected but CF_API_TOKEN not set — running without tunnel"
    else
        echo "$LOG Production mode — starting parish directly"
    fi
    exec ./parish --web "$APP_PORT"
fi
