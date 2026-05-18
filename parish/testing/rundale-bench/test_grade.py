#!/usr/bin/env python3
"""Unit tests for rundale-bench graders. Run: `python3 test_grade.py`."""
from __future__ import annotations

import copy
import hashlib
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from grade import (  # noqa: E402
    _jaccard,
    extract_dialogue_for_judging,
    grade_dialogue,
    grade_intent,
    grade_pairwise,
    grade_reaction,
    grade_schema,
    grade_simulation,
    verify_judge_rubric,
)


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------

def _judge_fixture(rubric: str = "test rubric") -> dict:
    return {
        "judge_id": "test_judge",
        "model": "claude-sonnet-4-6",
        "base_url": "https://api.anthropic.com/v1",
        "api_key_env": "ANTHROPIC_API_KEY",
        "temperature": 0,
        "rubric": rubric,
        "rubric_sha256": hashlib.sha256(rubric.encode()).hexdigest(),
        "axes": ["character", "authenticity", "language", "responsiveness", "craft"],
        "score_range": [1, 5],
    }


def _stub_invoke(response: dict):
    return lambda system, user, schema: response


# ---------------------------------------------------------------------------
# _jaccard
# ---------------------------------------------------------------------------

def test_jaccard():
    assert _jaccard(None, None) == 1.0
    assert _jaccard(None, "x") == 0.0
    assert _jaccard("x", None) == 0.0
    assert _jaccard("the pub", "the pub") == 1.0
    assert _jaccard("the pub", "the bog") == 1 / 3  # tokens: {the,pub} & {the,bog} = {the}
    assert _jaccard("THE Pub.", "the pub!") == 1.0  # case + punctuation insensitive
    assert _jaccard("", "") == 1.0


# ---------------------------------------------------------------------------
# grade_intent
# ---------------------------------------------------------------------------

def test_intent_exact_match():
    r = grade_intent(
        {"intent": "move", "target": "the pub", "dialogue": None},
        {"intent": "move", "target": "the pub", "dialogue": None},
    )
    assert r["label_match"] == 1
    assert r["target_jaccard"] == 1.0
    assert r["dialogue_jaccard"] == 1.0
    assert r["score"] == 1.0


def test_intent_label_mismatch_is_zero():
    r = grade_intent(
        {"intent": "move", "target": "Padraig", "dialogue": None},
        {"intent": "talk", "target": "Padraig", "dialogue": None},
    )
    assert r["label_match"] == 0
    assert r["score"] == 0.0


def test_intent_partial_target():
    r = grade_intent(
        {"intent": "move", "target": "the pub", "dialogue": None},
        {"intent": "move", "target": "the old pub", "dialogue": None},
    )
    assert r["label_match"] == 1
    assert 0 < r["target_jaccard"] < 1


def test_intent_dialogue_credit():
    r = grade_intent(
        {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow"},
        {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow wandering"},
    )
    assert r["label_match"] == 1
    assert r["score"] > 0.5


def test_intent_null_target():
    r = grade_intent(
        {"intent": "look", "target": None, "dialogue": None},
        {"intent": "look", "target": None, "dialogue": None},
    )
    assert r["score"] == 1.0


# ---------------------------------------------------------------------------
# grade_schema
# ---------------------------------------------------------------------------

_INTENT_SCHEMA = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "intent": {"type": "string", "enum": ["move", "talk", "look", "interact", "examine", "unknown"]},
        "target": {"type": ["string", "null"]},
        "dialogue": {"type": ["string", "null"]},
    },
    "required": ["intent", "target", "dialogue"],
}


def test_schema_valid():
    r = grade_schema({"intent": "move", "target": "the pub", "dialogue": None}, _INTENT_SCHEMA)
    assert r["schema_valid"], r


def test_schema_missing_required():
    r = grade_schema({"intent": "move", "target": "the pub"}, _INTENT_SCHEMA)
    assert not r["schema_valid"]
    assert any("dialogue" in e for e in r["errors"])


def test_schema_bad_enum():
    r = grade_schema({"intent": "fly", "target": None, "dialogue": None}, _INTENT_SCHEMA)
    assert not r["schema_valid"]


