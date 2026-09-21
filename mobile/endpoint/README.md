# Rundale Endpoint definitions

Two roles are published. The engine selects the role; the app only transports
it.

| Role | Definition | Output |
| ---- | ---------- | ------ |
| `intent` | [`rundale-intent-v1.json`](rundale-intent-v1.json) | The structured interpretation of free-form player input |
| `npc_dialogue` | [`rundale-dialogue-v1.json`](rundale-dialogue-v1.json) | One grounded NPC utterance |

## Intent role

The Intent Endpoint restores the engine's existing local-parser →
inferred-intent → validated-action flow on mobile (#1993). Its `instructions`
are `limerick_input::INTENT_SYSTEM_PROMPT` verbatim, and
`mobile_intent_contract::intent_endpoint_contract_matches_the_engine_prompt`
fails if the published contract drifts from the Rust source, so the prompt
stays owned by the engine rather than by the deployment.

The input shape is the dialogue invocation's, minus a resolved `speaker`
(interpretation has not selected one yet) and with an always-empty
`authoredFacts` (interpretation classifies an action, it does not answer the
player). `knownPeople` and `knownPlaces` carry authoritative current-world
state so a destination or addressee resolves against what actually exists.
[`example-intent-invocation.json`](example-intent-invocation.json) is the
secret-free serialization produced by the Rust DTO and checked against it.

The output is `{ "intent", "target"?, "dialogue"?, "atmosphere"? }`. The engine
applies the shared `limerick-input` validation to it, selects and executes the
action, and rejects a malformed or unsupported result explicitly. The Endpoint
never performs an action, resolves a target, or changes state. This role is not
streamed: the payload is a small structured object delivered in one `final`
frame.

Deploy it as its own immutable Endpoint version; the app reads its slug and
version from `RUNDALE_INTENT_ENDPOINT_SLUG` (default `rundale-intent`) and
`RUNDALE_INTENT_ENDPOINT_VERSION` (default `1`).

## Dialogue role

[`rundale-dialogue-v1.json`](rundale-dialogue-v1.json) is the version 1
Endpoint definition for the Phase 2 NPC dialogue role. It is the
`EndpointDefinition` body consumed by Parish Endpoints: `inputSchema`,
`outputSchema`, `instructions`, `providerConfig`, and `inferenceConfig`.
[`example-engine-invocation.json`](example-engine-invocation.json) is a
secret-free serialization produced by the Rust `EndpointInvocation` DTO and
checked against it in the `limerick-core` fixture test.

The public identity is organization `parish-demo`, slug `rundale-dialogue`,
version `1`. The exact artifact was published and promoted on 2026-09-09; its
deployed content hash is
`sha256:d2a58dc263543789c19a3bc5d3d934db7fee7e8fba81d5d01716bcf03315cae1`.
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
values are part of this v1 schema. The request's player and conversation text
is untrusted content; the Endpoint instructions do not duplicate the world's
facts and the engine remains authoritative for validation and state changes.

The output contract is exactly `{ "dialogue": "..." }`. `additionalProperties`
is false, and the dialogue is bounded at 8,192 characters. The engine receives
the terminal structured candidate for its own NPC validation and gameplay
commit. Streaming may expose only the top-level `dialogue` text projection;
partial text is provisional and never changes game state.

## Publication and invocation notes

Publish this definition as an immutable Endpoint version and bind the Rundale
Firebase App Check app ID to its organization and slug in the deployed Parish
Endpoints configuration. The JSON request body is exactly
`{ "input": <EndpointInvocation> }`. The mobile worker sends the engine's stable
request and attempt identities as bounded correlation headers. One attempt can
issue an interpretation request and then a dialogue request, so the engine's
`idempotencyKey` carries the stage (`<request>:<attempt>:interpretation` or
`<request>:<attempt>:dialogue`) and the two never collide in an Endpoint
idempotency cache. The server
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
