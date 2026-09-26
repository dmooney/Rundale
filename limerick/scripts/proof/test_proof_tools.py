"""Tests for the differential proof tools."""

from __future__ import annotations

import json
import sys
import threading
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import noise  # noqa: E402
from body_diff import load_requests  # noqa: E402
from differences import Intended, Item, check, diff_units, load_intended  # noqa: E402
from drive_session import Session, read_scenario  # noqa: E402
from script_compare import script_units  # noqa: E402
from scripted_openai import ScriptedServer, classify  # noqa: E402


def units(*lines: str) -> list[tuple[str, list[str]]]:
    """One unit per argument; a unit's lines are separated by '|'."""
    return [(f"u{i}", line.split("|")) for i, line in enumerate(lines)]


def signs(items: list[Item]) -> list[str]:
    return [f"{item.sign}{item.text}" for item in items]


# differences.diff_units


def test_identical_runs_have_no_differences() -> None:
    run = units("a|b", "c")
    assert diff_units("script", "f", [run, run], [run, run]) == []


def test_stable_units_are_compared_in_order() -> None:
    base = units("a|b", "c")
    head = units("b|a", "c")
    # A reorder shows as one line leaving and re-entering.
    assert signs(diff_units("script", "f", [base], [head])) == ["+b", "-b"]


def test_changed_line_is_reported_with_unit_label() -> None:
    items = diff_units("script", "f", [units("a|b")], [units("a|B")])
    assert [str(item) for item in items] == ["script/f u0 - b", "script/f u0 + B"]


def test_inserted_and_removed_units_are_reported_whole() -> None:
    assert signs(diff_units("script", "f", [units("a", "c")], [units("a", "b|x", "c")])) == [
        "+b",
        "+x",
    ]
    assert signs(diff_units("script", "f", [units("a", "b", "c")], [units("a", "c")])) == ["-b"]


def test_a_unit_that_varies_on_base_is_compared_as_a_range() -> None:
    base_runs = [units("x|p"), units("x|q")]
    # Head shows a combination no single base run had: still inside the range.
    assert diff_units("script", "f", base_runs, [units("x|p|q")]) == []
    # A line no base run has, in every head run, is a difference.
    assert signs(diff_units("script", "f", base_runs, [units("x|p|z"), units("x|z")])) == ["+z"]
    # A line every base run has, missing from every head run, is a difference.
    assert signs(diff_units("script", "f", base_runs, [units("p")])) == ["-x"]


def test_a_line_only_some_head_runs_have_is_not_a_difference() -> None:
    base_runs = [units("x"), units("x")]
    head_runs = [units("x|noise"), units("x")]
    assert diff_units("script", "f", base_runs, head_runs) == []


# differences.check / load_intended


def test_undeclared_and_unobserved_declarations(tmp_path: Path) -> None:
    path = tmp_path / "intended.toml"
    path.write_text(
        '[[intended]]\nsurface = "script"\nmatch = "Hold time"\nreason = "help text"\n\n'
        '[[intended]]\nmatch = "never"\nreason = "not observed"\n'
    )
    intended = load_intended(path)
    items = [
        Item("script", "test_commands", "cmd 1", "+", "/pause — Hold time quite still"),
        Item("requests", "talk", "turn 1", "+", "Hold time"),
    ]
    undeclared = check(items, intended)
    assert undeclared == [items[1]]  # surface filter excludes the request line
    assert [entry.hits for entry in intended] == [1, 0]


def test_name_filter_uses_fnmatch() -> None:
    entry = Intended(match="x", reason="r", name="test_debug*")
    assert entry.covers(Item("script", "test_debug_all_npcs", "cmd 1", "+", "x"))
    assert not entry.covers(Item("script", "test_walkthrough", "cmd 1", "+", "x"))


# noise


def test_tier_lists_are_sorted_and_undecorated() -> None:
    line = "  Here: Peig Hannigan 😤 [sharp], Aoife Brennan 🔥 [passionate]"
    assert noise.script_log_line(line, set()) == "  Here: Aoife Brennan, Peig Hannigan"
    tiers = "  Tier 2 (nearby): Nora Duffy, Liam Murphy"
    assert noise.script_log_line(tiers, set()) == "  Tier 2 (nearby): Liam Murphy, Nora Duffy"


def test_encounters_emoji_lines_and_save_times_are_masked() -> None:
    assert noise.script_log_line("  · A fox sits in the road.", set()) is None
    assert noise.script_log_line("Peig nods. 🙂", set()) is None
    assert noise.script_log_line("A fixed encounter text", {"A fixed encounter text"}) is None
    saved = "  #5 — game: 1820-03-20T08:26:01+00:00 | saved: 26 Sep 1:27 PM"
    assert noise.mask_times(saved) == "  #N — game: <t> | saved: <wall-clock> PM"


