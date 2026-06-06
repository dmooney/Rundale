#!/usr/bin/env bash
# Reproducible runtime-faithful prompt capture (REQ 2).
#
# Drives the REAL Rundale engine against a local recording stub (an
# OpenAI-compatible server that logs every request and returns a canned reply so
# the loop keeps advancing), over a scripted tour that exercises every inference
# category, then folds the byte-exact captured requests into the v2 datasets and
# re-pins the manifest. No API keys, no real model — the engine builds its true
# prompts before any client call, so what we capture is what the live game sends.
#
#   bash promptfoo/scripts/capture_prompts.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PF="$ROOT/promptfoo"
PORT="${RB_CAPTURE_PORT:-8733}"
CAP1="$(mktemp -t rb_cap1.XXXXXX.jsonl)"
CAP2="$(mktemp -t rb_cap2.XXXXXX.jsonl)"
SAVES="$(mktemp -d -t rb_cap_saves.XXXXXX)"

cleanup() { kill "${SRV:-0}" 2>/dev/null || true; }
trap cleanup EXIT

run_drive() { # $1=capture-out  $2=drive-script
    CAPTURE_OUT="$1" python3 "$PF/scripts/capture_server.py" "$PORT" >/tmp/rb_capsrv.log 2>&1 &
    SRV=$!
    # wait for the stub
    for _ in $(seq 1 20); do
        curl -sf "http://127.0.0.1:$PORT/v1/models" >/dev/null 2>&1 && break
        sleep 0.3
    done
    PARISH_SAVES_DIR="$SAVES" PARISH_PROVIDER=custom PARISH_MODEL=capture \
        PARISH_BASE_URL="http://127.0.0.1:$PORT/v1" PARISH_API_KEY=dummy \
        cargo run -q --manifest-path "$ROOT/parish/Cargo.toml" -p parish-engine -- --headless \
        <"$2" >/dev/null 2>&1 || true
    kill "$SRV" 2>/dev/null || true
    sleep 0.5
}

# Sweep 1: talk to many NPCs across locations (dialogue + intent + reaction + tier2).
python3 - >"$SAVES/drive1.txt" <<'PY'
locs=["the forge","the mill","the holy well","the crossroads","st. brigid's church","darcy's pub","murphy's farm","the weaver's cottage","the letter office","knockcroghery village"]
npcs=["Peig Hannigan","Padraig Darcy","Siobhan Murphy","Fr. Declan Tierney","Roisin Connolly","Tommy O'Brien","Aoife Brennan","Mick Flanagan","Niamh Darcy","Seamus Gallagher","Maire Gallagher","Cormac Duffy","Eamon Walsh","Kathleen Walsh","Liam Murphy","Una Malone"]
turns=["God save you. What news of the parish?","How does the harvest look this year?","Tell me, have you seen Father Declan today?","and how is your own family keeping?"]
out=["look"]; ni=0
for loc in locs:
    out += [f"go to {loc}","look"]
    for _ in range(2):
        out.append(f"talk to {npcs[ni%len(npcs)]}"); ni+=1
        out += turns
    out.append("/wait 360")
out += ["/wait 720","/quit"]; print("\n".join(out))
PY
run_drive "$CAP1" "$SAVES/drive1.txt"

# Sweep 2: re-arrivals (reaction) + multi-day jumps (tier3 batches).
python3 - >"$SAVES/drive2.txt" <<'PY'
locs=["the forge","the mill","the holy well","the crossroads","st. brigid's church","darcy's pub","murphy's farm","the weaver's cottage","the letter office","knockcroghery village","the lime kiln"]
out=[]
for _ in range(3):
    for loc in locs: out += [f"go to {loc}","look"]
    out += ["/wait 1440","/wait 1440"]
out.append("/quit"); print("\n".join(out))
PY
run_drive "$CAP2" "$SAVES/drive2.txt"

echo "[capture] folding captures into datasets…"
python3 "$PF/scripts/build_runtime_datasets.py" "$CAP1" "$CAP2"
python3 "$PF/scripts/pin_manifest.py"
echo "[capture] done. Re-run the bench to score the refreshed datasets."
