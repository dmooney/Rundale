---
name: 'source-command-todo-drain'
description: "Drain TODO.md demo-audit findings in parallel rounds \u2014 AC-first, live proof, attach bundle, retrigger CI as needed, land green PRs while next round is in flight."
---

# source-command-todo-drain

Use this skill when the user asks to run the migrated source command `todo-drain`.

## Command Template

Land fixes from `TODO.md` (Rundale demo-audit findings). Run rounds in parallel — start the next round while the previous PR's CI runs.

## Workflow per round

## 1. Sync + worktree

Never work in the main repo directory — other sessions may be using it. Switch into (or create) a worktree off latest `origin/main`:

```sh
git fetch origin main
git worktree add .codex/worktrees/round-<n> -b codex/round-<n> origin/main
```

Then move execution to that path so all subsequent commands run inside the worktree.

## 2. Pick TODO item

Open `TODO.md`. Prefer the smallest-scope unaddressed P0/P1. If the entry has a "revise" note pointing at a different ID, follow it.

## 3. Write acceptance criteria FIRST

Before any code change:

- `.proofs/todo-<id>/acceptance-criteria.md` — observable criteria, sized concretely (e.g. "`frequency_penalty: Option<f32>` field on `InferenceRequest`", not "improve repetition handling"). Include a "Deferred items" section listing anything intentionally punted from this round.
- `parish/testing/proofs/play_todo-<id>.txt` — harness commands that exercise the new code path in `parish-engine --headless --script`.

## 4. Implement

Smallest possible diff. When threading a new param through multiple layers (inference, IPC, UI), delegate the mechanical pass-through edits to a sonnet sub-agent — saves opus context.

Then decide whether this finding's _category_ warrants a permanent guard: if it has now been fixed more than once (e.g. auto-player movement, mid-conversation farewells, mood→emoji sign), add a `rubric_*` test in `parish/crates/parish-engine/tests/eval_baselines.rs` in the same PR so the regression cannot silently return. See `docs/agent/harness.md` → "Turning a recurring mistake into a sensor".

## 5. Run quality gates

From within the worktree:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p <changed-crate>
```

For UI changes also:

```sh
cd parish/apps/ui && npx vitest run && pnpm run check
```

## 6. Capture live transcript

```sh
cargo run -p parish-engine -- --headless --script \
  parish/testing/proofs/play_todo-<id>.txt > /tmp/transcript.txt
cp /tmp/transcript.txt .proofs/todo-<id>/transcript.json
```

For UI-only changes the engine harness is regression cover only — actual UI behaviour is verified in vitest. Note this in `evidence.md`.

## 7. Write evidence.md

First line must be `Evidence type: live gameplay transcript`. Include:

- Diff summary table (file, change).
- Acceptance-criteria → evidence map (one row per AC, citing `file:line` or test name).
- Commands run.
- Transcript excerpt.
- "Why this fixes #N" explainer.
- "Deferred items" section listing what was punted with a follow-up plan.

## 8. Write judge.md

Independent verdict. Must end with all three lines verbatim:

```text
Verdict: sufficient
Technical debt: clear
Acceptance criteria: met
```

Include risk-check (save compatibility, prompt budget, mode parity, architecture-fitness) and an acceptance-criteria audit table.

## 9. Commit + push + PR

- Conventional commit (`feat:` / `fix:` / `refactor:` / `docs:` / `test:` / `chore:`).
- Body explains the _why_, not the _what_. Do not add a Claude-specific co-author trailer unless the user explicitly asks for it.
- PR title prefix matches commit.
- PR body has Summary + Test plan checklist + `Proof bundle: .proofs/... posted via attach-proof`.

## 10. Attach proof bundle

From the worktree:

```sh
bash parish/scripts/attach-proof.sh todo-<id> <pr-num>
```

Do NOT call `just attach-proof` from a worktree — that uses the main repo's `justfile` and posts the wrong bundle.

## 11. Start next round immediately

Don't wait for CI. Branch off `origin/main` again with `git worktree add ... -b codex/round-<n+1>` and repeat 2-10.

Keep a running `Monitor` of all in-flight PRs:

```sh
prev=""
while true; do
  s=$(for pr in <ids>; do
    gh pr view "$pr" --json statusCheckRollup \
      --jq "[.statusCheckRollup[]? | {name: (\"$pr/\" + (.context // .name // \"unknown\")), bucket: (if (.state // .status // \"\") == \"PENDING\" then \"pending\" else ((.state // .conclusion // .status // \"unknown\") | ascii_downcase) end)}]" \
      2>/dev/null
  done | jq -s 'add')
  cur=$(jq -r '.[] | select(.bucket!="pending") | "\(.name): \(.bucket)"' \
    <<<"$s" | sort)
  comm -13 <(echo "$prev") <(echo "$cur")
  prev=$cur
  jq -e 'all(.bucket!="pending")' <<<"$s" >/dev/null 2>&1 && break
  sleep 60
done
echo "ALL TERMINAL"
```

## 12. Land green PRs

When monitor reports green:

```sh
gh pr merge <N> --squash --delete-branch
```

- The "main worktree" branch-delete error is harmless — merge succeeded on remote; verify via `gh pr view <N> --json state,mergeCommit`.
- Gemini `review / review: cancel` is normal (auto-cancelled bot review, not a real failure).

## Known CI failure patterns + fixes

- **Rust quality gate / coverage ratchet `cancel`.** Concurrency rule (`cancel-in-progress: true`) killed older runs when a new push happened. Push an empty commit to retrigger:

  ```sh
  git commit --allow-empty -m "ci: retrigger after auto-cancel" && git push
  ```

  Do NOT try `gh pr close && gh pr reopen` first — classifier may deny it.

- **Agent proof gate `fail` on first run after PR open.** Race: CI started before `attach-proof` posted the bundle comment. Retrigger via empty commit; second run finds the bundle and passes.

- **`gh workflow run` HTTP 500.** Workflow file isn't on the branch HEAD or validation issue. Use empty commit + push instead.

## Boundaries

- Never commit other sessions' leaked WIP from the main repo. If the Stop hook fires on someone else's broken match arms, tag the next message with `[skip-quality-hook]` and continue.
- Never force-push to a PR branch — bot review threads anchor to commit SHAs and force-push detaches them.
- Never amend; always create new commits. Pre-commit hook failures didn't actually commit, so `--amend` would corrupt prior history.
- Don't touch `.proofs/` archives in `docs/proofs/local-perf` or `docs/proofs/rundale-bench` — bench archives, exempt from the gate.
- After each landed PR, do not edit `TODO.md` to mark items done — leave it as the demo-audit record; the PR commit IS the marker.

## Stop conditions

Stop when:

- User says "stop" or "pause".
- `TODO.md` has no remaining unaddressed P0/P1 items.
- 3+ consecutive rounds hit unrelated infra failures (signal: real bug in workflow, not your code; escalate to user).

## Tooling Notes

The source command assumed shell, file editing, worktree switching, monitoring, task tracking, and skill invocation tools. In Codex, use the available equivalents; when a dedicated worktree or monitor tool is unavailable, use plain `git worktree` commands and concise status updates.
