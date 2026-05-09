Evidence type: gameplay transcript

Verdict: sufficient

Technical debt: clear

Pure test infrastructure change. No runtime/product code touched.

The mock data references real types (`ModSummary`, `EditorModSnapshot`) from
`parish/apps/ui/src/lib/editor-types.ts`, so any future schema drift surfaces
as a TypeScript error in `npm run check` rather than silent test rot. The
test bodies drive the same `ModBrowser.openMod` code path a real user takes,
so the assertions still cover the IPC plumbing end-to-end.

Removes 11-line `test.fixme` block plus its `// NPCs / Locations / Validator
…` comment in `features.spec.ts`. Adds 38 lines of mock data + 12 lines of
fixture wiring. Net: more coverage, less commented-out scaffolding.

No `#[allow]`, no placeholder/TODO markers, no feature flags or compatibility
shims introduced. Stacked on top of `claude/fix-playwright-ci-timeout-Jsxkh`
(PR #935) since the SPA fallback in that PR is a hard dependency: without
it `/editor` 404s before any of these mocks come into play.
