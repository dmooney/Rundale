# Rundale dialogue Endpoint definition

[`rundale-dialogue-v1.json`](rundale-dialogue-v1.json) is the version 1
Endpoint definition for the Phase 2 NPC dialogue role. It is the
`EndpointDefinition` body consumed by Parish Endpoints: `inputSchema`,
`outputSchema`, `instructions`, `providerConfig`, and `inferenceConfig`.
[`example-engine-invocation.json`](example-engine-invocation.json) is a
secret-free serialization produced by the Rust `EndpointInvocation` DTO and
checked against it in the `parish-core` fixture test.

The proposed public identity is slug `rundale-dialogue`, version `1`. This
repository artifact is configuration for publication; it does not claim that
an Endpoint record, production alias, API key, or deployed service exists.
The first provider target is `google/gemini-3.5-flash-lite`, with 1,024 output
tokens, no retry, and the required streaming projection
`inferenceConfig.streaming.textField = "dialogue"`.

## Engine wire agreement

The input schema is the JSON serialization of
`parish_core::mobile::EndpointInvocation` in
[`parish/crates/parish-core/src/mobile/mod.rs`](../../parish/crates/parish-core/src/mobile/mod.rs).
The Rust DTO uses `serde(rename_all = "camelCase")`, so the request uses
`sessionID`, `logicalRequestID`, `attemptID`, `playerInput`,
`currentLocation`, `knownPeople`, `knownPlaces`, `authoredFacts`, and
`recentConversation`. All fields are required and unknown fields are rejected.

Opaque mobile IDs serialize as strings. `baseRevision` is the explicit
`{"rawValue": number}` `StateRevision` shape. Engine `u32` identifiers in
grounded people, grounded conversation speakers, and locations remain JSON
integers. `recentConversation` contains `parish_types::ConversationExchange` values,
which retain that type's default snake_case serde field names. It therefore
uses the exact shape of
[`ConversationExchange`](../../parish/crates/parish-types/src/conversation.rs):
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

Publish this definition as an immutable Endpoint version, then route the
mobile worker through the deployed Parish Endpoints invocation API using the
normal authenticated consumer path. The JSON invocation body is wrapped by
that API as `{ "input": <EndpointInvocation> }`; the worker supplies its
idempotency and attempt correlation headers from the same request identities.
The mobile client never receives provider credentials or creator instructions.

The definition intentionally contains no API keys, Firebase tokens, endpoint
URLs, organization identifiers, or deployment alias. Fill those values in
the authorized service/configuration path when the Endpoint is provisioned.
