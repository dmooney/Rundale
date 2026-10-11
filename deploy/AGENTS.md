# deploy — agent scope

`Dockerfile` builds the web-server container: a Node stage builds the Svelte UI,
a Rust stage builds `limerick-server` with that UI embedded, and a runtime stage
serves both. See root [`AGENTS.md`](../AGENTS.md) and
[`limerick-server`](../limerick/crates/limerick-server/AGENTS.md).

## Local gotchas

- **The production server build needs the frontend output before Cargo runs.** `limerick-server` embeds CSP hashes from `limerick/apps/ui/dist` in its build script; copying the UI only into the runtime image can produce a health-green container whose browser bootstrap is blocked by CSP. Keep the frontend-to-builder copy and its deploy sensor together.
- **The Docker frontend stage must preserve the repository-relative UI path.**
  Notebook-art provenance derives labels from `limerick/apps/ui`, so build under
  `/build/limerick/apps/ui` and keep every cross-stage `dist` path aligned. Its
  Rust builder must also copy `limerick/config/` for compile-time `include_str!`.
