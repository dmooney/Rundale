#!/usr/bin/env bash
# Verification fixture for task: parish-mcp-cold-register
#
# Drives the REAL JSON-RPC handshake through parish-mcp-launch.sh and asserts the
# acceptance criteria. Cold scenarios force the no-binary path with an empty
# CARGO_TARGET_DIR; the warm scenario uses the real (shared) target dir.
#
#   bash parish/testing/fixtures/mcp_cold_register.sh
#
# Exit 0 = every asserted criterion passed.

set -uo pipefail
REPO="$(git rev-parse --show-toplevel)"
LAUNCH="$REPO/parish/scripts/parish-mcp-launch.sh"
MANIFEST="$REPO/parish/crates/parish-mcp/manifest.json"

pass=0 fail=0
ok() {
    echo "PASS: $1"
    pass=$((pass + 1))
}
no() {
    echo "FAIL: $1"
    fail=$((fail + 1))
}

REQ_INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}'
REQ_INITED='{"jsonrpc":"2.0","method":"notifications/initialized"}'
REQ_LIST='{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
REQ_CALL='{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"parish_world_snapshot","arguments":{}}}'

# drive <out> <err> <grace> <req...> — feed requests, kill after grace, capture.
drive() {
    local out="$1" err="$2" grace="$3"
    shift 3
    {
        printf '%s\n' "$@"
        sleep "$grace"
    } | bash "$LAUNCH" --base-url http://127.0.0.1:3030 >"$out" 2>"$err"
}

MANIFEST_NAMES="$(python3 -c 'import json,sys;print(",".join(t["name"] for t in json.load(open(sys.argv[1]))["tools"]))' "$MANIFEST")"

echo "=== AC#5: manifest.json committed ==="
if [ -f "$MANIFEST" ]; then ok "AC#5 manifest.json present ($(echo "$MANIFEST_NAMES" | tr ',' '\n' | wc -l | tr -d ' ') tools)"; else no "AC#5 manifest.json missing"; fi

echo "=== AC#1: cold register (no binary) returns init + tools/list ==="
COLD="$(mktemp -d)"
CARGO_TARGET_DIR="$COLD/empty" drive /tmp/cr_a.out /tmp/cr_a.err 2 "$REQ_INIT" "$REQ_INITED" "$REQ_LIST"
A_NAME="$(python3 -c 'import json;[print(json.loads(l)["result"]["serverInfo"]["name"]) for l in open("/tmp/cr_a.out") if json.loads(l).get("id")==1]' 2>/dev/null)"
A_VERSION="$(python3 -c 'import json;[print(json.loads(l)["result"]["serverInfo"]["version"]) for l in open("/tmp/cr_a.out") if json.loads(l).get("id")==1]' 2>/dev/null)"
A_NAMES="$(python3 -c 'import json;[print(",".join(t["name"] for t in json.loads(l)["result"]["tools"])) for l in open("/tmp/cr_a.out") if json.loads(l).get("id")==2]' 2>/dev/null)"
if [ "$A_NAME" = "parish-mcp" ] && [ "$A_VERSION" = "0.1.0" ] && [ "$A_NAMES" = "$MANIFEST_NAMES" ]; then
    ok "AC#1 cold register: required serverInfo fields + tools == manifest"
else
    no "AC#1 cold register mismatch (name='$A_NAME', version='$A_VERSION')"
fi
if grep -q "cold shim" /tmp/cr_a.err && ! grep -qi "compiling\|cargo build" /tmp/cr_a.err; then
    ok "AC#1 took the no-build shim (no cargo on the init path)"
else
    no "AC#1 did not take the no-build shim"
fi
rm -rf "$COLD"

echo "=== AC#2: tools/call before build is graceful ==="
COLD="$(mktemp -d)"
CARGO_TARGET_DIR="$COLD/empty" drive /tmp/cr_b.out /tmp/cr_b.err 2 "$REQ_INIT" "$REQ_INITED" "$REQ_CALL"
B_OK="$(python3 -c '
import json
for l in open("/tmp/cr_b.out"):
    o=json.loads(l)
    if o.get("id")==3:
        r=o.get("result",{})
        print("yes" if r.get("isError") is True and "building" in r.get("content",[{}])[0].get("text","").lower() else "no")
' 2>/dev/null)"
if [ "$B_OK" = "yes" ]; then ok "AC#2 cold tools/call -> isError:true 'still building'"; else no "AC#2 cold tools/call not graceful"; fi
rm -rf "$COLD"

echo "=== AC#3: warm fast path execs the real binary ==="
# No CARGO_TARGET_DIR override -> launcher resolves the real shared target dir.
drive /tmp/cr_c.out /tmp/cr_c.err 2 "$REQ_INIT" "$REQ_INITED" "$REQ_LIST"
C_NAMES="$(python3 -c 'import json;[print(",".join(t["name"] for t in json.loads(l)["result"]["tools"])) for l in open("/tmp/cr_c.out") if json.loads(l).get("id")==2]' 2>/dev/null)"
if [ "$C_NAMES" = "$MANIFEST_NAMES" ] && ! grep -q "cold shim" /tmp/cr_c.err; then
    ok "AC#3 warm path: real binary registered tools (no shim)"
elif grep -q "cold shim" /tmp/cr_c.err; then
    echo "SKIP: AC#3 binary not built in this env (took shim) — run 'just build' to exercise warm path"
else
    no "AC#3 warm path did not register expected tools"
fi

echo "----"
echo "passed=$pass failed=$fail"
[ "$fail" -eq 0 ]
