## Summary

## Proof Evidence

- [ ] Proof bundle prepared in `.proofs/{task-id}/` (gitignored — not committed).
- [ ] Evidence is a gameplay transcript, screenshot, or gif; `Evidence type: live ...` header set for runtime changes.
- [ ] `.proofs/{task-id}/judge.md` records an independent sufficiency verdict and `Acceptance criteria: met`.
- [ ] `just agent-check` passes locally.
- [ ] Posted to this PR with `just attach-proof {task-id}` — look for the `parish-proof-bundle:{task-id}` comment below.

## Checks

- [ ] `just check`
- [ ] `just verify` when gameplay, runtime, or UI behavior changed
