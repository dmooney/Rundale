Score this Rundale Tier 2 simulation session. The session uses /wait commands
to advance the clock and checks whether background NPC simulation fires.

Evaluate on 3 axes (score 1–5 each):

1. **simulation_activity** — After /wait commands, did the game log show evidence
   of background NPC activity (NPCs moving, states updating, events logged)?
   Score 5 if clear simulation events appear, 1 if the world is completely static.
2. **npc_state_change** — Between /npcs snapshots, did NPC lists or their
   described states change (different NPCs present, mood or activity changes)?
   Score 3 if NPCs are consistent but plausibly stable; 1 if identical across all snapshots.
3. **time_progression** — Did /time output show the clock advancing correctly
   after /wait commands, with season/time-of-day updating as expected?

Pass threshold: simulation_activity ≥ 3 AND time_progression ≥ 4.

Respond ONLY with this JSON (no prose, no markdown fences):
{
  "simulation_activity": <1-5>,
  "npc_state_change": <1-5>,
  "time_progression": <1-5>,
  "verdict": "pass" or "fail",
  "notes": "<one sentence explaining the verdict>"
}
