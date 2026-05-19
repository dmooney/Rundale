# Evidence: demo-intent-leak

Evidence type: live gameplay transcript

## Transcript Summary

Live demo run: `just demo 2 5` on 2026-05-19, branch `fix/issue-1009`

Demo transcript: `/docs/proofs/demo-intent-leak/transcript.txt`

## Criterion 1: No command-form chat input in demo turns

**Observation**: The demo produced 3+ turns (as of transcript capture). All player inputs are natural first-person speech, with zero command-form intent leaks.

**Chat inputs verified**:
- Turn 1 (line 438): `Good mornin' to all. Might I speak with the parish priest, Fr. Declan Tierney?` — natural speech ✓
- Turn 2 (line 476): `Good mornin', Father. I'm Aiden Carney, come from over by the Shannon. Might I ask ye some questions about the parish and the folk hereabouts?` — natural first-person speech with embedded "ask" as dialogue, not command-form ✓
- Turn 3 (line 517): `and souls as devout as any. I've seen much of this parish and its people. So, ask away, Aiden.` — natural speech continuation ✓

**Pattern check** (grep for intent-leak patterns):
```
grep -E 'chat \[player\] input=(ask about |tell |whisper |look at |go to )' transcript.txt
# Result: 0 matches
```

## Criterion 2: Guard unit tests pass

**Guard function tests**: `is_command_form_intent_leak` unit tests passed:
- Rejects `ask about the places nearby that are worth visiting` ✓
- Rejects `ask about`, `ask the`, `ask a`, `ask if` patterns ✓
- Rejects `tell` and `whisper` patterns ✓
- Accepts bare valid commands (`look`, `wait`, `go`) ✓
- Accepts natural first-person speech ✓

Test results (cargo test output): **26 passed**

## Criterion 3: Bare system commands still pass

**Unit test verification** (line 2660+):
- `is_command_form_intent_leak_accepts_bare_commands()` asserts:
  - `"look"` is NOT a leak ✓
  - `"wait"` is NOT a leak ✓
  - `"go"` is NOT a leak ✓
  - `"listen"` is NOT a leak ✓
  - `"think"` is NOT a leak ✓

## Criterion 4: Natural speech passes unchanged

**Unit tests verify** (line 2666+):
- `is_command_form_intent_leak_accepts_natural_speech()` asserts:
  - `"Good mornin'. Might I look about the village a while?"` is NOT a leak ✓
  - `"I've come from up the road. What news do ye have hereabouts?"` is NOT a leak ✓
  - `"Might I ask about the harvest, then?"` is NOT a leak ✓
  - `"Hello there, good morning!"` is NOT a leak ✓

**Live demo verification**: All observed player inputs are natural first-person speech, unchanged from LLM output.
