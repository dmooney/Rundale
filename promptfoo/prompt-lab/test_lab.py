#!/usr/bin/env python3
"""Offline tests for the prompt lab: no model, judge or network calls."""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

LAB = Path(__file__).resolve().parent
sys.path.insert(0, str(LAB))

from promptlab import content, judges, models, render, research  # noqa: E402
from promptlab import variant as variant_mod  # noqa: E402


class ContentTests(unittest.TestCase):
    def test_requirements_parse_all_twenty_from_the_doc(self):
        reqs = content.requirements()
        self.assertEqual(sorted(reqs, key=lambda k: int(k[1:])), [f"B{i}" for i in range(1, 21)])
        self.assertTrue(reqs["B5"].startswith("Admit what they don't know."))

    def test_characters_match_the_mod(self):
        npcs = {
            n["name"]: n
            for n in json.loads((content.MOD_DIR / "npcs.json").read_text(encoding="utf-8"))["npcs"]
        }
        for key, char in content.characters().items():
            npc = npcs[char["name"]]
            for field in ("age", "pronouns", "occupation", "personality", "mood"):
                self.assertEqual(char[field], npc[field], f"{key}.{field}")
            self.assertEqual(char["appearance"], npc["brief_description"])
            self.assertEqual(char["knows"], npc["knowledge"])
            names = {n["id"]: n["name"] for n in npcs.values()}
            places = {
                loc["id"]: loc["name"]
                for loc in json.loads((content.MOD_DIR / "world.json").read_text(encoding="utf-8"))[
                    "locations"
                ]
            }
            self.assertEqual(char["home"], places[npc["home"]], f"{key}.home")
            self.assertEqual(char["workplace"], places[npc["workplace"]], f"{key}.workplace")
            day = [
                f"{e['start_hour']:02d}:00-{e['end_hour']:02d}:59 {e['activity']} ({places[e['location']]})"
                for e in npc["seasonal_schedule"][0]["entries"]
            ]
            self.assertEqual(char["day"], day, f"{key}.day")
            rels = [
                (r["who"], r["feeling"].rsplit("(", 1)[1].rstrip(")"))
                for r in char["relationships"]
            ]
            self.assertEqual(
                rels,
                [(names[r["target_id"]], str(r["strength"])) for r in npc["relationships"]],
                f"{key}.relationships",
            )
            self.assertTrue(
                3 <= len(char["backstory"]) <= 5, f"{key} needs three to five backstory lines"
            )

    def test_village_matches_the_mod(self):
        world = {
            loc["name"]: loc
            for loc in json.loads((content.MOD_DIR / "world.json").read_text(encoding="utf-8"))[
                "locations"
            ]
        }
        festivals = json.loads((content.MOD_DIR / "festivals.json").read_text(encoding="utf-8"))
        village = content.village()
        self.assertEqual({p["name"] for p in village["places"]}, set(world))
        for place in village["places"]:
            self.assertEqual(
                place["ways"], [c["path_description"] for c in world[place["name"]]["connections"]]
            )
            self.assertEqual(place["story"], world[place["name"]]["mythological_significance"])
        self.assertEqual([f["name"] for f in village["festivals"]], [f["name"] for f in festivals])

    def test_cases_are_well_formed_and_split(self):
        cases = content.all_cases()
        ids = [c.id for c in cases]
        self.assertEqual(len(ids), len(set(ids)))
        reqs = content.requirements()
        for case in cases:
            self.assertIn(case.split, content.SPLITS)
            self.assertIn(case.speaker, content.characters())
            self.assertIn(case.addressing, content.ADDRESSING)
            self.assertNotIn(case.speaker, case.present)
            self.assertTrue(set(case.tags) <= set(reqs), case.id)
            for turn in case.conversation:
                self.assertTrue(turn.who == "player" or turn.who in content.characters(), case.id)
        self.assertGreaterEqual(len(content.cases("heldout")), 6)
        self.assertGreaterEqual(len(content.cases("train")), 15)

    def test_everyone_is_where_their_day_puts_them(self):
        for case in content.all_cases():
            for key in (case.speaker, *case.present):
                _, place = content.activity_at(key, case.clock)
                self.assertEqual(
                    place, case.location, f"{case.id}: {key} is at {place} at {case.clock}"
                )

    def test_training_loads_never_return_heldout(self):
        self.assertTrue(all(c.split == "train" for c in content.cases("train")))
        with self.assertRaises(ValueError):
            content.cases("all")

    def test_case_weather_is_a_value_the_engine_emits(self):
        # limerick-types `Weather` Display strings.
        emitted = {"Clear", "Partly Cloudy", "Overcast", "Light Rain", "Heavy Rain", "Fog", "Storm"}
        for case in content.all_cases():
            self.assertIn(case.weather, emitted, case.id)

    def test_part_of_day_mirrors_the_engine(self):
        cases = {
            "05:00": "Dawn",
            "07:00": "Morning",
            "12:30": "Midday",
            "14:00": "Afternoon",
            "18:15": "Dusk",
            "19:00": "Night",
            "23:00": "Midnight",
            "03:00": "Midnight",
        }
        for clock, part in cases.items():
            self.assertEqual(content.part_of_day(clock), part, clock)


