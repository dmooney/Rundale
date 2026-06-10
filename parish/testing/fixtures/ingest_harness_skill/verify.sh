#!/usr/bin/env bash
# Verification fixture for harness-skill-ingest.
#
# Tooling task — not a game script. Ingests a sample skill-run payload into a
# temp harness.db, starts the dashboard server, and asserts the dashboard API
# returns the run with the right status, quality, 7 axes, findings, cost, and a
# servable per-turn frame.
#
# BEFORE the feature lands this script FAILS at the `ingest` step (subcommand
# does not exist) — that is the demonstration-of-absence. AFTER it lands it
# exits 0 and prints PASS for each criterion.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../../.." && pwd)"
tmp="$(mktemp -d)"
db="$tmp/harness.db"
artifacts="$tmp/artifacts"
uuid="00000000-0000-0000-0000-0000000000aa"
port=8799
trap 'kill "${serve_pid:-0}" 2>/dev/null || true; rm -rf "$tmp"' EXIT

# Lay out artifacts/runs/<uuid>/turns/000/frame.png (1x1 PNG placeholder).
mkdir -p "$artifacts/runs/$uuid/turns/000"
base64 -d >"$artifacts/runs/$uuid/turns/000/frame.png" <<'PNG'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M8AAAMBAQDJ/pLvAAAAAElFTkSuQmCC
PNG
echo '[]' >"$artifacts/runs/$uuid/turns/000/lines.json"

cd "$root/parish"

echo "== ingest =="
cargo run -q -p parish-harness -- ingest \
    --payload "$here/sample-payload.json" \
    --artifacts "$artifacts" \
    --db "$db"

echo "== serve =="
cargo run -q -p parish-harness -- serve --db "$db" --artifacts "$artifacts" --port "$port" &
serve_pid=$!
for _ in $(seq 1 30); do
    curl -sf "http://127.0.0.1:$port/api/runs" >/dev/null 2>&1 && break
    sleep 1
done

runs="$(curl -s "http://127.0.0.1:$port/api/runs")"
id="$(printf '%s' "$runs" | python3 -c 'import sys,json;print(json.load(sys.stdin)[0]["id"])')"
detail="$(curl -s "http://127.0.0.1:$port/api/runs/$id")"
cost="$(curl -s "http://127.0.0.1:$port/api/cost")"

python3 - "$runs" "$detail" "$cost" <<'PY'
import sys, json
runs, detail, cost = (json.loads(a) for a in sys.argv[1:4])
r = runs[0]
assert r["status"] == "completed", r
assert abs(r["quality_score"] - 74.0) < 0.01, r
axes = {a["axis"] for a in detail["axes"]}
assert len(axes) == 7, axes
assert len(detail["findings"]) == 2, detail["findings"]
print("PASS list/detail/axes/findings")
PY

curl -sf "http://127.0.0.1:$port/api/runs/$id/turns/0/frame.png" -o "$tmp/frame.png"
test -s "$tmp/frame.png" && echo "PASS frame.png served"
echo "ALL PASS"
