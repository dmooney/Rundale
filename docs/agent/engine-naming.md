# Engine naming conventions

Use **Rundale** for the game, its shipped world and player-facing product
language. Use **Limerick** for the Rust engine, its crates and binaries, the
Limerick Designer editor, Limerick Endpoints, and the `limerick_*` MCP tools.
The HTTP client package is `limerick-client` and its binary is `limerick`; the
headless package and binary are `limerick-engine`; the MCP package is
`limerick-mcp`, registered under the server key `limerick`.

When checking a rename, run:

```sh
python3 limerick/scripts/check-engine-naming.py
```

The guard is strict. Exact reviewed exceptions are limited to geographic or
historical parish vocabulary, SHA-bearing immutable provenance records, and
scanner tests that intentionally exercise legacy text. Generated artifacts do
not receive a blanket exemption; add a narrowly justified exact exception when
an immutable record must retain its original bytes. Any approved generated
inventory exception must name the exact file and SHA; refresh it explicitly
after review.

The Endpoints integration remains deferred on the unmerged branch. The current
acceptance revision still requires the unconditional CI guard, so any incoming
implementation must use the renamed identities before it is merged.

## Evidence and later integrations

Original benchmark inputs, scored results, provider responses, captured art
prompts, approved releases, and historical proofs retain their original bytes,
identities, hashes, and source locators. Relocation changes their containers and
current consumers. An old source locator inside an approved receipt is historical
metadata; it must not be rewritten to resemble a current filesystem path.

A changed benchmark input requires a new benchmark version and merkle; scores
against its predecessor cannot satisfy promotion for the new input. A changed
approved image requires a new artifact and receipt. No paid benchmark rerun is
required for this rename. The deterministic walkthrough help-output baseline has
a new `test_walkthrough.limerick-v1.json` revision; its original snapshot remains
intact. This is a regression snapshot, not a new scored benchmark input.

The contributor landing the deferred Endpoints branch must rename incoming
engine identities and run affected integration checks before merging. Completion
applies to the verified revision, not to other branches or future merges. Global
installations, Git history, external archives, and unsupported old development
saves remain outside this repository change.

Build verification also scans explicit generated roots:

```sh
python3 limerick/scripts/check-engine-naming.py --generated-root limerick/apps/ui/dist --generated-exceptions limerick/scripts/engine-generated-exceptions.json
python3 limerick/scripts/check-engine-naming.py --generated-root limerick/crates/limerick-tauri/gen
```

The generated inventory is separate so a clean source-only checkout can pass.
After a UI build its exact entries and hashes must all match; stale entries fail.
Dependencies and compiler caches are not output roots. Inspect rendered branding
separately because binary image pixels cannot be validated by a text scan.
