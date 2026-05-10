Evidence type: gameplay transcript

Verdict: sufficient

Technical debt: clear

The change has two parts:

1. **Glyph CDN route mock** — surgical fixture extension that follows the
   existing `**/tiles/**` mock pattern. Removes a flaky external dependency
   (`demotiles.maplibre.org`, no SLA per the upstream docstring) from the
   e2e suite without changing app code. Validated by the 43 already-passing
   tests still passing in CI run 25604247150 after the first push of this
   PR — those tests were the canary for the same `networkidle` hang.

2. **SPA fallback in parish-server** — fixes a real product bug. SvelteKit's
   adapter-static with `fallback: 'index.html'` + `strict: false`
   (`parish/apps/ui/svelte.config.js`) requires the web server to serve the
   SPA shell for any path it doesn't have a static file for. The previous
   `ServeDir::new(...)` mount returned 404 for `/editor` (and would for any
   future client-only route). The fix uses `tower_http::services::ServeFile`
   as the not-found service — idiomatic axum/tower-http SPA wiring, no new
   dependency. `cargo clippy -p parish-server --tests -- -D warnings` is
   clean and `cargo test -p parish-server --lib` still passes 180/180.

The two `test.fixme` markers on `navigates to editor and shows tabs` and
`switches between editor tabs` are not new debt — they document an existing
scaffolding gap (missing `editor_list_mods` / `editor_open_mod` Tauri mocks)
that pre-dates this PR and was already noted in the `techdebt-ui-e2e`
proof bundle. The fixme comments in `features.spec.ts` cite the exact
condition in `parish/apps/ui/src/routes/editor/+page.svelte` that gates
rendering, so the follow-up scope is unambiguous.

No `#[allow]` directives, no placeholder/TODO markers, no behavior flags
or feature gates introduced. Server change is debug + release; no cfg
guards added.
