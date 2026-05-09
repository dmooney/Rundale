Evidence type: gameplay transcript

# Restore the two `test.fixme` Editor e2e tests

## Background

PR #935 marked two tests in `parish/apps/ui/e2e/features.spec.ts` as
`test.fixme`:

- `Editor › navigates to editor and shows tabs`
- `Editor › switches between editor tabs`

Both tests assume all 5 editor tabs render immediately on `/editor`, but
`parish/apps/ui/src/routes/editor/+page.svelte:64-78` only renders
NPCs/Locations/Validator when an `EditorModSnapshot` is loaded:

```svelte
{#each tabs as t}
    {#if snap || t.id === 'mods' || t.id === 'saves'}
        <button class="tab-btn" ...>
```

The Tauri mock fixture didn't stub `editor_list_mods` / `editor_open_mod`,
so `snap` stayed null and only Mods + Saves rendered. PR #935 documented
this as a scaffolding gap and deferred the fix.

## What this PR does

1. Add `EDITOR_MODS` (one minimal `ModSummary`) and `EDITOR_SNAPSHOT` (a
   minimal `EditorModSnapshot` with empty NPC / location / festival /
   encounter / anachronism arrays and a clean validation report) to
   `parish/apps/ui/e2e/mock-data.ts`. Types come from
   `src/lib/editor-types.ts` so any drift is a TypeScript error.
2. Wire the new constants into `installTauriMock` so the invoke mock
   returns them for `editor_list_mods` and `editor_open_mod`.
3. Rewrite both tests to click the rendered `.mod-card` first, which
   calls `editorOpenMod(path)` via `ModBrowser.openMod`
   (`src/components/editor/ModBrowser.svelte:9-22`). That sets
   `editorSnapshot`, makes `snap` truthy, and unblocks the conditional
   render.
4. Drop the `test.fixme` markers and the comment block that pointed at
   the gap.

The third Editor test (`back link returns to game page`) was already
passing under the SPA fallback fix in #935 and is unchanged here.

## Why click the card instead of pre-seeding the store

The mock is window-injected and doesn't reach Svelte stores directly.
The closest non-invasive fix is to drive the same code path a user would
take — click a mod card, let `ModBrowser.openMod` set the store. That
also exercises the IPC plumbing end-to-end rather than skipping past it.

## Verification

- Local Playwright run not reproducible on this machine (`Homebrew node
  25 is broken: dyld libllhttp.9.3.dylib missing`, classifier denied
  symlink fix). The two tests are validated in CI.
- TypeScript: imports `ModSummary` / `EditorModSnapshot` from
  `editor-types.ts`, so the mock shape is checked by `npm run check`
  (the `UI quality` job).
- Expected post-fix outcome: 46/50 passing, 1 skipped — only the
  pre-existing 3 smoke skips.

## Files changed

- `parish/apps/ui/e2e/mock-data.ts` — `EDITOR_MODS`, `EDITOR_SNAPSHOT`.
- `parish/apps/ui/e2e/fixtures.ts` — register the new mocks under
  `editor_list_mods` / `editor_open_mod` in the invoke response table.
- `parish/apps/ui/e2e/features.spec.ts` — drop `test.fixme`, click
  `.mod-card` to load the snapshot, refresh the comment block.
- `docs/proofs/editor-e2e-mocks/transcript.md`, `judge.md` — proof bundle.

## Pre-fix transcript (reference)

The two tests in their `test.fixme` form, post-#935:

```
4 skipped
46 passed
```

Marking both as live tests turns the two `skipped` entries into runs.
The `mod-card` click + new mock data unblocks them.
