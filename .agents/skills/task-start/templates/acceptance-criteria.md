# Acceptance Criteria: <TASK_ID>

## Task

<one-paragraph restatement of what the task asks for — what the player or
system experiences differently after this change>

## Criteria

- <criterion 1> — observable via: <command or game action that proves it>
- <criterion 2> — observable via: <command or game action that proves it>
- ...

## Verification script

Run: `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script parish/testing/fixtures/play_<TASK_ID>.txt`

Expected signals in output:
- <JSON field or text pattern that confirms criterion 1>
- <JSON field or text pattern that confirms criterion 2>
