#!/usr/bin/env python3
"""Check CLI formatting through real pipes and pseudo terminals."""

import errno
import json
import os
import pty
import select
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HOST = Path(
    os.environ.get("HOST_BINARY", ROOT / "tools/linux-host/.build/debug/rundale-linux-host")
).resolve()


class TerminalOutputTests(unittest.TestCase):
    def run_host(self, *, terminal=False, format=None, environment=None, protocol=False):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "commands.txt"
            script.write_text(":quit\n")
            args = [
                str(HOST),
                "--mod-dir",
                str(ROOT / "mods/rundale"),
                "--save-dir",
                str(Path(directory) / "save"),
                "--inference",
                "fixture",
            ]
            if format:
                args += ["--format", format]
            if protocol:
                args += ["--protocol"]
            else:
                args += ["--script", str(script)]
            env = dict(os.environ, TERM="xterm-256color")
            env.pop("NO_COLOR", None)
            env.update(environment or {})
            if not terminal:
                request = json.dumps({"version": 1, "id": 1, "command": "shutdown", "args": {}})
                result = subprocess.run(
                    args,
                    input=request + "\n",
                    capture_output=True,
                    text=True,
                    env=env,
                    timeout=20,
                    check=True,
                )
                return result.stdout
            master, slave = pty.openpty()
            try:
                with subprocess.Popen(
                    args, stdin=subprocess.DEVNULL, stdout=slave, stderr=subprocess.PIPE, env=env
                ) as process:
                    os.close(slave)
                    slave = None
                    output = bytearray()
                    deadline = time.monotonic() + 20
                    try:
                        while True:
                            if time.monotonic() >= deadline:
                                self.fail("terminal host timed out")
                            if not select.select([master], [], [], 0.2)[0]:
                                continue
                            try:
                                chunk = os.read(master, 65536)
                            except OSError as error:
                                if error.errno != errno.EIO:
                                    raise
                                break
                            if not chunk:
                                break
                            output.extend(chunk)
                        self.assertEqual(process.wait(timeout=5), 0, process.stderr.read().decode())
                    finally:
                        if process.poll() is None:
                            process.kill()
                return output.decode().replace("\r\n", "\n")
            finally:
                os.close(master)
                if slave is not None:
                    os.close(slave)

    def test_pipe_and_explicit_markdown_keep_original_frames(self):
        raw = self.run_host()
        self.assertIn("## ", raw)
        self.assertIn("**Composer", raw)
        self.assertNotIn("\x1b", raw)
        self.assertEqual(raw, self.run_host(terminal=True, format="markdown"))
        self.assertEqual(raw, self.run_host(terminal=True, environment={"TERM": "dumb"}))

    def test_terminal_auto_and_explicit_ansi_render_markdown(self):
        for kwargs in ({"terminal": True}, {"format": "ansi", "environment": {"NO_COLOR": "1"}}):
            with self.subTest(kwargs=kwargs):
                rendered = self.run_host(**kwargs)
                self.assertIn("\x1b[", rendered)
                self.assertIn("Composer", rendered)
                self.assertNotIn("**Composer", rendered)

    def test_no_color_retains_rendered_layout(self):
        rendered = self.run_host(terminal=True, environment={"NO_COLOR": "1"})
        self.assertIn("Composer", rendered)
        self.assertNotIn("**Composer", rendered)
        self.assertNotIn("\x1b", rendered)

    def test_protocol_ignores_presentation(self):
        output = self.run_host(format="ansi", protocol=True)
        self.assertNotIn("\x1b", output)
        self.assertEqual(json.loads(output)["id"], 1)

    def test_invalid_format_fails_before_opening_save(self):
        with tempfile.TemporaryDirectory() as directory:
            save = Path(directory) / "save"
            result = subprocess.run(
                [
                    str(HOST),
                    "--mod-dir",
                    str(ROOT / "mods/rundale"),
                    "--save-dir",
                    str(save),
                    "--format",
                    "invalid",
                ],
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("--format", result.stderr)
            self.assertFalse(save.exists())


if __name__ == "__main__":
    unittest.main()