class RenderTests(unittest.TestCase):
    def setUp(self):
        self.brief = variant_mod.load_named("brief")
        self.case = content.cases("train", ids={"t03-greet-roisin-after-micheal"})[0]

    def test_production_shape(self):
        for v in (render.BASELINE, self.brief):
            request = render.render(v, self.case)
            self.assertTrue(request.user.startswith("Endpoint input values:\n{"))
            self.assertEqual(request.output_schema["required"], ["dialogue"])
            json.loads(request.user.split("\n", 1)[1])

    def test_baseline_uses_v1_instructions_and_input_shape(self):
        request = render.render(render.BASELINE, self.case)
        endpoint = content.v1_endpoint()
        self.assertEqual(request.system, endpoint["instructions"])
        schema = endpoint["inputSchema"]
        self.assertEqual(set(request.values) - set(schema["properties"]), set())
        self.assertEqual(set(schema["required"]) - set(request.values), set())
        exchange = request.values["recentConversation"][0]
        self.assertEqual(exchange["player_input"], "God bless all here.")
        self.assertEqual(exchange["speaker_name"], "Mícheál Connolly")

    def test_brief_static_sections_first_and_player_line_last(self):
        request = render.render(self.brief, self.case)
        labels = self.brief["labels"]
        sent = list(json.loads(request.user.split("\n", 1)[1]))
        self.assertEqual(sent[-2:], [labels["moment"], labels["player_line"]])
        self.assertEqual(request.values[labels["player_line"]], self.case.player_line)

    def test_keys_are_sorted_as_the_phone_sends_them(self):
        # mobile/RundaleBridge LimerickRuntime.swift encodes input with .sortedKeys.
        for v in (render.BASELINE, self.brief):
            body = render.render(v, self.case).user.split("\n", 1)[1]
            self.assertEqual(
                body,
                json.dumps(
                    json.loads(body), ensure_ascii=False, separators=(",", ":"), sort_keys=True
                ),
            )

    def test_static_prefix_is_shared_across_cases_for_one_character(self):
        a, b = (content.cases("train", ids={i})[0] for i in ("t01-greet-peig", "t07-father-murphy"))
        ra, rb = render.render(self.brief, a), render.render(self.brief, b)
        self.assertEqual(ra.system, rb.system)
        moment = json.dumps(self.brief["labels"]["moment"])
        self.assertEqual(ra.user.split(moment)[0], rb.user.split(moment)[0])

    def test_character_never_receives_another_characters_private_content(self):
        peig = content.cases("train", ids={"t01-greet-peig"})[0]
        text = render.render(self.brief, peig).user
        for key, char in content.characters().items():
            if key == "peig":
                continue
            self.assertNotIn(char["personality"], text)
            for line in char["backstory"] + char["knows"]:
                self.assertNotIn(line, text)

    def test_layouts_and_switches(self):
        prose = render.render({**self.brief, "layout": "prose"}, self.case)
        self.assertTrue(all(isinstance(v, str) for v in prose.values.values()))
        no_past = render.render({**self.brief, "backstory": False}, self.case).user
        self.assertNotIn(content.characters()["roisin"]["backstory"][0], no_past)
        shaded = render.render({**self.brief, "common_knowledge": "shaded"}, self.case).user
        self.assertIn(content.common_knowledge()["shaded"]["roisin"], shaded)

    def test_anachronism_flag_only_where_set_and_enabled(self):
        flagged = content.cases("train", ids={"t09-telephone"})[0]
        cue = self.brief["wording"]["anachronism_flag"]
        self.assertIn(cue, render.render(self.brief, flagged).user)
        self.assertNotIn(cue, render.render(self.brief, self.case).user)
        self.assertNotIn(cue, render.render(variant_mod.load_named("brief-noflag"), flagged).user)

    def test_judge_reference_is_independent_of_the_variant(self):
        ref = render.judge_reference(self.case)
        text = json.dumps(ref, ensure_ascii=False)
        self.assertIn(content.characters()["roisin"]["backstory"][0], text)
        self.assertIn(content.common_knowledge()["shaded"]["roisin"], text)


