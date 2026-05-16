# Evidence — local-inference onboarding UI stays in sync with backend

Evidence type: gameplay transcript

## The bug

After the vllm-mlx onboarding wizard shipped (#608875e4), the
first-run progress UI no longer rendered when the user clicked
"Set up local inference" on the macOS LocalInferenceFork. Two
distinct failure modes were present in `main`:

1. **UI-click path.** `pickLocal()` awaited the long-running
   `start_local_inference_setup` Tauri command before calling
   `onComplete()`. The fork therefore stayed mounted with the
   placeholder "Starting local inference setup…" for the whole
   multi-minute HuggingFace download. SetupOverlay's
   `setup-status` / `setup-progress` event listeners updated their
   own state, but the template branched on `needsOnboarding` →
   `LocalInferenceFork`, so the progress bar never reached the
   screen.

2. **MCP-driven path.** A remote MCP client can POST
   `/api/start-local-inference` and drive the same Rust bootstrap
   (`do_start_local_inference_setup` in parish-tauri/src/commands.rs).
   That path emits the same setup-status / setup-progress events,
   but the AppState `needs_onboarding` flag is only cleared at the
   very end of the bootstrap (after config write). So even though
   Rust was actively downloading 9 GB of weights, the SetupOverlay
   sat on the fork the whole time. Same for the case where the
   desktop window reloads mid-download — `get_setup_snapshot`
   still reports `needs_onboarding: true` with `completed > 0`.

## The fix

Three small Svelte changes converge on a single rule: *any backend
setup activity = dismiss the fork.*

* `LocalInferenceFork.svelte`: call `onComplete()` synchronously
  *before* awaiting the IPC so the UI flips to the progress overlay
  immediately. Dead local-confirming state removed.
* `SetupOverlay.svelte::onSetupStatus` / `onSetupProgress`: set
  `needsOnboarding = false` on first event. Covers MCP-triggered
  setup that bypasses the button click.
* `SetupOverlay.svelte::applySetupSnapshot`: only render the fork
  when the snapshot has no in-flight activity
  (`completed === 0 && total === 0 && messages are the default`).
  Covers UI reloads mid-download.

## What was verified

### Playwright e2e (regression-tested both ways)

`parish/apps/ui/e2e/local-inference-fork.spec.ts` now has four
tests:

1. **Fork renders** on a 48 GB Mac (existing, regenerates the
   docs/screenshots baseline).
2. **UI-click path** — click "Set up local inference" → fork
   detaches, `[role=progressbar][aria-label=Setup progress]`
   becomes visible, `aria-valuenow` advances to 50 when
   `setup-progress {completed:1M,total:2M}` is emitted, status
   message appears in activity panel.
3. **MCP-driven path** — fork rendered, then `setup-status` event
   emitted without any click → fork auto-dismisses, progress UI
   renders, progress drives to 50%.
4. **Mid-flight reload** — `get_setup_snapshot` returns
   `needs_onboarding: true` AND `completed: 2.5 GB / total: 9.2 GB`
   AND non-default messages. SetupOverlay mounts straight to
   progress UI, never renders the fork.

All four pass with the fix; both new tests verified to fail when
the SetupOverlay changes are stashed:

```
$ npx playwright test e2e/local-inference-fork.spec.ts --workers=1
PASS (4) FAIL (0)
Time: 7630ms
```

```
$ git stash push -- parish/apps/ui/src/components/SetupOverlay.svelte
$ npm run build && npx playwright test e2e/local-inference-fork.spec.ts --grep "MCP-driven|Mid-flight" --workers=1
PASS (0) FAIL (2)
  - MCP-driven setup auto-dismisses the fork on first setup-status event
  - Mid-flight snapshot on UI reload skips the fork and resumes progress UI
```

### Live desktop session via parish-mcp

Started `parish-tauri --mcp-port 3030` with a vite dev server on
:5173, then drove onboarding from MCP:

```
$ curl -sS -X POST -H 'Content-Type: application/json' \
       -d '{"variant":"two-slot"}' \
       http://127.0.0.1:3030/api/start-local-inference

$ mcp__parish__tauri_invoke get_setup_snapshot
{
  "completed": 3683115274,
  "current_message": "Downloading model-00001-of-00002.safetensors",
  "messages": [
    "Preparing the storyteller...",
    "Preparing model download…",
    "Downloading added_tokens.json",
    "Downloading config.json",
    "Downloading merges.txt",
    "Downloading model-00001-of-00002.safetensors"
  ],
  "needs_onboarding": true,
  "total": 9201261038,
  ...
}
```

User confirmation in the live desktop window: fork dismissed,
progress UI showing the download bar + activity log scrolling.
Setup ran to completion (HF cache from a prior aborted attempt
let it finish quickly); both vllm-mlx slots
(`mlx-community/Qwen2.5-14B-Instruct-4bit` Dialogue,
`mlx-community/Qwen2.5-1.5B-Instruct-4bit` small) spawned
successfully:

```
2026-05-15T22:41:07Z parish_inference::setup: vllm-mlx ready after ~7500ms
2026-05-15T22:41:09Z parish_inference::setup: vllm-mlx ready after ~2500ms
```

## Files changed

* parish/apps/ui/src/components/LocalInferenceFork.svelte
* parish/apps/ui/src/components/SetupOverlay.svelte
* parish/apps/ui/e2e/local-inference-fork.spec.ts (new tests)
* docs/screenshots/onboarding-local-inference.png (baseline regen)
