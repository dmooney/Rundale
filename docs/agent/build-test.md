# Build & Test

## Cargo

- Build: `cargo build` (builds the default member, `parish-cli`)
- Build everything: `cargo build --workspace`
- Release build: `cargo build --release`
- Run: `cargo run -p parish` (or `cargo run`, since parish-cli is the default)
- Test all: `cargo test --workspace`
- Test one: `cargo test <test_name>`
- Format check: `cargo fmt --check` (apply: `cargo fmt`)
- Lint: `cargo clippy --workspace -- -D warnings`

## Game harness

Scripted gameplay fixtures live in `testing/fixtures/`. Run one with:

```sh
cargo run -p parish -- --script testing/fixtures/test_walkthrough.txt
# or
just game-test
just game-test-one test_movement_errors
just game-test-all
```

## Frontend

```sh
cd apps/ui && npx vitest run    # unit tests
cd apps/ui && npx playwright test    # e2e (auto-starts axum server)
just ui-test
just ui-e2e
just screenshots                 # regenerate docs/screenshots/*.png
```

To update Playwright baselines after intentional UI changes:

```sh
just ui-e2e-update
```

## Web server (browser testing)

```sh
cd apps/ui && npm run build && cd ../..
cargo run -p parish -- --web            # default port 3001
cargo run -p parish -- --web 8080
```

Then open `http://localhost:3001`.

## Tauri desktop

```sh
just tauri-dev      # cargo tauri dev
just tauri-build    # production bundle
```

System packages on Linux: `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`.

## Quality gates

- `/check` — fmt + clippy + tests + doc-consistency
- `/verify` — full pre-push checklist (gates + harness walkthrough)
- `/prove <feature>` — required after implementing any gameplay feature
- `/rubric` — snapshot baselines + structural rubrics (sister to `/prove`)
- `/feature-scaffold <name>` — depth-first decomposition before coding
- `/game-test [script]` — harness run

## Eval baselines

```sh
just baselines       # regenerate gameplay-output snapshots after intentional change
just harness-audit   # cross-reference fixtures, baselines, and roadmap for gaps
```

See [../design/testing.md](../design/testing.md) §Eval baselines for the schema.

## Coverage

`cargo tarpaulin` (target: keep above 90%).
