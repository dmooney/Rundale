# Phase 2 Endpoint integration handoff

The separate Parish Endpoints task owns service implementation and deployment.
This Rundale task has not changed `/Users/dmooney/parish-endpoints` or deployed
its proposed service changes. The native client and this contract are ready for
integration; real remote delivery remains an acceptance gate.

## Native request

- Pinned route: `POST /v1/endpoints/{organization}/{slug}/versions/{version}/stream`.
- Proposed identity: organization `rundale`, slug `rundale-dialogue`, version `1`.
  These records have not been provisioned by this task.
- Body: `{ "input": <EndpointInvocation> }`. Use
  [the definition](rundale-dialogue-v1.json) and
  [the production Rust fixture](example-engine-invocation.json).
- `Authorization: Bearer <Firebase ID token>` and `X-Firebase-AppCheck`.
- `X-Request-Id` is the durable logical request ID. `X-Attempt-Id` changes on
  retry. `Idempotency-Key` combines both identities. The client does not assume
  remote replay semantics; it rejects obsolete results before local commit.
- Credentials follow Cottage's anonymous Firebase Auth and App Check pattern.
  Rundale's registered public iOS app ID is
  `1:24861210203:ios:2df6bf4ed8c4828253b17e`, bundle `com.rundale.mobile`, in
  project `cottage-d6dc9`. Provider credentials remain server-side.

## Stream

Every SSE data object carries `contract_version: 1`, `request_id`, `attempt_id`,
`invocation_id`, `event_id`, `sequence`, `endpoint_version`, and `type`.
The SSE event name agrees with `type`. Sequence numbers increase strictly;
`event_id` is `invocation_id:sequence`. The server selects the invocation ID and
the client pins it from the first valid frame. The integer endpoint version
must equal the requested pinned version on every frame.

Types are `progress`, `text_delta`, `final`, and `error`. `text_delta.text` is an
incremental decoded text chunk. The final object is exactly
`output: { "dialogue": "..." }`; it remains subject to the engine's canonical
validation. A stream ending without a terminal frame fails. Cancellation and
late results cannot commit a stopped or superseded attempt.

The proposed provider configuration uses `google/gemini-3.5-flash-lite`, 1,024
output tokens, zero automatic retries, and explicit
`inferenceConfig.streaming.textField = "dialogue"`.

## Isolated proposal and evidence

An isolated service proposal is available at
`/private/tmp/rundale-phase2-endpoints`; it must be reconciled with the service
task's current changes before any application. The earlier incremental patch
at `/private/tmp/rundale-phase2-endpoint-review.patch` is **not** a delivery
patch: validation found malformed trailing diff headers in two migration JSON
files, corrected only in the isolated working copy.

The proposal addresses Firebase consumer scoping, real provider streaming,
organization policy and kill switches, atomic quota reservation, complete
attempt usage/cost accounting, and distinct attempts for one logical request.
Without the request/attempt database change, mobile Retry collides with the
original unique `request_id` constraint.

On the isolated copy: server dependency build passed; 56 tests passed with two
opt-in tests skipped. Migrations passed on disposable PostgreSQL 17. A separate
five-way race with one quota slot admitted one request and rejected four with
`QUOTA_EXCEEDED`. The container was removed afterward. Workspace aliases were
pinned to the isolated sources to avoid resolving stale original build output.
Lint still reported one unused parameter in the proposed memory repository.
No live-provider or deployed-service claim follows from these checks.

Automatic approval review rejected worker mutations involving authorization,
quotas, and cost accounting without explicit production-change authorization.
Only isolated proposal validation proceeded. Service publication, app consumer
grants, and the real native streaming demo remain with the integration work.
