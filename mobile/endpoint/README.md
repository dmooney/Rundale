# Rundale Endpoint definitions

The Endpoint definitions are game data. They live in the world's mod, one file
per inference role, and the mod manifest declares them (ADR-025 §5):

```toml
# mods/rundale/mod.toml
[endpoints]
dialogue = "endpoints/rundale-dialogue.v1.json"
intent = "endpoints/rundale-intent.v1.json"
```

Each file is named `<slug>.v<version>.json`; the name is the Endpoint's
identity, and the body is the `EndpointDefinition` consumed by Limerick
Endpoints: `inputSchema`, `outputSchema`, `instructions`, `providerConfig`, and
`inferenceConfig`. The engine loads them with the mod (`limerick_mod::endpoints`)
and attaches each role's reference and structured input to the calls it makes
(`limerick_core::endpoint_input`).

| Role     | File                                                                                | Output                                     |
| -------- | ----------------------------------------------------------------------------------- | ------------------------------------------ |
| Dialogue | [`rundale-dialogue.v1.json`](../../mods/rundale/endpoints/rundale-dialogue.v1.json) | `{ "dialogue": "..." }`, streamed as text  |
| Intent   | [`rundale-intent.v1.json`](../../mods/rundale/endpoints/rundale-intent.v1.json)     | A probability per option for each question |

Until the game is released, each definition stays at version 1: a change edits
the `.v1.json` file and replaces the published copy in place (`pnpm definitions
replace`). After release, published versions become immutable and a change
ships as a new `<slug>.v<version>.json`. The engine builds each input with
`EndpointCall::invocation`.

Only version 1 of each Endpoint is published. The richer `ios-port` dialogue
contract (acquired knowledge, player memory, task offers) was published to
`limerick-prod` as v2 and has been removed; when the engine builds those fields,
they land by replacing v1. Dialogue targets `google/gemini-3.5-flash-lite`
with no retry, allows 1,024 output tokens and streams the `dialogue` field.

Intent is a decision Endpoint (Limerick Endpoints ADR 014) answered by Jev
(`jev-1.13.0`) on TypeSafe's own API. It does not stream. One call settles the
action (move, talk, look, examine, interact or none), the destination, the
phrase acted on, who a line is spoken to, who it mentions, and its atmospheric
subject ([dynamic NPC context §4.6](../../docs/design/dynamic-npc-context.md)).
The model only selects among options the engine offers, and the engine checks
every answer against them (`limerick_core::intent_decision`).

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

The intent input adds `role: "player_intent"`, the bounded `line` the
deterministic local parser did not recognise, and the options the decision
chooses among: `present` (people in the room), `elsewhere` (introduced people
elsewhere), `references` (words that fit several people present), `places`, and
`phrases` cut from the line. Each option carries an `optionId` assigned per
call.

The dialogue input adds `role: "npc_dialogue"`, `playerInput`, and one
speaker's snapshot of the present moment, holding only what that speaker could
know ([dynamic NPC context §6](../../docs/design/dynamic-npc-context.md#6-dialogue-snapshot)):

- `speaker`: id, name, occupation as `role`, current location, and authored
  `mood` from `npcs.json`.
- `addressing`: `spoken_to`, `part_of_all`, or `joining_mentioned`. Until
  recipient resolution records it (#2221), every speaker is `spoken_to`.
- `currentLocation`: the place rendered for the current time and weather,
  naming the people there.
- `time` (`date`, `clockTime`, `partOfDay`), `weather`, and `festival`
  (today's festival from `GameClock::check_festival`, or null).
- `present`: the other people in the room with the speaker.
- `knownPeople`: every NPC by id, name, and occupation, with no location.
- `knownPlaces`: every place, rendered without naming anyone there.
- `authoredFacts`: the speaker's `knowledge` from `npcs.json`.
- `recentConversation`: the last 8 exchanges the speaker witnessed, oldest
  first, with the snake_case fields `timestamp`, `speaker_id`,
  `speaker_name`, `player_input`, `npc_dialogue`, and `location`. Each
  committed exchange records the NPCs present; exchanges in saves written
  before that count as witnessed only by their speaker.
- `player`: `knowsName` and, when the speaker knows it, the player's `name`
  (otherwise null).
- `maxOutputChars` (8,192) and `maxStreamBytes` (16,384).

People are `npc-<id>` and places `place-<id>`. The engine caps people, places,
and facts at 32. Each speaker's snapshot is built when that speaker's call is
built, so in a line with several speakers a later speaker's snapshot includes
the replies already committed.

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
call's rendered prompt, so desktop provider requests are unchanged. The iPhone
app is the Endpoint host: the FFI hands it each pending call's reference and
`input` (`pending_endpoint`), and the app posts the request body to that
version's `/stream` route (see the
[FFI README](../../limerick/crates/limerick-mobile-ffi/README.md#inference)).

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

## Live Endpoint suite

`RundaleLiveEndpointUITests` plays the app against a deployed Limerick
Endpoints service: a dialogue turn (`rundale-intent` v1, then
`rundale-dialogue` v1), Stop during a streamed reply, free-form movement
classified by `rundale-intent` v1, and "ask Connolly about the household" at
the cottage asking which Connolly before anyone answers, and the #1901 probe
that an accepted reply agrees with the speaker's current location. It is opt-in and needs a Firebase App Check
debug token registered for the Rundale iOS app. Against `limerick-prod`:

```sh
bash mobile/scripts/build-rust-mobile.sh
xcodegen generate --spec mobile/project.yml
TOKEN=$(endpoints/deploy/limerick-prod.sh appcheck-token create <name>)
TEST_RUNNER_AppCheckDebugToken=$TOKEN \
TEST_RUNNER_RUNDALE_LIVE_ENDPOINT_BASE_URL=https://limerick-endpoints-877612517009.us-east1.run.app \
  xcodebuild -project mobile/Rundale.xcodeproj -scheme Rundale \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' \
  -only-testing:RundaleUITests/RundaleLiveEndpointUITests test
endpoints/deploy/limerick-prod.sh appcheck-token delete <name>
```

The base URL defaults to that `limerick-prod` origin and the organization to
`limerick-demo` (`RUNDALE_LIVE_ENDPOINT_BASE_URL`,
`RUNDALE_LIVE_ENDPOINT_ORGANIZATION` override them). The Firebase
`GoogleService-Info.plist` is supplied privately and is ignored.