class VariantTests(unittest.TestCase):
    def setUp(self):
        self.start = variant_mod.load_named("brief")

    def test_shipped_variants_validate_and_pass_the_fact_guard(self):
        for path in content.PROMPTS_DIR.glob("*/variant.json"):
            v = variant_mod.load(path)
            self.assertEqual(v["name"], path.parent.name)
            self.assertEqual(
                variant_mod.fact_guard(
                    self.start, {**v, "anachronism_flag": self.start["anachronism_flag"]}
                ),
                [],
            )

    def test_moment_and_player_line_must_sort_last(self):
        for key, label in (
            ("moment", "anHourNow"),
            ("player_line", "aLine"),
            ("setting", "zSetting"),
        ):
            bad = copy.deepcopy(self.start)
            bad["labels"][key] = label
            with self.subTest(key=key), self.assertRaises(variant_mod.VariantError):
                variant_mod.validate(bad)
        bad = copy.deepcopy(self.start)
        bad["labels"]["village"] = "Village"
        with self.assertRaises(variant_mod.VariantError):
            variant_mod.validate(bad)

    def test_rewrite_may_change_wording_and_format(self):
        proposal = {
            "instructions": self.start["instructions"].replace(
                "Be that person.", "Become that person."
            ),
            "layout": "prose",
            "common_knowledge": "shaded",
        }
        merged = variant_mod.apply_rewrite(self.start, self.start, proposal, "brief-r01")
        self.assertEqual(merged["layout"], "prose")
        self.assertEqual(merged["name"], "brief-r01")

    def test_rewrite_may_not_add_world_facts(self):
        for addition in ("Peig is the eldest.", "Remember Athlone.", "It is 1820."):
            with self.subTest(addition=addition):
                proposal = {"instructions": self.start["instructions"] + " " + addition}
                with self.assertRaises(variant_mod.VariantError):
                    variant_mod.apply_rewrite(self.start, self.start, proposal, "x")

    def test_rewrite_may_not_name_people_from_the_test_lines(self):
        for addition in ("There is no Father Murphy.", "Greet Seán kindly."):
            with self.subTest(addition=addition):
                proposal = {"instructions": self.start["instructions"] + " " + addition}
                with self.assertRaises(variant_mod.VariantError):
                    variant_mod.apply_rewrite(self.start, self.start, proposal, "x")

    def test_ordinary_capitalised_words_are_not_facts(self):
        proposal = {
            "instructions": self.start["instructions"]
            + " Their words matter. May you speak plainly."
        }
        variant_mod.apply_rewrite(self.start, self.start, proposal, "x")

    def test_locked_switches_survive_a_rewrite(self):
        merged = variant_mod.apply_rewrite(
            self.start, self.start, {"backstory": False, "anachronism_flag": False}, "x"
        )
        self.assertTrue(merged["backstory"])
        self.assertTrue(merged["anachronism_flag"])


