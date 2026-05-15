# Judge verdict — onboarding progress UI sync

Verdict: sufficient

Technical debt: clear

The PR closes a real first-run UX hole introduced by the
vllm-mlx onboarding work (#608875e4). Both failure modes are
covered by automated regression tests that have been validated
to fail without the fix, and the MCP-driven path was driven
end-to-end against a live desktop window.

## What was claimed and verified

1. **UI-click path now flips the overlay synchronously.**
   `LocalInferenceFork.pickLocal()` calls `onComplete()` before
   awaiting the multi-minute IPC, so the SetupOverlay drops the
   fork and renders the progress UI on click. Playwright test
   "Picking local inference flips overlay to progress UI and
   renders live progress" pins the contract: mock the IPC as a
   never-resolving Promise, verify the fork detaches, the
   progress bar becomes visible, status messages land in the
   activity panel, and the bar reaches 50% on
   `setup-progress {1M, 2M}`. Verified to fail with the
   `LocalInferenceFork.svelte` change stashed.

2. **MCP-driven path auto-dismisses the fork.**
   `SetupOverlay.svelte::onSetupStatus` and `onSetupProgress`
   set `needsOnboarding = false` on the first live event. Tested
   by emitting `setup-status` directly (no click) and asserting
   the fork detaches + progress UI mounts. Verified to fail with
   the SetupOverlay change stashed.

3. **Mid-flight reload no longer rebrands progress as
   onboarding.** `applySetupSnapshot` now ignores
   `needs_onboarding: true` when the snapshot also reports
   `completed > 0` or non-default messages. Tested by serving a
   "mid-download" snapshot on mount and asserting the progress UI
   renders without the fork ever appearing. Verified to fail
   without the fix.

4. **Live MCP run.** Drove `POST /api/start-local-inference` via
   parish-mcp against a running `parish-tauri --mcp-port 3030`
   with a vite dev server. User confirmed the desktop window
   transitioned from fork → progress bar + activity log on the
   first event. Setup ran to completion (HF cache short-circuited
   the download); both `mlx-community/Qwen2.5-14B-Instruct-4bit`
   and `mlx-community/Qwen2.5-1.5B-Instruct-4bit` slots spawned
   successfully per parish-tauri logs.

## Technical debt left behind

None. The Svelte changes use the same `setup-status` /
`setup-progress` / `setup-done` event surface that was already
plumbed through TauriProgress. No new state, no new IPC, no
type drift. Dead `local-confirming` mode and `localError` state
were removed rather than left behind. The added playwright
tests share the existing `installTauriMock` / `emitEvent` /
`updateMockResponse` fixtures.
