# ADR 012: Mobile invocation identity and validated streaming

## Status

Accepted.

## Decision

The data plane accepts two authenticated principal types: scoped consumer API
keys and mobile Firebase principals. Mobile authentication verifies both a
Firebase Auth ID token and a Firebase App Check token. Server configuration
binds each accepted App Check app ID to one explicit organization identity,
organization slug, immutable Endpoint-version allowlist, and daily quota. Mobile credentials
never authorize creator-control routes.

Both principal types pass through the same Endpoint resolution, tenant,
quota, rate, and global/organization/Endpoint/provider kill-switch checks.
Invocation records identify the calling organization; API-key invocations
also retain their key ID, while mobile invocation rows leave that nullable
field empty. Raw player input and model output are not retained.

Completed JSON invocation routes remain supported. Alias and immutable-version
routes also expose `/stream` variants using the same `{ "input": ... }`
envelope, but mobile principals must call an explicitly allowed immutable
version rather than the mutable alias. Streaming is an explicit, versioned Endpoint capability with a
configured top-level text projection. The first projection is `dialogue`.
OpenAI remains completed-response only; Google is the first streaming
provider.

The public SSE contract is version 1. Every event carries the request,
attempt, invocation, event, Endpoint-version, and contiguous sequence
identities. Only decoded text from the configured output field can appear in
`text_delta`; raw structured JSON, reasoning, and unrelated fields remain
quarantined. A successful stream ends in exactly one independently validated
`final` output. A started failure ends in exactly one safe `error` event.

Disconnect and deadline signals cancel provider work and are recorded as safe
invocation metadata. Buffers, event counts, request bodies, and output are
bounded. A partially delivered stream is never automatically retried, and
correlation headers do not promise replay.

## Consequences

The native app can call the data plane directly without a shared consumer key
or provider credential. Parish on device remains the authority: deltas are
provisional, and only the engine's canonical validation of the terminal
candidate can commit gameplay or durable transcript state.

Endpoint publication and application deployment remain separate operations.
Immutable versions and the production alias continue to provide behavioral
rollback; Cloud Run revisions provide service rollback.