class JudgeTests(unittest.TestCase):
    def test_extract_json_from_chatter(self):
        self.assertEqual(
            judges.extract_json('Sure.\n```json\n{"score": 80, "reason": "a {brace}"}\n```'),
            {"score": 80, "reason": "a {brace}"},
        )
        with self.assertRaises(ValueError):
            judges.extract_json("no json here")

    def test_judge_prompt_has_tags_expectation_and_true_world(self):
        case = content.cases("train", ids={"t07-father-murphy"})[0]
        prompt = judges.judge_prompt(case, "Father who?")
        self.assertIn("B5: Admit what they don't know.", prompt)
        self.assertIn(case.expect, prompt)
        self.assertIn("Letter Office", prompt)
        self.assertNotIn("B1:", prompt)

    def test_judge_retries_then_records_failure(self):
        case = content.cases("train")[0]
        with mock.patch.object(
            judges,
            "ask",
            side_effect=[("garbage", 0.0), ('{"score": 75, "misses": [], "reason": "ok"}', 0.01)],
        ):
            verdict = judges.judge("claude", {"kind": "claude-cli"}, case, "Hello.")
        self.assertTrue(verdict.ok)
        self.assertEqual(verdict.score, 75)
        self.assertAlmostEqual(verdict.cost_usd, 0.01)
        with mock.patch.object(judges, "ask", return_value=('{"score": 140}', 0.0)):
            self.assertFalse(judges.judge("claude", {"kind": "claude-cli"}, case, "Hello.").ok)

    def test_claude_billing_envelope_is_an_error(self):
        envelope = json.dumps(
            {"type": "result", "is_error": True, "result": "Credit balance is too low"}
        )
        proc = mock.Mock(stdout=envelope, stderr="", returncode=1)
        with (
            mock.patch("subprocess.run", return_value=proc),
            self.assertRaisesRegex(RuntimeError, "Credit balance"),
        ):
            judges._claude({"model": "sonnet"}, "hi")

    def test_codex_falls_back_to_billed_api_when_missing(self):
        cfg = research.load_config()["judges"]["gpt"]
        with mock.patch("shutil.which", return_value=None):
            self.assertEqual(judges.resolve(cfg)["kind"], "openai-api")
        with mock.patch("shutil.which", return_value="/usr/bin/codex"):
            self.assertEqual(judges.resolve(cfg)["kind"], "codex-cli")


