Score this Rundale reaction session. The session moves the player to several
populated locations and waits briefly at each to trigger NPC arrival reactions.

Evaluate on 3 axes (score 1–5 each):

1. **reaction_firing** — Did NPCs at each visited location produce arrival
   reaction text (a greeting or acknowledgement)? Score 5 if every populated
   location produced reactions, 1 if none did.
2. **reaction_quality** — Were reaction texts period-appropriate (1820 rural
   Irish speech patterns, no modern language, distinct from generic greetings)?
   Score 3 if reactions fired but were generic.
3. **npc_variety** — Did different NPCs react differently, suggesting distinct
   personalities, rather than all giving identical boilerplate?

Pass threshold: reaction_firing ≥ 3 AND all axes ≥ 2.

Respond ONLY with this JSON (no prose, no markdown fences):
{
  "reaction_firing": <1-5>,
  "reaction_quality": <1-5>,
  "npc_variety": <1-5>,
  "verdict": "pass" or "fail",
  "notes": "<one sentence explaining the verdict>"
}
