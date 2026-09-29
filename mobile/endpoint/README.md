# Rundale Endpoint definitions

The Endpoint definitions are game data. They live in the world's mod, one file
per inference role, and the mod manifest declares them (ADR-025 §5):

```toml
# mods/rundale/mod.toml
[endpoints]
dialogue = "endpoints/rundale-dialogue.v1.json"
intent = "endpoints/rundale-intent.v2.json"
```

Each file is named `<slug>.v<version>.json`; the name is the Endpoint's
identity, and the body is the `EndpointDefinition` consumed by Limerick
Endpoints: `inputSchema`, `outputSchema`, `instructions`, `providerConfig`, and
`inferenceConfig`. The engine loads them with the mod (`limerick_mod::endpoints`)
and attaches each role's reference and structured input to the calls it makes
(`limerick_core::endpoint_input`).

| Role     | File                                                                                | Output                                             |
| -------- | ----------------------------------------------------------------------------------- | -------------------------------------------------- |
| Dialogue | [`rundale-dialogue.v1.json`](../../mods/rundale/endpoints/rundale-dialogue.v1.json) | `{ "dialogue": "..." }`, streamed as text          |
| Intent   | [`rundale-intent.v2.json`](../../mods/rundale/endpoints/rundale-intent.v2.json)     | `{ "intent", "target", "dialogue", "atmosphere" }` |

Until the game is released, each definition stays at version 1: a change edits
the `.v1.json` file and replaces the published copy in place (`pnpm definitions
replace`). After release, published versions become immutable and a change
ships as a new `<slug>.v<version>.json`. The dialogue file's `inputSchema`
description still names `parish_core::mobile::EndpointInvocation`, the
pre-rename source of the shape; the engine now builds it with
`EndpointCall::invocation`.

`rundale-dialogue.v2.json` is exported from `limerick-prod`, where `ios-port`
published it: the richer contract with acquired knowledge, player memory, and
task offers, which the engine on `main` does not build. It is kept so the
database holds only copies of files; the manifest does not select it. Both
selected files target `google/gemini-3.5-flash-lite` with no retry. Dialogue
allows 1,024 output tokens and streams the `dialogue` field; intent allows 256,
as the desktop Intent profile does.

The intent definition's `instructions` are the engine's intent prompt
(`limerick_input::intent_system_prompt()`) verbatim. A test fails if they drift;
regenerate the file after changing the prompt.

## Engine wire agreement

A call with an Endpoint carries an `EndpointCall`: the reference (role, slug,
version) and the role's structured input. The host builds the request's
`input` with `EndpointCall::invocation`, which adds the invocation envelope it
owns to the engine's fields. The JSON request body is exactly
`{ "input": <invocation> }`.

The envelope is the same for both roles: `contractVersion` (`{major: 1,
minor: 0}`), `sessionID`, `logicalRequestID`, `attemptID`, `baseRevision`
(`{"rawValue": number}`), and `idempotencyKey` (`<request>:<attempt>`). Opaque
IDs serialize as strings. All fields are required and unknown fields are
rejected.

The intent input adds `role: "player_intent"` and the bounded `playerInput`
the deterministic local parser did not recognise.

The dialogue input adds `role: "npc_dialogue"`, `playerInput`, `speaker`,
`currentLocation`, `knownPeople`, `knownPlaces`, `authoredFacts`,
`recentConversation`, `maxOutputChars` (8,192), and `maxStreamBytes` (16,384).
People are `npc-<id>` with their occupation as `role`; places are
`place-<id>` with the description rendered for the current time and weather;
authored facts are the speaker's `knowledge` from `npcs.json`. The engine caps
people, places, and facts at 32 and sends the last 8 exchanges at the current
location, oldest first. `recentConversation` contains
`limerick_types::ConversationExchange` values, which keep that type's
snake_case field names: `timestamp`, `speaker_id`, `speaker_name`,
`player_input`, `npc_dialogue`, and `location`.

[`example-engine-invocation.json`](example-engine-invocation.json) and
[`example-intent-invocation.json`](example-intent-invocation.json) are secret-free
invocations the engine builds on the canonical world. The Rust test
`endpoint_calls` checks them against the engine's output
(`UPDATE_ENDPOINT_FIXTURES=1` regenerates them), and the Endpoints suite
validates them against the definitions' schemas.

The request's player and conversation text is untrusted content. The Endpoint
instructions do not duplicate the world's facts, and the engine remains
authoritative for validation and state changes. The engine receives the
terminal structured candidate for its own validation and gameplay commit.
Streaming may expose only the top-level `dialogue` text projection; partial
text is provisional and never changes game state.

Desktop in-process inference ignores the Endpoint reference and sends the
call's rendered prompt, so desktop provider requests are unchanged. The mobile
host that sends these invocations is #2044.

## Publication and invocation notes

The files are published, and the deployment checked against them, with the
Endpoints definitions command
([ADR 013](../../endpoints/docs/adr/013-publish-definitions-from-files.md)):

```sh
cd endpoints
DATABASE_URL=... PROVIDER_MODE=live GOOGLE_ALLOWED_MODELS=gemini-3.5-flash-lite \
  pnpm definitions verify <organization-slug> ../mods/rundale/endpoints
```

`verify` passes only when every file's content hash equals its published copy
and every published version of these slugs has a file; `publish` adds the
missing versions and writes nothing on any disagreement; `replace` also
overwrites changed versions in place (pre-release only); `export` writes a
published version that has no file into this directory.

Publish each definition as an Endpoint version and bind the Rundale
Firebase App Check app ID to its organization and slug in the deployed Limerick
Endpoints configuration. The mobile worker sends the engine's stable
request and attempt identities as bounded correlation headers. The server
verifies both Firebase credentials, tenant and Endpoint bindings, quotas, kill
switches, and the pinned version before provider dispatch. The client never
receives provider credentials, a shared consumer key, or creator instructions.

The `/stream` route returns the version 1 SSE contract frozen in
[`fixtures/dialogue-v1.sse`](fixtures/dialogue-v1.sse): ordered `progress` and
`text_delta` frames followed by exactly one validated `final` or `error`
terminal frame. Partial dialogue is provisional. The final output remains the
exact `{ "dialogue": "..." }` object and is validated again by the embedded
engine before commit. See [the current integration handoff](phase2-handoff.md)
for deployment and live-proof status.

The definitions intentionally contain no API keys, Firebase tokens, endpoint
URLs, organization identifiers, or deployment alias. Fill those values in
the authorized service/configuration path when the Endpoint is provisioned.

## Deterministic boundary checks

Run the shared transport and schema checks from the repository root:

```sh
swift test --package-path mobile/LimerickEndpointKit
cd limerick && cargo test -p limerick-core --test endpoint_calls
cd ../endpoints && pnpm exec vitest run apps/server/test/mobile-invocation.test.ts
```

These use fake provider responses and credentials. They establish agreement
between Swift, Rust, and TypeScript. The separate live evidence in
[phase2-handoff.md](phase2-handoff.md) establishes publication, simulator
Firebase/App Check, Google delivery, and Stop accounting; physical-iPhone App
Attest remains unverified.