def test_schema_extra_keys_rejected():
    r = grade_schema({"intent": "move", "target": "x", "dialogue": None, "extra": 1}, _INTENT_SCHEMA)
    assert not r["schema_valid"]


def test_schema_parses_json_string():
    r = grade_schema('{"intent":"move","target":"x","dialogue":null}', _INTENT_SCHEMA)
    assert r["schema_valid"]


def test_schema_malformed_string():
    r = grade_schema('{not json', _INTENT_SCHEMA)
    assert not r["schema_valid"]


# ---------------------------------------------------------------------------
# verify_judge_rubric
# ---------------------------------------------------------------------------

def test_judge_rubric_match():
    j = _judge_fixture("hello")
    verify_judge_rubric(j)  # does not raise


def test_judge_rubric_tamper_detected():
    j = _judge_fixture("hello")
    j["rubric"] = "tampered"
    raised = False
    try:
        verify_judge_rubric(j)
    except RuntimeError as e:
        assert "sha256" in str(e)
        raised = True
    assert raised, "expected RuntimeError on rubric tamper"


# ---------------------------------------------------------------------------
# grade_dialogue
# ---------------------------------------------------------------------------

def test_dialogue_happy_path():
    j = _judge_fixture()
    invoke = _stub_invoke({
        "character": 5, "authenticity": 4, "language": 5,
        "responsiveness": 4, "craft": 5, "overall": 4.6,
    })
    r = grade_dialogue("'Tis a fine evening, lass.", j, invoke)
    assert r["overall"] == 4.6
    assert r["character"] == 5
    assert r["non_latin_chars"] == {}


def test_dialogue_non_latin_detected():
    j = _judge_fixture()
    invoke = _stub_invoke({
        "character": 5, "authenticity": 5, "language": 5,
        "responsiveness": 5, "craft": 5, "overall": 5.0,
    })
    r = grade_dialogue("Привет, lass.", j, invoke)
    assert "Cyrillic" in r["non_latin_chars"]


def test_dialogue_rubric_tamper_blocks_call():
    j = _judge_fixture("orig")
    j["rubric"] = "tampered"
    raised = False
    try:
        grade_dialogue("hi", j, _stub_invoke({}))
    except RuntimeError as e:
        raised = "sha256" in str(e)
    assert raised


# ---------------------------------------------------------------------------
# grade_reaction
# ---------------------------------------------------------------------------

def test_reaction_happy():
    j = _judge_fixture()
    r = grade_reaction("Aye, sit yourself down.", "publican", j, _stub_invoke({"in_character": 4}))
    assert r["score"] == 0.8
    assert r["length_ok"]
    assert r["non_latin_chars"] == {}


def test_reaction_too_short():
    j = _judge_fixture()
    r = grade_reaction("hi", "publican", j, _stub_invoke({"in_character": 5}))
    assert r["score"] == 0.0
    assert not r["length_ok"]


def test_reaction_non_latin_zeros():
    j = _judge_fixture()
    r = grade_reaction("Aye, привет.", "publican", j, _stub_invoke({"in_character": 5}))
    assert r["score"] == 0.0
    assert "Cyrillic" in r["non_latin_chars"]


# ---------------------------------------------------------------------------
# grade_pairwise
# ---------------------------------------------------------------------------

def test_pairwise_judge_picks_winner():
    j = _judge_fixture()
    r = grade_pairwise("Reply A", "Reply B", "prompt", j, _stub_invoke({"winner": "A", "reason": "cleaner voice"}))
    assert r["winner"] == "A"
    assert "cleaner" in r["reason"]


def test_pairwise_tie():
    j = _judge_fixture()
    r = grade_pairwise("a", "b", "p", j, _stub_invoke({"winner": "tie", "reason": "equivalent"}))
    assert r["winner"] == "tie"


def test_pairwise_invalid_winner_falls_back_to_tie():
    j = _judge_fixture()
    r = grade_pairwise("a", "b", "p", j, _stub_invoke({"winner": "X", "reason": "?"}))
    assert r["winner"] == "tie"
    assert "error" in r


