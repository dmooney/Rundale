# Troubleshooting — reporting a bug

When something looks off — a strange NPC reply, a stuck turn, a mysterious
failure — there is now a reproducible artefact to attach to the report.
Every backend process writes two paired JSONL files to
`{your saves directory}/inference_logs/`:

```
{timestamp-pid}.jsonl            ← every LLM call (prompt, response, model, timing)
{timestamp-pid}.transcript.jsonl ← user-visible chat events (dialogue, travel, off-screen beats)
```

The transcript is fed from the same `GameEvent` bus that drives the
per-character and per-location journals, so it captures NPC dialogue,
player travel, off-screen NPC interactions, weather shifts and festivals.
Every `npc_dialogue` line carries a `parish.request_id` matching the
corresponding inference-log line, so a weird NPC reply can be grepped
straight back to the full prompt + response that produced it.

**To file a bug report**: reproduce the issue, then zip the
`inference_logs/` folder (or just the matched pair from the relevant
session) and attach it to the GitHub issue along with a description of
what you saw versus what you expected.

**Privacy**: known API-key shapes (OpenAI `sk-…`, Anthropic `sk-ant-…`,
Groq, AWS, Google, GitHub PATs, `Bearer …` headers) are scrubbed before
they hit disk and replaced with `[REDACTED:*]` markers. Other user-typed
content (player input, NPC names, places) is kept verbatim so the bug
remains reproducible — review the files before sharing.

**Opt-out**: pass `--no-inference-log` on the CLI, set the env var
`PARISH_INFERENCE_LOG=off`, or set `[engine.inference] log_to_disk = false`
in your `parish.toml`. The in-game slash command `/inference-log on|off|
status|path` toggles writes at runtime.
