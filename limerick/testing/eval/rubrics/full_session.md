Score this Rundale full-session eval. The session is a 50-turn free-play
session covering movement, dialogue, reactions, and time progression.

Evaluate on 6 axes (score 1–5 each):

1. **world_coherence** — Did the game world behave consistently throughout?
   Locations matched their descriptions, time advanced plausibly, NPCs were
   present when expected.
2. **inference_reliability** — Did inference calls succeed across all four
   categories (dialogue, simulation, intent, reaction)? Penalise timeouts,
   empty responses, or repeated identical outputs.
3. **dialogue_quality** — Across NPC interactions, were responses period-
   appropriate, character-distinct, and responsive to player input?
4. **period_authenticity** — Was the overall session free of anachronisms in
   language, names, places, and cultural references?
5. **player_agency** — Did the player's actions visibly affect the world?
   Did choices matter (different routes gave different descriptions, NPCs
   remembered prior exchanges)?
6. **session_stability** — Did the session complete without crashes, hangs,
   or unrecoverable errors?

Pass threshold: mean ≥ 3.5 AND inference_reliability ≥ 4 AND session_stability ≥ 4.

Respond ONLY with this JSON (no prose, no markdown fences):
{
  "world_coherence": <1-5>,
  "inference_reliability": <1-5>,
  "dialogue_quality": <1-5>,
  "period_authenticity": <1-5>,
  "player_agency": <1-5>,
  "session_stability": <1-5>,
  "verdict": "pass" or "fail",
  "notes": "<one sentence explaining the verdict>"
}
