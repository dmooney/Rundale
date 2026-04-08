---
name: game-test
description: Run the Parish game test harness to verify game mechanics work correctly. Use after changing world, movement, NPC, or input code.
disable-model-invocation: true
argument-hint: [script-file]
---

Run the Parish GameTestHarness to verify game behavior.

If `$ARGUMENTS` is provided, use that as the script file path. Otherwise, run the default walkthrough.

## Steps

1. **Build first**: Run `cargo build` to ensure the project compiles.
2. **Run the test script**:
   - If a script file was specified: `cargo run -- --script $ARGUMENTS`
   - Otherwise: `cargo run -- --script testing/fixtures/test_walkthrough.txt`
3. **Inspect the JSON output** line by line. Check that:
   - Movement results have valid `to` locations and reasonable `minutes` values
   - Look results contain non-empty descriptions
   - System commands return expected responses
   - No unexpected errors or panics appear
4. **Run additional fixture scripts** if the default passed:
   - `cargo run -- --script testing/fixtures/test_movement_errors.txt`
   - `cargo run -- --script testing/fixtures/test_commands.txt`
   - `cargo run -- --script testing/fixtures/test_speed.txt`
5. **Report results**: Summarize which scripts passed and flag any anomalies in the JSON output.
