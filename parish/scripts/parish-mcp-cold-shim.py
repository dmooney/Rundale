#!/usr/bin/env python3
"""No-build cold-start stand-in for the parish-mcp stdio server.

Why this exists (parish-mcp-cold-register): Claude Code spawns the `.mcp.json`
command at session init and reads its tool list within a short startup window.
On a fresh worktree the Rust `parish-mcp` binary does not exist yet, and
compiling it synchronously overruns that window, so the `mcp__parish__*` tools
never register and there is no in-session reload (#1352).

This shim is pure Python stdlib — it needs no build. The launcher execs it only
when the real binary is missing. It answers `initialize` / `tools/list` / `ping`
instantly from the committed `manifest.json` (the same file the Rust binary reads
via include_str!, so the tool list is identical). It never compiles anything.

The real binary is produced by the normal `cargo build` / `just build` (parish-mcp
is a workspace default member). As soon as it appears, the first `tools/call`
transparently hands off to it — no session restart. Until then, `tools/call`
returns a graceful "still building" tool error rather than hanging.

Invocation (from parish-mcp-launch.sh):
    parish-mcp-cold-shim.py --manifest <path> --bin <path> -- <args for real binary>
Everything after `--` is forwarded verbatim to the real binary on handoff.
"""

import argparse
import contextlib
import json
import os
import subprocess
import sys
import threading


def log(msg: str) -> None:
    # stderr only — stdout is the JSON-RPC stream.
    print(f"[parish-mcp-cold-shim] {msg}", file=sys.stderr, flush=True)


def send(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def parse_args():
    p = argparse.ArgumentParser(add_help=False)
    p.add_argument("--manifest", required=True)
    p.add_argument("--bin", required=True)
    # Args after `--` are forwarded to the real binary on handoff.
    argv = sys.argv[1:]
    child_args = []
    if "--" in argv:
        i = argv.index("--")
        argv, child_args = argv[:i], argv[i + 1 :]
    ns = p.parse_args(argv)
    return ns.manifest, ns.bin, child_args


def proxy_handoff(binpath: str, child_args, saved_init, pending_call) -> None:
    """Hand the live connection to the real binary.

    Replays the client's initialize handshake to the freshly-built binary so its
    protocol state matches what the client already negotiated against the shim,
    forwards the pending tools/call, then pumps both directions for the rest of
    the session. The tool list is identical across the handoff (both read the
    same manifest.json), so the client never sees a tools list change.
    """
    log(f"binary present at {binpath} — handing off to real parish-mcp")
    child = subprocess.Popen(
        [binpath, *child_args],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,  # inherit — binary logs to stderr
        text=True,
        encoding="utf-8",  # tool descriptions contain non-ASCII (—, Seán)
        bufsize=1,
    )
    # stdin=PIPE / stdout=PIPE guarantee these are not None.
    assert child.stdin is not None and child.stdout is not None

    def to_child(obj):
        child.stdin.write(json.dumps(obj) + "\n")
        child.stdin.flush()

    # 1. Replay initialize (use the client's params so capabilities/version match).
    if saved_init is None:
        saved_init = {
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {}},
        }
    to_child(saved_init)
    # 2. Discard the child's initialize response — the client already got one.
    child.stdout.readline()
    # 3. Announce initialized, then forward the pending tools/call.
    to_child({"jsonrpc": "2.0", "method": "notifications/initialized"})
    to_child(pending_call)

    # 4. Transparent bidirectional pump for the rest of the session.
    #    Use readline() (not `for line in f`) — file iteration reads ahead into a
    #    hidden buffer, which would swallow bytes already pulled past the pending
    #    tools/call on sys.stdin during the handoff above.
    def pump(src, dst):
        try:
            while True:
                line = src.readline()
                if line == "":
                    break
                dst.write(line)
                dst.flush()
        except Exception:
            pass
        finally:
            with contextlib.suppress(Exception):
                dst.close()

    t = threading.Thread(target=pump, args=(sys.stdin, child.stdin), daemon=True)
    t.start()
    pump(child.stdout, sys.stdout)  # ends when the child exits
    child.wait()


def main() -> None:
    # Pin stdio to UTF-8 — the JSON-RPC stream and tool descriptions contain
    # non-ASCII, which a C/POSIX locale default would mis-decode.
    for stream in (sys.stdin, sys.stdout):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")

    manifest_path, binpath, child_args = parse_args()
    try:
        with open(manifest_path, encoding="utf-8") as f:
            manifest = json.load(f)
    except Exception as e:  # noqa: BLE001 - any failure here must not crash registration
        log(f"failed to read manifest {manifest_path}: {e}")
        manifest = {}

    proto = manifest.get("protocolVersion", "2025-06-18")
    manifest_server_info = manifest.get("serverInfo", {})
    if not isinstance(manifest_server_info, dict):
        manifest_server_info = {}
    server_info = dict(manifest_server_info)
    server_info.setdefault("name", "parish-mcp")
    server_info.setdefault("version", "0.1.0")
    tools = manifest.get("tools", [])
    log(f"cold start: serving {len(tools)} tools from manifest (binary not built yet)")

    saved_init = None

    # readline() (not `for line in sys.stdin`) so no input is stranded in an
    # iterator's read-ahead buffer when we hand off to the real binary mid-stream.
    while True:
        raw = sys.stdin.readline()
        if raw == "":
            break  # EOF — client closed the connection
        line = raw.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = req.get("method")
        rid = req.get("id")

        if method == "initialize":
            saved_init = req
            send(
                {
                    "jsonrpc": "2.0",
                    "id": rid,
                    "result": {
                        "protocolVersion": proto,
                        # MCP Implementation requires both name and version.
                        # Forward the manifest object intact so the cold path
                        # cannot silently drop future optional identity fields.
                        "serverInfo": server_info,
                        "capabilities": {"tools": {"listChanged": False}},
                    },
                }
            )
        elif method in ("notifications/initialized", "initialized"):
            pass  # notification — no response
        elif method == "ping":
            send({"jsonrpc": "2.0", "id": rid, "result": {}})
        elif method == "tools/list":
            send({"jsonrpc": "2.0", "id": rid, "result": {"tools": tools}})
        elif method == "tools/call":
            if os.access(binpath, os.X_OK):
                proxy_handoff(binpath, child_args, saved_init, req)
                return
            send(
                {
                    "jsonrpc": "2.0",
                    "id": rid,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": (
                                    "parish engine is still building — the parish-mcp "
                                    "binary is not ready yet. It is produced by the normal "
                                    "`cargo build` / `just build`; retry this call shortly."
                                ),
                            }
                        ],
                        "isError": True,
                    },
                }
            )
        elif rid is not None:
            send(
                {
                    "jsonrpc": "2.0",
                    "id": rid,
                    "error": {"code": -32601, "message": f"method not found: {method}"},
                }
            )
        # else: unknown notification — ignore.


if __name__ == "__main__":
    main()
