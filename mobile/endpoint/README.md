# Rundale dialogue Endpoint definition

[`rundale-dialogue-v1.json`](rundale-dialogue-v1.json) is the version 1
Endpoint definition for the Phase 2 NPC dialogue role. It is the
`EndpointDefinition` body consumed by Limerick Endpoints: `inputSchema`,
`outputSchema`, `instructions`, `providerConfig`, and `inferenceConfig`.
[`example-engine-invocation.json`](example-engine-invocation.json) is a
secret-free serialization produced by the Rust `EndpointInvocation` DTO and
checked against it in the `limerick-core` fixture test.

[`rundale-dialogue-v2.json`](rundale-dialogue-v2.json) is the immutable Phase 5
contract. It retains v1 dialogue while separating authored facts, acquired NPC
knowledge, remembered player claims, and relevant task state. Its optional
output proposals are bounded to one exact-evidence player memory and one
authored task-offer ID; neither becomes authoritative until Rust validates and
commits the final candidate. Version 1 remains supported for older builds.

The public identity is organization `limerick-demo`, slug `rundale-dialogue`.
Version `1` was published and promoted on 2026-09-09 with content hash
`sha256:d2a58dc263543789c19a3bc5d3d934db7fee7e8fba81d5d01716bcf03315cae1`.
New Phase 5 builds pin immutable version `2`; record its deployed hash here
after publication and retain version 1 for older builds.
The first provider target is `google/gemini-3.5-flash-lite`, with 1,024 output
tokens, no retry, and the versioned streaming projection
`inferenceConfig.streaming.textField = "dialogue"`.

## Engine wire agreement

The input schema is the JSON serialization of
`limerick_core::mobile::EndpointInvocation` in
[`limerick/crates/limerick-core/src/mobile/mod.rs`](../../limerick/crates/limerick-core/src/mobile/mod.rs).
The Rust DTO uses `serde(rename_all = "camelCase")`, so the request uses
`sessionID`, `logicalRequestID`, `attemptID`, `playerInput`,
`currentLocation`, `knownPeople`, `knownPlaces`, `authoredFacts`, and
`acquiredKnowledge`, `rememberedPlayerClaims`, `relevantTaskState`, and
`recentConversation`. All fields are required and unknown fields are rejected.

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
values are part of both versioned schemas. The request's player and conversation text
is untrusted content; the Endpoint instructions do not duplicate the world's
facts and the engine remains authoritative for validation and state changes.

Version 1 output is exactly `{ "dialogue": "..." }`. Version 2 additionally
permits one bounded `proposedPlayerMemory` and one bounded
`authoredTaskOfferID`; `additionalProperties` remains false. The engine receives
the terminal structured candidate for its own NPC validation and gameplay
commit. Streaming may expose only the top-level `dialogue` text projection;
partial text is provisional and never changes game state.

## Publication and invocation notes

Publish this definition as an immutable Endpoint version and bind the Rundale
Firebase App Check app ID to its organization and slug in the deployed Limerick
Endpoints configuration. The JSON request body is exactly
`{ "input": <EndpointInvocation> }`. The mobile worker sends the engine's stable
request and attempt identities as bounded correlation headers. The server
verifies both Firebase credentials, tenant and Endpoint bindings, quotas, kill
switches, and the pinned version before provider dispatch. The client never
receives provider credentials, a shared consumer key, or creator instructions.

The `/stream` route retains the version 1 SSE transport contract frozen in
[`fixtures/dialogue-v1.sse`](fixtures/dialogue-v1.sse): ordered `progress` and
`text_delta` frames followed by exactly one validated `final` or `error`
terminal frame. Partial dialogue is provisional. The final schema-defined
output is validated by the Swift transport and again by the embedded engine
before any effect commits. See [the current integration handoff](phase2-handoff.md)
for deployment and live-proof status.

The definition intentionally contains no API keys, Firebase tokens, endpoint
URLs, organization identifiers, or deployment alias. Fill those values in
the authorized service/configuration path when the Endpoint is provisioned.

## Deterministic boundary checks

Run the shared transport and schema checks from the repository root:

```sh
swift test --package-path mobile/LimerickEndpointKit
cd limerick && cargo test -p limerick-core --features mobile --test mobile_endpoint_fixture
cd ../endpoints && pnpm exec vitest run apps/server/test/mobile-invocation.test.ts
```

These use fake provider responses and credentials. They establish agreement
between Swift, Rust, and TypeScript. The separate live evidence in
[phase2-handoff.md](phase2-handoff.md) establishes publication, simulator
Firebase/App Check, Google delivery, and Stop accounting; physical-iPhone App
Attest remains unverified.
