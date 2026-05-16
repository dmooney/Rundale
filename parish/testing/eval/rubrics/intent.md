Score this Rundale intent-parsing session. The session sends 20+ varied
phrasings of movement, look, and greeting commands to test the intent parser.

Evaluate on 3 axes (score 1–5 each):

1. **classification_accuracy** — What fraction of player commands produced the
   correct result type (Moved for movement, Looked for look, NpcResponse or
   greeting for social commands)? Score: 5=≥90%, 4=≥80%, 3=≥70%, 2=≥60%, 1=<60%.
2. **rephrase_robustness** — Did the game correctly handle non-standard phrasings
   ("make my way to", "wander over to", "strike up a conversation")? Score 5
   if all novel phrasings worked, 1 if most failed.
3. **error_recovery** — When a command was unrecognised, did the game respond
   gracefully (helpful message, no crash, no silent hang)?

Pass threshold: all axes ≥ 3 AND classification_accuracy ≥ 4.

Respond ONLY with this JSON (no prose, no markdown fences):
{
  "classification_accuracy": <1-5>,
  "rephrase_robustness": <1-5>,
  "error_recovery": <1-5>,
  "verdict": "pass" or "fail",
  "notes": "<one sentence explaining the verdict>"
}
