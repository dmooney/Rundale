# Judge Verdict: demo-intent-leak

Verdict: sufficient

Technical debt: clear

Acceptance criteria: met

## Detailed Verdict

### Criterion 1: No command-form chat input in demo turns

**Status**: Met

Live demo transcript shows 3+ turns, all with natural first-person speech:
- Turn 1 (line 438): "Good mornin' to all. Might I speak with the parish priest, Fr. Declan Tierney?" ✓
- Turn 2 (line 476): "Good mornin', Father. I'm Aiden Carney, come from over by the Shannon. Might I ask ye some questions about the parish and the folk hereabouts?" ✓
- Turn 3 (line 517): "and souls as devout as any. I've seen much of this parish and its people. So, ask away, Aiden." ✓

Pattern search for intent-leak markers (`ask about`, `tell`, `whisper`, `look at`, `go to`) yields zero matches.

### Criterion 2: Guard rejects intent-leak examples

**Status**: Met

Unit test `is_command_form_intent_leak_rejects_ask_patterns()` passes, verifying rejection of:
- `"ask about the places nearby that are worth visiting"` (the exact bug from #1009) ✓
- `"ask about the harvest"` ✓
- `"ask the priest"`, `"ask a stranger"`, `"ask if anyone knows"` ✓

### Criterion 3: Bare system commands still pass

**Status**: Met

Unit test `is_command_form_intent_leak_accepts_bare_commands()` passes, verifying that:
- `"look"`, `"wait"`, `"go"`, `"listen"`, `"think"` are NOT flagged as leaks ✓

### Criterion 4: Natural speech passes unchanged

**Status**: Met

Unit test `is_command_form_intent_leak_accepts_natural_speech()` passes, verifying that natural speech like:
- `"Good mornin'. Might I look about the village a while?"` ✓
- `"I've come from up the road. What news do ye have hereabouts?"` ✓
- `"Might I ask about the harvest, then?"` ✓

All pass through unchanged, with embedded dialogue verbs ("ask", "look") properly recognized as natural speech when wrapped in first-person context.

## Implementation Review

### Code Changes

1. **Demo prompt hardening** (`mods/rundale/demo-prompt.txt`):
   - Added CRITICAL instruction forbidding command-form intent descriptions
   - Explicitly teaches first-person speech patterns

2. **Runtime guard** (`parish-tauri/src/commands.rs`):
   - New function: `is_command_form_intent_leak(text: &str) -> bool`
   - Detects dialogue-form intent leaks: `ask about`, `tell`, `whisper` (but NOT movement commands like `go to`)
   - Integrated into all four extraction patterns in `extract_action_from_response()`
   - Pattern 4 fallback now rejects JSON that was already rejected by Pattern 2

3. **System prompt examples** (`parish-tauri/src/commands.rs`):
   - Changed from bare `"ask about the harvest"` to first-person `"Might I ask about the harvest, then?"`

4. **Test coverage**:
   - 8 new unit tests for guard function
   - 2 new unit tests for extract_action_from_response with guard integration
   - All 26 demo_tests pass

### Scope Adherence

- ✓ Only edited `mods/rundale/demo-prompt.txt` (demo prompt tightening)
- ✓ Only edited `parish-tauri/src/commands.rs` (guard implementation + examples)
- ✓ Proof bundle created under `docs/proofs/demo-intent-leak/`
- ✓ Verification fixture at `parish/testing/fixtures/play_demo-intent-leak.txt`
- ✓ No changes to parser logic outside extract path
- ✓ No changes to game engine, world, NPC systems

### Potential Regressions

None observed. Movement commands (`go to X`, `look at X`) are correctly allowed. Bare system commands pass through. Natural dialogue with embedded dialogue verbs are preserved.