def test_tier3_prompt_npc_blocks_are_sorted() -> None:
    prompt = (
        "Simulate.\n\nNPCs (id in brackets — reuse these in your JSON):\n"
        "- [17] Ciaran\n  close to Kathleen\n- [16] Kathleen\n  close to Ciaran\n"
        "For each NPC, return one update."
    )
    lines = noise.prompt_text(prompt).split("\n")
    assert lines[3:7] == [
        "- [16] Kathleen",
        "  close to Ciaran",
        "- [17] Ciaran",
        "  close to Kathleen",
    ]
    assert lines[-1] == "For each NPC, return one update."


# script_compare


def test_script_units_split_fields_and_include_exit(tmp_path: Path) -> None:
    out = tmp_path / "test_x.jsonl"
    record = {
        "command": "/help",
        "result": "system_command",
        "response": "Available:\n  /pause",
        "new_log_lines": ["b line", "a line\nsecond"],
    }
    out.write_text(json.dumps(record) + "\nnot json\n")
    (tmp_path / "test_x.exit").write_text("0\n")
    units_ = script_units(out, set())
    assert units_[0] == ("exit", ["exit: 0"])
    assert units_[1][1] == [
        'command: "/help"',
        'result: "system_command"',
        "response| Available:",
        "response|   /pause",
        "log: a line",
        "log: b line",
        "log: second",
    ]
    assert units_[2][1] == ["raw: not json"]


# scripted_openai / body_diff


def test_classify_routes_each_workload() -> None:
    def body(system: str, user: str) -> dict:
        return {
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]
        }

    workload, reply = classify(body("You are an input parser.", "walking on toward The Mill"))
    assert workload == "intent"
    assert json.loads(reply) == {"intent": "move", "target": "The Mill", "dialogue": None}
    workload, reply = classify(body("Respond in character as Peig.", "Any work here?"))
    assert (workload, json.loads(reply)["assigned_task"]) == (
        "dialogue",
        "Dig over the potato patch.",
    )
    assert classify(body("Pick an emoji", "x"))[0] == "reaction"
    assert classify(body("", "You are simulating background NPC activity"))[0] == "simulation"


def test_server_logs_bodies_and_streams(tmp_path: Path) -> None:
    log = tmp_path / "requests.jsonl"
    server = ScriptedServer(log).start()
    try:
        url = f"http://127.0.0.1:{server.port}/v1/chat/completions"
        messages = [{"role": "system", "content": "Respond in character"}]
        for stream in (False, True):
            request = urllib.request.Request(
                url,
                data=json.dumps({"messages": messages, "stream": stream}).encode(),
                headers={"Content-Type": "application/json"},
            )
            with urllib.request.urlopen(request) as response:
                text = response.read().decode()
            if stream:
                assert text.endswith("data: [DONE]\n\n")
            else:
                assert "God bless ye" in json.loads(text)["choices"][0]["message"]["content"]
    finally:
        server.shutdown()
    entries = [json.loads(line) for line in log.read_text().splitlines()]
    assert [(e["workload"], e["body"]["stream"]) for e in entries] == [
        ("dialogue", False),
        ("dialogue", True),
    ]


def test_requests_group_by_turn_with_background_sorted(tmp_path: Path) -> None:
    def entry(workload: str, text: str) -> str:
        body = {"model": "scripted", "messages": [{"role": "user", "content": text}]}
        return json.dumps({"workload": workload, "body": body})

    log = tmp_path / "requests.jsonl"
    log.write_text(
        "\n".join(
            [
                json.dumps({"turn": 1, "input": "hello"}),
                entry("intent", "i"),
                entry("simulation", "z"),
                entry("simulation", "a"),
                entry("dialogue", "d"),
            ]
        )
    )
    loaded = load_requests(log)
    assert [label for label, _ in loaded] == [
        "turn 1 request (intent)",
        "turn 1 request (dialogue)",
        "turn 1 background (simulation)",
        "turn 1 background (simulation)",
    ]
    assert loaded[2][1][-1] == "user| a"


# drive_session


def test_session_keeps_a_secure_cookie_over_http() -> None:
    seen: list[str | None] = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            pass

        def do_GET(self) -> None:
            seen.append(self.headers.get("Cookie"))
            self.send_response(200)
            self.send_header("Set-Cookie", "limerick_sid=abc; HttpOnly; Secure; Path=/")
            self.send_header("Content-Length", "0")
            self.end_headers()

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        session = Session(f"http://127.0.0.1:{server.server_address[1]}")
        session.request("/")
        session.request("/")
    finally:
        server.shutdown()
    assert seen == [None, "limerick_sid=abc"]


def test_scenario_skips_comments_and_blanks(tmp_path: Path) -> None:
    path = tmp_path / "s.txt"
    path.write_text("# comment\n\n/pause\n  hello  \n")
    assert read_scenario(path) == ["/pause", "hello"]