def test_pairwise_non_latin_auto_disqualifies():
    j = _judge_fixture()
    # A has Cyrillic, B is clean → B wins automatically
    r = grade_pairwise("Привет lass", "Aye lass", "p", j, _stub_invoke({"winner": "A", "reason": "ignored"}))
    assert r["winner"] == "B"
    assert r["auto_disqualified"] == "A"


def test_pairwise_rubric_tamper_blocks():
    j = _judge_fixture("orig")
    j["rubric"] = "tampered"
    raised = False
    try:
        grade_pairwise("a", "b", "p", j, _stub_invoke({}))
    except RuntimeError as e:
        raised = "sha256" in str(e)
    assert raised


# ---------------------------------------------------------------------------
# grade_simulation
# ---------------------------------------------------------------------------

_TIER2_SCHEMA_INNER = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "summary": {"type": "string"},
        "mood_changes": {"type": "array", "items": {"type": "object"}},
        "relationship_changes": {"type": "array", "items": {"type": "object"}},
    },
    "required": ["summary", "mood_changes", "relationship_changes"],
}


def test_simulation_invalid_schema_zero():
    j = _judge_fixture()
    r = grade_simulation({"summary": "x"}, _TIER2_SCHEMA_INNER, j, _stub_invoke({"plausibility": 5}))
    assert not r["schema_valid"]
    assert r["score"] == 0.0


def test_simulation_valid_then_judge():
    j = _judge_fixture()
    pred = {"summary": "Padraig pours stout.", "mood_changes": [], "relationship_changes": []}
    r = grade_simulation(pred, _TIER2_SCHEMA_INNER, j, _stub_invoke({"plausibility": 4}))
    assert r["schema_valid"]
    assert r["plausibility"] == 4
    assert r["score"] == 0.8


# ---------------------------------------------------------------------------
# extract_dialogue_for_judging — strip runtime metadata envelope
# ---------------------------------------------------------------------------

def test_extract_dialogue_dash_marker():
    reply = (
        "Chew on a few cloves if you can get them, or make a warm poultice from "
        "willow bark to draw out the ache.\n"
        "---\n"
        "{\"action\": \"reaches into her basket\", \"mood\": \"content\", \"language_hints\": []}"
    )
    out = extract_dialogue_for_judging(reply)
    assert "---" not in out
    assert "action" not in out
    assert out.startswith("Chew on a few cloves")
    assert out.endswith("draw out the ache.")


def test_extract_dialogue_dash_marker_trailing_whitespace():
    reply = "dialogue line\n--- \n{\"action\": \"x\"}"
    assert extract_dialogue_for_judging(reply) == "dialogue line"


def test_extract_dialogue_dash_marker_no_trailing_block():
    # Some replies emit the `---` then truncate — still strip everything
    # from the marker onward.
    reply = "spoken text\n---"
    assert extract_dialogue_for_judging(reply) == "spoken text"


def test_extract_dialogue_dash_at_start_returns_empty():
    # Reply that opens with `---` has no preamble. Helper returns the
    # empty-string prefix (rstripped) so the judge sees the model emitted
    # nothing the player would have heard.
    reply = "\n---\n{\"action\": \"x\"}"
    assert extract_dialogue_for_judging(reply) == ""


def test_extract_dialogue_json_first():
    reply = (
        "{\"dialogue\": \"Ah, good morning to ye!\", "
        "\"action\": \"looks up\", \"mood\": \"friendly\", "
        "\"language_hints\": []}"
    )
    assert extract_dialogue_for_judging(reply) == "Ah, good morning to ye!"


def test_extract_dialogue_json_first_with_escaped_quotes():
    reply = (
        "{\"dialogue\": \"She said \\\"hello\\\" to me.\", "
        "\"action\": \"shrugs\"}"
    )
    assert extract_dialogue_for_judging(reply) == "She said \"hello\" to me."


def test_extract_dialogue_json_first_missing_dialogue_field():
    # JSON-first envelope but no `dialogue` key → fall through to verbatim
    # return so the judge can score whatever the model actually emitted.
    reply = "{\"action\": \"x\", \"mood\": \"y\"}"
    assert extract_dialogue_for_judging(reply) == reply