class ModelTests(unittest.TestCase):
    def test_unpriced_model_is_refused(self):
        with self.assertRaises(models.ModelConfigError):
            models.price_of({"model": "x"})

    def test_cost_from_usage(self):
        cfg = research.load_config()["models"]["production"]
        self.assertAlmostEqual(models.cost(cfg, 1_000_000, 100_000), 0.3 + 0.25)

    def test_dialogue_must_be_the_schema_object(self):
        self.assertEqual(models._dialogue('{"dialogue": "Good day."}'), "Good day.")
        for bad in ('{"dialogue": ""}', '{"dialogue": "x", "mood": "y"}', "Good day.", None):
            self.assertIsNone(models._dialogue(bad))

    def test_gemini_request_matches_the_production_adapter(self):
        captured = {}

        def fake_post(url, body, headers, **kw):
            captured.update(url=url, body=body)
            return {
                "candidates": [
                    {
                        "content": {"parts": [{"text": '{"dialogue": "Good day."}'}]},
                        "finishReason": "STOP",
                    }
                ],
                "usageMetadata": {
                    "promptTokenCount": 1000,
                    "candidatesTokenCount": 10,
                    "thoughtsTokenCount": 5,
                },
            }

        cfg = research.load_config()["models"]["production"]
        case = content.cases("train")[0]
        request = render.render(variant_mod.load_named("brief"), case)
        with (
            mock.patch.dict("os.environ", {"GEMINI_API_KEY": "k"}),
            mock.patch.object(models, "post_json", fake_post),
        ):
            reply = models.generate(cfg, request)
        self.assertTrue(reply.ok)
        self.assertIn("gemini-3.5-flash-lite:generateContent", captured["url"])
        self.assertEqual(captured["body"]["systemInstruction"]["parts"][0]["text"], request.system)
        self.assertEqual(captured["body"]["contents"][0]["parts"][0]["text"], request.user)
        self.assertEqual(
            captured["body"]["generationConfig"]["responseJsonSchema"], request.output_schema
        )
        self.assertEqual(reply.output_tokens, 15)
        self.assertAlmostEqual(reply.cost_usd, (1000 * 0.3 + 15 * 2.5) / 1e6)


