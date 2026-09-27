Score this Rundale smoke-test session. The session is a short 10-turn play that
exercises basic movement and a single NPC exchange.

Evaluate on 3 axes (score 1–5 each):

1. **completion** — Did all commands produce a meaningful response (no silent
   failures, no error messages, no empty descriptions)?
2. **npc_response** — Did any NPC interaction that occurred produce a
   period-appropriate response in the voice of a rural Irish villager (1820)?
   Score 3 if no NPC interaction occurred.
3. **period_language** — Were all game descriptions and NPC responses free of
   anachronisms (no modern idiom, no non-English script leakage)?

Pass threshold: all axes ≥ 3 AND no axis = 1.

Respond ONLY with this JSON (no prose, no markdown fences):
{
  "completion": <1-5>,
  "npc_response": <1-5>,
  "period_language": <1-5>,
  "verdict": "pass" or "fail",
  "notes": "<one sentence explaining the verdict>"
}
