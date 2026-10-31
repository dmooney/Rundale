#!/usr/bin/env python3
"""Exercise the native session through the real MCP bridge with mock inference."""

import json
import os
import select
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MCP = Path(os.environ.get("MCP_BINARY", ROOT / "limerick/target/debug/limerick-mcp")).resolve()
HOST = Path(
    os.environ.get("HOST_BINARY", ROOT / "tools/rundale-cli/.build/debug/rundale-cli")
).resolve()
MOD = ROOT / "mods/rundale"


class Client:
    def __init__(self, save, inference="mock"):
        self.process = subprocess.Popen(
            [
                str(MCP),
                "--backend",
                "session-host",
                "--host-executable",
                str(HOST),
                "--mod-dir",
                str(MOD),
                "--save-dir",
                str(save),
                "--inference",
                inference,
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        self.sequence = 0

    def call(self, name, **arguments):
        self.sequence += 1
        request = {
            "jsonrpc": "2.0",
            "id": self.sequence,
            "method": "tools/call",
            "params": {"name": "limerick_" + name, "arguments": arguments},
        }
        self.process.stdin.write((json.dumps(request, ensure_ascii=False) + "\n").encode())
        self.process.stdin.flush()
        if not select.select([self.process.stdout], [], [], 65)[0]:
            raise AssertionError(f"MCP timed out: {name}")
        response = json.loads(self.process.stdout.readline())
        if "error" in response:
            raise AssertionError(response["error"])
        return response["result"]

    def action(self, name, **args):
        result = self.call(name, **args)
        if result.get("isError"):
            raise AssertionError(result)
        return result["structuredContent"]

    def wait_for(self, predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            state = self.action("session_presentation")
            if predicate(state):
                return state
            time.sleep(0.1)
        raise AssertionError(f"Session did not reach expected state: {state}")

    def eof(self):
        self.process.stdin.close()
        self.process.wait(timeout=15)
        self.process.stdout.close()
        if self.process.returncode != 0:
            raise AssertionError(f"MCP exited {self.process.returncode}")


class SessionMcpTest(unittest.TestCase):
    def test_stream_stop_save_resume_and_eof(self):
        with tempfile.TemporaryDirectory() as directory:
            save = Path(directory)
            client = Client(save)
            try:
                unopened = client.action("session_presentation")
                self.assertFalse(unopened["presentation"]["sessionOpen"])
                client.action("new_game")
                client.action("submit_input", text="go to Connolly Cottage")
                client.wait_for(lambda s: not s["presentation"]["isStreaming"])
                client.action("submit_input", text="Róisín, what do you trade? slow")
                streamed = client.wait_for(
                    lambda s: any(
                        item["isProvisional"]
                        for frame in s["history"]
                        for item in frame.get("transcript", [])
                    )
                )
                self.assertTrue(streamed["presentation"]["isStreaming"])
                busy = client.call("persist_session")
                self.assertTrue(busy["isError"])
                self.assertIn("busy", busy["content"][0]["text"].lower())
                client.action("stop")
                stopped = client.wait_for(lambda s: not s["presentation"]["isStreaming"])
                self.assertFalse(
                    any(i["isProvisional"] for i in stopped["presentation"]["transcript"])
                )
                client.action("submit_input", text="Róisín, how is the spinning going?")
                committed_reply = client.wait_for(lambda s: not s["presentation"]["isStreaming"])
                self.assertTrue(
                    any(
                        item["kind"] == "npc_dialogue" and item["state"] == "committed"
                        for item in committed_reply["presentation"]["transcript"]
                    )
                )
                saved = client.action("persist_session")
                self.assertEqual(saved["presentation"]["persistence"]["status"], "saved")

                def committed_content(items):
                    # Presentation row IDs can change when the canonical event
                    # history is rehydrated; content and commit status must not.
                    return [
                        {key: value for key, value in item.items() if key != "id"}
                        for item in items
                        if item["state"] in ("committed", "accepted")
                    ]

                transcript = committed_content(saved["presentation"]["transcript"])
                client.action("close_session")
                client.eof()
                client = None

                # Starting the bridge alone must neither open nor replace a save.
                client = Client(save)
                self.assertFalse(
                    client.action("session_presentation")["presentation"]["sessionOpen"]
                )
                resumed = client.action("resume_session")
                self.assertEqual(
                    committed_content(resumed["presentation"]["transcript"]), transcript
                )
                # EOF must close the child and release its save lock.
                client.eof()
                client = None
                client = Client(save)
                self.assertEqual(
                    committed_content(
                        client.action("resume_session")["presentation"]["transcript"]
                    ),
                    transcript,
                )
            finally:
                if client is not None:
                    client.eof()


if __name__ == "__main__":
    unittest.main()