class LoopTests(unittest.TestCase):
    def test_plateau_rule(self):
        def hist(*gains):
            return [{"gain": None}] + [{"gain": g} for g in gains]

        self.assertFalse(research.plateaued(hist(1.0, 1.0), 2.0, 3))
        self.assertTrue(research.plateaued(hist(5.0, 1.0, -3.0, 1.9), 2.0, 3))
        self.assertFalse(research.plateaued(hist(1.0, 1.0, 2.5), 2.0, 3))

    def test_ledger_cap(self):
        ledger = research.Ledger(1.0)
        ledger.add(0.6)
        ledger.check()
        ledger.add(0.5)
        with self.assertRaises(research.SpendCapReached):
            ledger.check()

    def test_row_scoring(self):
        names = ["claude", "gpt"]

        def row(ok=True, **scores):
            return {
                "reply": {"ok": ok},
                "verdicts": {j: {"ok": True, "score": s} for j, s in scores.items()},
            }

        r = row(claude=80, gpt=70)
        research.finish_row(r, names, 20)
        self.assertEqual((r["score"], r["flagged"]), (75, False))
        r = row(claude=90, gpt=50)
        research.finish_row(r, names, 20)
        self.assertTrue(r["flagged"])
        r = row(ok=False)
        research.finish_row(r, names, 20)
        self.assertEqual(r["score"], 0.0)
        r = row(claude=80)
        research.finish_row(r, names, 20)
        self.assertTrue(r["flagged"])
        self.assertFalse(r["judged"], "one judge alone must not score a row")
        self.assertIsNone(r["score"])

    def test_failed_request_is_not_scored_as_the_prompts_miss(self):
        cfg = research.load_config()["models"]["production"]
        request = render.render(variant_mod.load_named("brief"), content.cases("train")[0])

        def quota(*_a, **_k):
            raise RuntimeError("HTTP 429: quota")

        with (
            mock.patch.dict("os.environ", {"GEMINI_API_KEY": "k"}),
            mock.patch.object(models, "post_json", quota),
        ):
            reply = models.generate(cfg, request)
        self.assertTrue(reply.transport_failed)
        row = {"reply": reply.to_json(), "verdicts": {}}
        research.finish_row(row, ["claude", "gpt"], 20)
        self.assertFalse(row["judged"])
        self.assertIsNone(row["score"])
        bad_json = models.Reply(
            "m", False, None, "not json", "no valid dialogue", 1, 1, 0.0, 0.1, "STOP"
        )
        row = {"reply": bad_json.to_json(), "verdicts": {}}
        research.finish_row(row, ["claude", "gpt"], 20)
        self.assertEqual(row["score"], 0.0)

    def test_judge_outage_pauses_the_loop_without_counting_the_round(self):
        cfg = research.load_config()
        cfg["research"]["samples_per_case"] = 1
        case = content.cases("train")[:4]
        ok = models.Reply("m", True, "Good day.", "{}", None, 10, 5, 0.001, 0.1, "STOP")

        def fake_judge(name, jcfg, c, reply):
            if name == "gpt":
                return judges.Verdict(name, False, None, error="usage limit")
            return judges.Verdict(name, True, 80.0)

        with (
            tempfile.TemporaryDirectory() as tmp,
            mock.patch.object(research, "OUTPUT_DIR", Path(tmp)),
            mock.patch.object(research, "load_config", return_value=cfg),
            mock.patch.object(models, "generate", return_value=ok),
            mock.patch.object(judges, "judge", fake_judge),
        ):
            state = research.run_loop(
                "o", "brief", stand_in=True, case_ids={c.id for c in case}, log=lambda *_: None
            )
            self.assertIn("outage", state["paused"])
            self.assertIsNone(state["stopped"])
            self.assertIsNone(state["baseline"])
            self.assertEqual(state["rounds"], [])
            self.assertGreater(state["spend_usd"], 0)

    def test_loop_keeps_best_stops_on_plateau_and_never_sees_heldout(self):
        """Drive run_loop with fake generation, judging and rewriting."""
        scores = iter([50.0, 60.0, 70.0, 71.0, 69.0, 70.5, 99.0])
        seen_cases: set[str] = set()

        def fake_score(variant, cases, cfg, ledger, out_dir, *, stand_in=False, log=print):
            seen_cases.update(c.id for c in cases)
            out_dir.mkdir(parents=True, exist_ok=True)
            if isinstance(variant, dict):
                (out_dir / "variant.json").write_text(json.dumps(variant), encoding="utf-8")
                (out_dir / "instructions.md").write_text(variant["instructions"], encoding="utf-8")
            score = next(scores)
            ledger.add(0.01)
            summary = {
                "score": score,
                "by_model": {},
                "by_requirement": {"B1": score},
                "flagged": [],
                "round_spend_usd": 0.01,
            }
            (out_dir / "summary.json").write_text(json.dumps(summary), encoding="utf-8")
            (out_dir / "rows.jsonl").write_text("", encoding="utf-8")
            return summary

        start = variant_mod.load_named("brief")

        def fake_propose(state, run_dir, cfg, start_v, train, log=print):
            v = copy.deepcopy(start)
            v["name"] = f"brief-r{len(state['rounds']):02d}"
            return v, {"reasoning": "test", "from": state["best"]["dir"]}

        with (
            tempfile.TemporaryDirectory() as tmp,
            mock.patch.object(research, "OUTPUT_DIR", Path(tmp)),
            mock.patch.object(research, "score_round", fake_score),
            mock.patch.object(research, "propose", fake_propose),
        ):
            state = research.run_loop("t", "brief", log=lambda *_: None)
            self.assertIn("plateau", state["stopped"])
            self.assertEqual([r["score"] for r in state["rounds"]], [60.0, 70.0, 71.0, 69.0, 70.5])
            self.assertEqual(state["best"]["round"], 2)
            self.assertEqual(state["baseline"]["score"], 50.0)
            self.assertTrue((Path(tmp) / "t" / "best-variant.json").exists())
            self.assertEqual(
                research.run_loop("t", "brief", log=lambda *_: None)["stopped"], state["stopped"]
            )
            report = research.report("t")
            self.assertIn("| 2 | `brief-r02` | 71.0 | +1.0 |", report)
        self.assertTrue(seen_cases)
        self.assertFalse(any(c.startswith("h") for c in seen_cases))


if __name__ == "__main__":
    unittest.main(verbosity=1)
