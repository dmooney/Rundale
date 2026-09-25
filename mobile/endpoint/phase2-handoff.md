# Phase 2 Endpoint integration handoff

## Current agreement (migrated 2026-09-25)

The repository contract is [rundale-dialogue-v1.json](rundale-dialogue-v1.json).
The embedded `endpoints/` service implements its pinned streaming route and the
shared Swift/Rust/TypeScript fixture freezes the public wire shape. Limerick
Endpoints now runs in the dedicated `limerick-prod` project as Cloud Run service
`limerick-endpoints`, origin `https://limerick-endpoints-877612517009.us-east1.run.app`.
Organization `limerick-demo`, Endpoint `rundale-dialogue`, immutable version `1`
is pinned by the app, as is `rundale-intent` version `1`. No production origin is
compiled into normal app configuration.

The migration used a clean database. `rundale-dialogue` versions 1 and 2 and
`rundale-intent` version 1 were republished from the previous deployment's exact
definitions; their content hashes are unchanged (`sha256:d2a58dc2...`,
`sha256:4749a9a9...`, `sha256:8705607e...`). The repository copy of dialogue v1
differs from the published v1 only in an input-schema description string renamed
from `parish_core` to `limerick_core`; the wire contract is identical. The mobile
allowlist binds Firebase app `1:877612517009:ios:586f98a2cc3e7d0c676130` to
`rundale-dialogue` versions 1 and 2 and `rundale-intent` version 1. The previous
`cottage-d6dc9` deployment is retired and its receipts below are historical.
Private Firebase configuration remains worktree-local and ignored.

## Trusted boundary

The native client sends its Firebase Auth ID token and mandatory App Check token
directly to the configured Parish Endpoints origin. Parish verifies both tokens
with the Firebase Admin SDK, resolves the App Check app ID through the strict
`MOBILE_APP_BINDINGS_JSON` allowlist, and checks organization UUID/slug,
Endpoint slug, status, provider/model switches, per-principal rate limits, and
the minimum applicable daily quota before invoking a provider. A failed mobile
check does not fall through to creator authorization. Existing Endpoint API-key
invocation and creator authentication remain separate paths.

No provider credential or shared invocation secret ships in the app. Parish
Endpoints owns Vertex/OpenAI credentials and provider dispatch; the embedded
Parish engine remains authoritative for gameplay validation and commit.

## Streaming delivery

The pinned route is
`/v1/endpoints/{organization}/{slug}/versions/{version}/stream`. It accepts only
`{ "input": <EndpointInvocation> }`, requires bounded `X-Request-Id` and
`X-Attempt-Id` correlation, and responds as `text/event-stream`. Contract v1
uses ordered, bounded `progress`, `text_delta`, `final`, and `error` frames.
Each frame carries the request, attempt, invocation, event, sequence, Endpoint
version, and terminal metadata validated by `ParishEndpointKit`.

Google is the first native streaming provider. The server incrementally decodes
only the configured top-level `dialogue` JSON string, never exposes unrelated
raw JSON, bounds frames and decoded output, and validates the complete output
schema before `final`. Disconnect and Stop abort provider work and are recorded
as cancellation. The embedded engine treats every delta as provisional and
alone may accept and commit the final candidate.

## Deployed evidence (2026-09-09)

- Cloud Run revision `parish-server-00006-kew`, immutable image digest
  `sha256:a10e328c9c698bb9f881ca8f903ddd7b4418663a1c6d05950323daf86a996dad`,
  serves 100% of traffic. Revision `parish-server-00004-xiq` remains available
  for application rollback.
- Migration execution `parish-migrate-z2qbw` succeeded before traffic moved.
  The migration adds a durable cancellation marker used in the conditional
  terminal transition.
- The published immutable v1 snapshot matches the checked-in definition at
  `sha256:d2a58dc263543789c19a3bc5d3d934db7fee7e8fba81d5d01716bcf03315cae1`
  and is promoted to `production`. The mobile allowlist binds only the registered
  Rundale iOS app to `parish-demo/rundale-dialogue@1`.
- Missing credentials were rejected with 401 on the tagged revision before
  traffic moved. The canonical live and ready checks returned 200 afterward.
- Native iOS simulator test 01 used real anonymous Firebase Auth, a privately
  registered App Check debug token, the pinned route, and Vertex Google streaming.
  It committed a schema-validated terminal dialogue; the database recorded
  `succeeded` in 897 ms.
- Native simulator test 02 pressed Stop during inference. The UI reported
  `Interrupted; not applied`, committed no late dialogue, the DELETE returned
  202, and the same production invocation row recorded `failed`,
  `REQUEST_CANCELLED`, `cancellation_requested=true` in 538 ms. Cloud Run logged
  the matching POST and authenticated DELETE on revision `00006-kew`.
- The independent auth/stream review found no remaining actionable issue after
  the persisted Stop-versus-success race fix and its blocked-accounting test.

This evidence establishes the deployed simulator path; it does not establish
App Attest, signing, networking, lifecycle behavior, or usability on a physical
iPhone. No gateway or Endpoint consumer key is part of the mobile architecture.

## Intent role (#1993)

The repository now defines
[`rundale-intent-v1.json`](rundale-intent-v1.json), and the app routes
`role: "intent"` invocations to `limerick-demo/rundale-intent@1`. The deployed
mobile allowlist includes that version, but configuration inspection alone does
not prove that its immutable definition and provider route execute successfully.
An authenticated live Intent run through the native composer remains the
closure gate. Do not treat the allowlist entry or a dialogue response as proof
of the selected intent action.
