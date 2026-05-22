# Acceptance Criteria: ui-scene-spam

## Task

The Kilteevan scene description currently re-emits on every world-update tick without location change, cluttering the text log with duplicated prose. The fix deduplicates scene-description entries by tracking the last-seen location name in the text-log subscriber — a scene only appends when arriving at a new location or on explicit `look` command. Movement, explicit look, session start, and save load correctly trigger scene-description output.

## Criteria

- **Criterion 1: No re-emit on idle turns** — After session start or location arrival, N consecutive player inputs without movement (dialogue only) produce exactly 1 scene-description entry, not N. Observable via: Session start at Kilteevan followed by multiple dialogue turns; verify text log contains the Kilteevan prose once, not repeated.
- **Criterion 2: Scene emits on movement arrival** — Moving to a new location appends exactly 1 new scene-description entry. Observable via: Player movement from Kilteevan to an adjacent location; verify a new prose entry appears in the text log.
- **Criterion 3: Explicit look always appends** — Typing `look` or `look around` always appends a scene-description entry to the text log, even if the same location prose was shown moments ago (idempotent re-print OK). Observable via: After a dialogue turn at Kilteevan, type `look`; verify scene prose appears in the log again.
- **Criterion 4: Test fixture present** — A new test fixture `parish/testing/fixtures/play_ui-scene-spam.txt` exercises criteria 1 and 2 (dialogue without movement, then movement). Observable via: `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script parish/testing/fixtures/play_ui-scene-spam.txt` completes and logs scene description once on load, once on movement.

## Verification script

Run: `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script parish/testing/fixtures/play_ui-scene-spam.txt`

Expected signals in output:
- First scene description line after session start (location name logged)
- Exactly one scene description from idle dialogue turns
- Second scene description after movement command
- Log structure shows `{"source":"system",...}` entries with location prose only on location change