def test_extract_dialogue_legacy_plain_text():
    # Pre-#994 bench prompt era — no envelope. Return as-is.
    reply = "Aye, the toothache's a cruel thing, especially when it robs your rest."
    assert extract_dialogue_for_judging(reply) == reply


def test_extract_dialogue_malformed_json_recovers():
    # Truncated mid-string: runtime's heuristic recovers the dialogue
    # prefix; bench mirrors that. Without recovery the judge would see
    # the raw JSON envelope.
    reply = "{\"dialogue\": \"unterminated"
    assert extract_dialogue_for_judging(reply) == "unterminated"


def test_extract_dialogue_completely_malformed_falls_through():
    # No envelope, no dialogue key — return verbatim.
    reply = "{not json at all"
    assert extract_dialogue_for_judging(reply) == reply


def test_extract_dialogue_empty_input():
    assert extract_dialogue_for_judging("") == ""


def test_extract_dialogue_multiple_dash_lines():
    # Only the FIRST `\n---` ends the dialogue; later occurrences are
    # part of the metadata block.
    reply = "first line\n---\n{\"action\": \"---\"}"
    assert extract_dialogue_for_judging(reply) == "first line"


def test_extract_dialogue_dash_at_very_start_no_newline():
    # Codex review #PRRT_kwDORqdnvs6CzukN: model emits metadata-only
    # output with no leading newline before `---`. Runtime splits on
    # `---` itself; bench must match.
    reply = "--- \n{\"action\": \"shrugs\"}"
    assert extract_dialogue_for_judging(reply) == ""


def test_extract_dialogue_markdown_json_fence():
    # Codex review #PRRT_kwDORqdnvs6CzukS: Anthropic-style fence wrap.
    # Runtime strips fences in `parish_npc::strip_json_fence`; bench
    # must match.
    reply = '```json\n{"dialogue": "Aye, fine day.", "action": "nods"}\n```'
    assert extract_dialogue_for_judging(reply) == "Aye, fine day."


def test_extract_dialogue_bare_markdown_fence():
    reply = '```\n{"dialogue": "Plain fence", "action": "nods"}\n```'
    assert extract_dialogue_for_judging(reply) == "Plain fence"


def test_extract_dialogue_truncated_json_recovery():
    # Codex review #PRRT_kwDORqdnvs6CzukL: max_tokens hit mid-stream.
    # Runtime's heuristic recovers the dialogue prefix; bench must match.
    reply = '{"dialogue": "Ah, the toothache is a cruel thing", "actio'
    assert extract_dialogue_for_judging(reply) == "Ah, the toothache is a cruel thing"


def test_extract_dialogue_truncated_json_with_escaped_quotes():
    reply = '{"dialogue": "She said \\"hi\\" to me", "act'
    assert extract_dialogue_for_judging(reply) == 'She said "hi" to me'


def test_extract_dialogue_json_with_trailing_junk():
    # raw_decode tolerates trailing junk after the closing brace; the
    # old hand-rolled parser only worked if the reply ended at `}`.
    reply = '{"dialogue": "Hello", "action": "wave"}\n\nextra prose'
    assert extract_dialogue_for_judging(reply) == "Hello"


def test_extract_dialogue_fenced_truncated_json():
    # Belt-and-braces: fence + truncation both present.
    reply = '```json\n{"dialogue": "fenced and cut"'
    assert extract_dialogue_for_judging(reply) == "fenced and cut"


# ---------------------------------------------------------------------------
# runner
# ---------------------------------------------------------------------------

def main() -> int:
    tests = [v for k, v in globals().items() if k.startswith("test_") and callable(v)]
    failed = 0
    for t in tests:
        try:
            t()
        except AssertionError as e:
            print(f"FAIL {t.__name__}: {e}")
            failed += 1
        except Exception as e:
            print(f"ERROR {t.__name__}: {type(e).__name__}: {e}")
            failed += 1
        else:
            print(f"OK   {t.__name__}")
    print(f"\n{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
