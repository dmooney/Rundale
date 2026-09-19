# Rundale Endpoint definitions

## Dialogue — `rundale-dialogue-v1.json`

[`rundale-dialogue-v1.json`](rundale-dialogue-v1.json) is the version 1
Endpoint definition for the Phase 2 NPC dialogue role. It is the
`EndpointDefinition` body consumed by Parish Endpoints: `inputSchema`,
`outputSchema`, `instructions`, `providerConfig`, and `inferenceConfig`.
[`example-engine-invocation.json`](example-engine-invocation.json) is a
secret-free serialization produced by the Rust `EndpointInvocation` DTO after
Intent has selected a Talk action, and is checked against it in the
`limerick-core` fixture test.

The public identity is organization `parish-demo`, slug `rundale-dialogue`,
version `1`. The exact artifact was published and promoted on 2026-09-09; its
deployed content hash is
`sha256:d2a58dc263543789c19a3bc5d3d934db7fee7e8fba81d5d01716bcf03315cae1`.
The first provider target is `google/gemini-3.5-flash-lite`, with 1,024 output
tokens, no retry, and the versioned streaming projection
`inferenceConfig.streaming.textField = "dialogue"`.

## Intent — `rundale-intent-v1.json`

[`rundale-intent-v1.json`](rundale-intent-v1.json) is the version 1 Endpoint
definition for shared player-intent inference (`role: "intent"`). It reuses the
desktop `INTENT_SYSTEM_PROMPT` semantics and returns
`{intent, target?, dialogue?, atmosphere?}`. Mobile calls this Endpoint when
`parse_intent_local` cannot classify the input; dialogue generation follows only
when the validated intent is Talk/Unknown. The intended public identity is
organization `parish-demo`, slug `rundale-intent`, version `1`. Publish and bind
it through the same Parish Endpoints path as dialogue; the iOS app selects the
slug from `RUNDALE_ENDPOINT_INTENT_SLUG` (default `rundale-intent`).

## Engine wire agreement

The input schema is the JSON serialization of
`limerick_core::mobile::EndpointInvocation` in
[`limerick/crates/limerick-core/src/mobile/mod.rs`](../../limerick/crates/limerick-core/src/mobile/mod.rs).
The Rust DTO uses `serde(rename_all = "camelCase")`, so the request uses
`sessionID`, `logicalRequestID`, `attemptID`, `playerInput`,
`currentLocation`, `knownPeople`, `knownPlaces`, `authoredFacts`, and
`recentConversation`. Dialogue invocations include a required `speaker`. Intent invocations omit `speaker`. Unknown fields are rejected by each role schema.

Opaque mobile IDs serialize as strings. `baseRevision` is the explicit
`{"rawValue": number}` `StateRevision` shape. Engine `u32` identifiers in
grounded people, grounded conversation speakers, and locations remain JSON
integers. `recentConversation` contains `limerick_types::ConversationExchange` values,
which retain that type's default snake_case serde field names. It therefore
uses the exact shape of
[`ConversationExchange`](../../limerick/crates/limerick-types/src/conversation.rs):
`timestamp`, `speaker_id`, `speaker_name`, `player_input`, `npc_dialogue`, and
`location`.

The engine supplies the authoritative context and bounds it before dispatch:
up to 32 people, 32 places, 32 authored facts, and 8 recent exchanges. It
sets `maxOutputChars` to 8,192 and `maxStreamBytes` to 16,384. Those fixed
values are part of this v1 schema. The request's player and conversation text
is untrusted content; the Endpoint instructions do not duplicate the world's
facts and the engine remains authoritative for validation and state changes.

The output contract depends on role:

- **Dialogue** (`npc_dialogue`): exactly `{ "dialogue": "..." }`. `additionalProperties`
  is false, and the dialogue is bounded at 8,192 characters. Streaming may expose
  only the top-level `dialogue` text projection; partial text is provisional and
  never changes game state.
- **Intent** (`intent`): exactly
  `{ "intent": "...", "target"?: "...", "dialogue"?: "...", "atmosphere"?: "..." }`
  per [`rundale-intent-v1.json`](rundale-intent-v1.json). The engine validates the
  candidate with shared `limerick-input` rules before dispatching travel, look,
  narration, or a follow-up Dialogue invocation. Malformed Intent payloads that
  omit structured intent are rejected; validated `Unknown` follows the same
  dialogue-continue semantics as desktop.

The engine receives the terminal structured candidate for its own validation and
gameplay commit.

## Publication and invocation notes

Publish this definition as an immutable Endpoint version and bind the Rundale
Firebase App Check app ID to its organization and slug in the deployed Parish
Endpoints configuration. The JSON request body is exactly
`{ "input": <EndpointInvocation> }`. The mobile worker sends the engine's stable
request and attempt identities as bounded correlation headers. The server
verifies both Firebase credentials, tenant and Endpoint bindings, quotas, kill
switches, and the pinned version before provider dispatch. The client never
receives provider credentials, a shared consumer key, or creator instructions.

The `/stream` route returns the version 1 SSE contract frozen in
[`fixtures/dialogue-v1.sse`](fixtures/dialogue-v1.sse): ordered `progress` and
`text_delta` frames followed by exactly one validated `final` or `error`
terminal frame. Partial dialogue is provisional. Dialogue finals remain the
exact `{ "dialogue": "..." }` object; Intent finals use the structured Intent
object above. Both are validated again by the embedded engine before commit.
See [the current integration handoff](phase2-handoff.md) for deployment and
live-proof status.

The definition intentionally contains no API keys, Firebase tokens, endpoint
URLs, organization identifiers, or deployment alias. Fill those values in
the authorized service/configuration path when the Endpoint is provisioned.

## Deterministic boundary checks

Run the shared transport and schema checks from the repository root:

```sh
swift test --package-path mobile/ParishEndpointKit
cd limerick && cargo test -p limerick-core --features mobile --test mobile_endpoint_fixture
cd ../endpoints && pnpm exec vitest run apps/server/test/mobile-invocation.test.ts
```

These use fake provider responses and credentials. They establish agreement
between Swift, Rust, and TypeScript. The separate live evidence in
[phase2-handoff.md](phase2-handoff.md) establishes publication, simulator
Firebase/App Check, Google delivery, and Stop accounting; physical-iPhone App
Attest remains unverified.

Save compatibility: `pending_endpoint_role` on request/attempt records is
optional (`serde(default)`). Saves written before the Intent path still load;
interrupted work without a stored role defaults to dialogue only when a
dialogue attempt was already open.
