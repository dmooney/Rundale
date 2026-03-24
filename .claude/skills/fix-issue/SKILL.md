---
name: fix-issue
description: Work through a GitHub issue end-to-end — diagnose, implement, test, and verify. Pass the issue number as an argument.
argument-hint: <issue-number>
---

Work through GitHub issue #$ARGUMENTS end-to-end.

## Steps

1. **Fetch the issue**: Run `gh issue view $ARGUMENTS` to read the title, body, and labels.
2. **Understand the problem**: Research the relevant code. Identify the root cause or the feature gap.
3. **Plan the fix**: Outline what files need to change and what tests to add. Keep it minimal — only change what's needed.
4. **Implement**: Make the code changes. Follow the project's code style (cargo fmt, clippy clean, doc comments on public items).
5. **Add tests**: Write unit tests covering the new or changed behavior. Aim for comprehensive coverage of the fix.
6. **Run checks**: Execute `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`. All must pass.
7. **Game harness**: Run `cargo run -- --script tests/fixtures/test_walkthrough.txt` and verify the JSON output looks correct.
8. **Update docs**: Update any affected documentation (README.md, docs/, doc comments) to reflect the changes.
9. **Commit**: Create a conventional commit (e.g., `fix: resolve #$ARGUMENTS — <description>`).
10. **Report**: Summarize what was changed and confirm all checks passed.
