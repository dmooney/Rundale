---
name: prove
description: Prove a gameplay feature works by play-testing with the script harness. Write a targeted script, run it, read the output critically, and fix any issues found.
disable-model-invocation: false
argument-hint: <feature description>
---

Prove that a gameplay feature works at runtime — not just that tests pass.

## Steps

1. **Write a targeted test script** at `testing/fixtures/play_prove.txt` that exercises the feature from a player's perspective. Use `/wait` to advance time, move between locations, and use `/time`, `/status`, `/debug clock`, `/debug npcs`, `look`, `/npcs` to observe effects. Design the script to make the feature's impact visible in the output.

2. **Run it**: `cargo run -- --script testing/fixtures/play_prove.txt`

3. **Read the JSON output critically**. For each line, ask:
   - Do values change when expected? (e.g., weather transitions, NPC relocations)
   - Do descriptions read naturally? Would a player find this text grammatical and immersive?
   - Does NPC behavior respond correctly to the new feature?
   - Are any fields empty, nonsensical, or stuck at their initial value?

4. **Fix what you find.** Common issues:
   - New tick/update logic added to `parish-server` and `headless` but **not to the test harness** (`crates/parish-cli/src/testing.rs`) — the script harness has its own game loop in `advance_time()` and `Command::Tick`.
   - Large `/wait` jumps that only call your logic once at the final timestamp instead of at each intermediate step.
   - Template interpolation producing ungrammatical text when new enum variants have multi-word Display strings.
   - Features that silently no-op because a required field isn't wired up in a constructor.

5. **Re-run until the output proves the feature is live.** If you had to fix something, re-run the script and re-check. Don't stop at "tests pass" — stop at "I can see it working in the game output."

6. **Report** a brief summary: what you tested, what the output showed, and any fixes made.

## Think Like the Player

Would someone who doesn't know the code understand what's happening? Would a game creator accept this output quality? If the answer is no, the feature isn't done.
