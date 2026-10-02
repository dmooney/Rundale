#!/usr/bin/env bash
# Operator access to the limerick-prod Endpoints deployment.
#
#   limerick-prod.sh definitions <verify|publish|export|replace>
#   limerick-prod.sh sql < query.sql          # prints result rows as JSON
#   limerick-prod.sh appcheck-token create <display-name>   # prints the token
#   limerick-prod.sh appcheck-token delete <display-name>
#
# The database credential is read from Secret Manager and never printed.
# Deployment settings (owner UID, allowed models) are read from the live
# Cloud Run service, so the script tracks the deployed configuration.
set -euo pipefail

project=limerick-prod
region=us-east1
service=limerick-endpoints
instance="$project:$region:limerick-endpoints-db"
organization=limerick-demo
app_id=1:877612517009:ios:586f98a2cc3e7d0c676130
proxy_port=55433
repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
server_dir="$repo_root/endpoints/apps/server"

usage() {
    sed -n '4,7p' "$0" | sed 's/^# //' >&2
    exit 2
}

service_env() {
    gcloud run services describe "$service" --project "$project" --region "$region" --format=json \
        | python3 -c 'import json,sys
d=json.load(sys.stdin)
for c in d["spec"]["template"]["spec"]["containers"]:
    for e in c.get("env",[]):
        if e["name"]==sys.argv[1] and "value" in e: print(e["value"])' "$1"
}

start_proxy() {
    proxy_log="$(mktemp -t limerick-prod-proxy)"
    cloud-sql-proxy "$instance" --port "$proxy_port" >"$proxy_log" 2>&1 &
    proxy_pid=$!
    trap 'kill "$proxy_pid" 2>/dev/null; rm -f "$proxy_log"' EXIT
    for _ in $(seq 1 30); do
        nc -z 127.0.0.1 "$proxy_port" 2>/dev/null && return 0
        sleep 0.5
    done
    echo "Cloud SQL Auth Proxy did not start:" >&2
    cat "$proxy_log" >&2
    exit 1
}

database_url() {
    gcloud secrets versions access latest --secret DATABASE_URL --project "$project" \
        | python3 -c 'import sys,urllib.parse as u
p=u.urlsplit(sys.stdin.read().strip())
print(u.urlunsplit((p.scheme,f"{p.username}:{p.password}@127.0.0.1:"+sys.argv[1],p.path,"","")))' "$proxy_port"
}

cmd_definitions() {
    case "${1:-}" in verify | publish | export | replace) ;; *) usage ;; esac
    local owner models
    owner="$(service_env LIMERICK_OWNER_FIREBASE_UID)"
    [[ -n $owner ]] || owner="$(service_env PARISH_OWNER_FIREBASE_UID)"
    models="$(service_env GOOGLE_ALLOWED_MODELS)"
    start_proxy
    cd "$repo_root/endpoints"
    DATABASE_URL="$(database_url)" LIMERICK_OWNER_FIREBASE_UID="$owner" \
    PROVIDER_MODE=live GOOGLE_ALLOWED_MODELS="$models" \
        pnpm -s definitions "$1" "$organization" ../mods/rundale/endpoints
}

cmd_sql() {
    local query
    query="$(cat)"
    start_proxy
    cd "$server_dir"
    DATABASE_URL="$(database_url)" QUERY="$query" node -e '
const { Client } = require("pg");
(async () => {
  const client = new Client({ connectionString: process.env.DATABASE_URL });
  await client.connect();
  try {
    const result = await client.query(process.env.QUERY);
    for (const r of [result].flat()) console.log(JSON.stringify({ command: r.command, rowCount: r.rowCount, rows: r.rows }, null, 2));
  } finally {
    await client.end();
  }
})().catch((error) => { console.error(error.message); process.exit(1); });'
}

appcheck_api() {
    local method=$1 path=$2 body=${3:-}
    local args=(-sS --fail-with-body -X "$method"
        -H "Authorization: Bearer $(gcloud auth print-access-token)"
        -H "x-goog-user-project: $project")
    [[ -n $body ]] && args+=(-H "Content-Type: application/json" -d "$body")
    curl "${args[@]}" "https://firebaseappcheck.googleapis.com/v1/projects/$project/apps/$app_id/debugTokens$path"
}

cmd_appcheck_token() {
    local action=${1:-} name=${2:-}
    [[ -n $name ]] || usage
    case "$action" in
        create)
            local token
            token="$(uuidgen | tr '[:upper:]' '[:lower:]')"
            appcheck_api POST "" "{\"displayName\":\"$name\",\"token\":\"$token\"}" >/dev/null
            echo "$token"
            ;;
        delete)
            local ids
            ids="$(appcheck_api GET "" | python3 -c 'import json,sys
for t in json.load(sys.stdin).get("debugTokens",[]):
    if t["displayName"]==sys.argv[1]: print(t["name"].rsplit("/",1)[1])' "$name")"
            [[ -n $ids ]] || {
                echo "No debug token named $name" >&2
                exit 1
            }
            for id in $ids; do appcheck_api DELETE "/$id" >/dev/null; done
            echo "Deleted debug token(s) named $name"
            ;;
        *) usage ;;
    esac
}

case "${1:-}" in
    definitions) shift && cmd_definitions "$@" ;;
    sql) shift && cmd_sql ;;
    appcheck-token) shift && cmd_appcheck_token "$@" ;;
    *) usage ;;
esac
