---
name: todo-drain
description: "Drain TODO.md demo-audit findings in parallel rounds \u2014 AC-first, live proof, evidence page, retrigger CI as needed, land green PRs while next round is in flight."
---

# todo-drain

Land fixes from `TODO.md` (Rundale demo-audit findings). Run rounds in parallel — start the next round while the previous PR's CI runs.

## Workflow per round

## 1. Sync + worktree

Never work in the main repo directory — other sessions may be using it. Switch into (or create) a worktree off latest `origin/main`:

```sh
git fetch origin main
git worktree add .worktrees/round-<n> -b round-<n> origin/main
```

Then move execution to that path so all subsequent commands run inside the worktree.

## 2. Pick TODO item

Open `TODO.md`. Prefer the smallest-scope unaddressed P0/P1. If the entry has a "revise" note pointing at a different ID, follow it.

## 3. Write acceptance criteria FIRST

Before any code change:

- Observable acceptance criteria, sized concretely (e.g. "`frequency_penalty: Option<f32>` field on `InferenceRequest`", not "improve repetition handling"), drafted as the opening of the PR body. List anything intentionally punted from this round under "Deferred items".
- `limerick/testing/proofs/play_todo-<id>.txt` — harness commands that exercise the new code path in `limerick-engine --headless --script`.

## 4. Implement

Smallest possible diff. When threading a new param through multiple layers (inference, IPC, UI), delegate the mechanical pass-through edits to a sonnet sub-agent — saves opus context.

Then decide whether this finding's _category_ warrants a permanent guard: if it has now been fixed more than once (e.g. auto-player movement, mid-conversation farewells, mood→emoji sign), add a `rubric_*` test in `limerick/crates/limerick-engine/tests/eval_baselines.rs` in the same PR so the regression cannot silently return. See `docs/agent/harness.md` → "Turning a recurring mistake into a sensor".

## 5. Run quality gates

From within the worktree:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p <changed-crate>
```

For UI changes also:

```sh
cd limerick/apps/ui && npx vitest run && pnpm run check
```

## 6. Capture live evidence

Run the change in a real process on the production path, record it, and keep the recording and transcript outside the repository (a scratch directory such as `~/rundale-evidence/todo-<id>/`):

```sh
cargo run -p limerick-engine -- --headless --script \
  limerick/testing/proofs/play_todo-<id>.txt > ~/rundale-evidence/todo-<id>/transcript.txt
```

For UI-only changes the engine harness is regression cover only — actual UI behaviour is verified in vitest and in the recording. Say so in the PR body.

## 7. Write the PR body

Ordinary prose, no machine-checked format. Include:

- The acceptance criteria, each with what in the recording, transcript, or test output shows it.
- Diff summary.
- Verification actually run, with skips and unavailable gates stated as such.
- Transcript excerpt.
- "Why this fixes #N" explainer.
- "Deferred items" with a follow-up plan.
- Risk check: save compatibility, prompt budget, mode parity, architecture fitness.

## 8. Commit + push + PR

- Conventional commit (`feat:` / `fix:` / `refactor:` / `docs:` / `test:` / `chore:`).
- Body explains the _why_, not the _what_. Do not add a Claude-specific co-author trailer unless the user explicitly asks for it.
- PR title prefix matches commit.
- PR body from step 7, linking the evidence page (step 9).

## 9. Publish the evidence page

Runtime-shipping changes must link `https://dmooney.github.io/rundale-pages/pr/<pr-num>/` in the PR body; CI's agent-check fails without it. Put an `index.html`, the H.264 `.mp4`, and a short `.gif` in one directory, then from the worktree:

```sh
bash limerick/scripts/publish-pr-page.sh <pr-num> ~/rundale-evidence/todo-<id>
```

See [agent-check](../../../docs/agent/agent-check.md).

## 10. Start next round immediately

Don't wait for CI. Branch off `origin/main` again with `git worktree add ... -b round-<n+1>` and repeat 2-9.

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

## 11. Hand green PRs to the gatekeeper

When monitor reports green, spawn a fresh gatekeeper subagent for each PR, as root `AGENTS.md` [Gatekeeper review](../../../AGENTS.md#gatekeeper-review) describes; it merges, sends the PR back, or hands it to the owner. Never merge it yourself.

- After the gatekeeper merges, verify via `gh pr view <N> --json state,mergeCommit`.
- Gemini `review / review: cancel` is normal (auto-cancelled bot review, not a real failure).

## Known CI failure patterns + fixes

- **Rust quality gate / coverage ratchet `cancel`.** Concurrency rule (`cancel-in-progress: true`) killed older runs when a new push happened. Push an empty commit to retrigger:

  ```sh
  git commit --allow-empty -m "ci: retrigger after auto-cancel" && git push
  ```

  Do NOT try `gh pr close && gh pr reopen` first — classifier may deny it.

- **Evidence gate `fail` after adding the evidence link.** The job reads the body when it runs and does not re-run on a body edit. Re-run the job or push an empty commit.

- **`gh workflow run` HTTP 500.** Workflow file isn't on the branch HEAD or validation issue. Use empty commit + push instead.

## Boundaries

- Never commit other sessions' leaked WIP from the main repo.
- Never force-push to a PR branch — bot review threads anchor to commit SHAs and force-push detaches them.
- Never amend; always create new commits. Pre-commit hook failures didn't actually commit, so `--amend` would corrupt prior history.
- Don't touch the bench archives in `docs/proofs/local-perf` or `docs/proofs/rundale-bench`.
- After each landed PR, do not edit `TODO.md` to mark items done — leave it as the demo-audit record; the PR commit IS the marker.

## Stop conditions

Stop when:

- User says "stop" or "pause".
- `TODO.md` has no remaining unaddressed P0/P1 items.
- 3+ consecutive rounds hit unrelated infra failures (signal: real bug in workflow, not your code; escalate to user).

## Tooling Notes

If your harness lacks a dedicated worktree or monitor tool, use plain `git worktree` commands and concise status updates.
