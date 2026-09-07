# Rundale Mobile — Software Technical Vision

Architecture for the text-first reset and all six delivery phases

> Status: Proposed technical direction, version 1.1. Scope: Native iPhone client, embedded Parish Engine, Rundale content, and the integration contract with Parish Endpoints. This is a companion to the current Product & Technical Specification, not a replacement for its requirements or a claim that any milestone is complete.

Editorial import note: references to the native Product & Technical Specification, including [P1], resolve within this repository to the versioned [Product & Technical Specification](product-technical-spec.md). The native document remains source provenance; this note does not change the imported source text or its proposed status.

## 1. Technical north star

Rundale should become a small, dependable local application that can grow into a richer living world without changing who owns the game. SwiftUI provides the reading-and-typing experience. The embedded Rust Parish Engine interprets commands, enforces rules, advances simulated time, manages NPC state, and commits saves. Parish Endpoints supplies remote inference. The network never becomes the authority for location, knowledge, tasks, or story history. [P1]

The important investment is not an elaborate framework. It is a few durable boundaries: intent versus execution, proposed output versus committed facts, authored content versus mutable saves, presentation events versus engine internals, and remote computation versus local authority. These boundaries must exist in the tiny implementation even when only one NPC and one location are enabled.

The first prototype should therefore be deliberately narrow but structurally representative. A fixture response and a real Parish response should pass through the same presentation contract. A one-NPC conversation should use the same request identity, cancellation, persistence, and validation rules that will later protect gossip and tasks. The three-location world should be ordinary validated content, not a special engine mode built around three hard-coded objects.

The desired end state remains a text adventure. Supporting more NPCs, richer memory, optional undo, or eventual save synchronization must not require a new player interface. Conversely, keeping future options open does not authorize building those features early. The project can remain text-only indefinitely. [P1]

Architectural success test: adding a new world behavior should normally require a bounded engine/content change, relevant tests, and perhaps an additional semantic event—not a rewrite of the composer, the storage ownership model, or the Endpoint integration.

## 2. Authority, assumptions, and source alignment

### 2.1 What this document treats as fixed

The current native Google Docs specification is the governing product source. It requires SwiftUI on iPhone, on-device Rust gameplay, authoritative local saves, remote inference exclusively through Parish Endpoints, and the six existing delivery milestones. It deliberately resets both frontend and world/NPC data while retaining useful Parish Engine capabilities. Existing graphical interfaces and old content do not define compatibility obligations. [P1]

This vision makes additional technical recommendations. Where the product spec does not choose a mechanism, recommendations below are defaults to validate, not retroactively approved product requirements. No individual class hierarchy, exact database table inventory, or comprehensive internal API is prescribed.

This is a target architecture, not a repository audit. Before modifying the engine, inspect the current repository to identify reusable implementations and actual portability constraints. Do not assume a capability is absent merely because it is not described here, or correct merely because it exists.

### 2.2 A cross-project dependency that must be resolved

The Rundale spec requires streaming NPC output through Parish Endpoints during Phase 2. The separate Parish Endpoints architecture currently excludes streaming from its original MVP. It also describes client-safe OIDC/JWT invocation as planned, while allowing owner-only dogfood work to precede that capability. Those are document-level integration gaps; they are not evidence of the deployed service's actual feature set. [P1, P2]

Rundale therefore needs a small, explicit Endpoint capability agreement before Phase 2 can be accepted: a versioned inference contract, authenticated mobile-safe invocation, incremental response delivery, a validated terminal result, bounded failure/cancellation behavior, and request correlation. Implement missing capabilities in the generic Parish Endpoints product rather than bypassing it with direct provider calls or inventing a Rundale game server.

This document does not change either source document. The integration owner should reconcile their requirements when implementation work begins. Marketplace features, billing systems, and a broad Endpoint platform expansion are not prerequisites for this narrow integration.

### 2.3 Assumptions selected for the reset

The recommended early runtime is single-player and single-writer, with one state-changing player request active at a time. Game time advances through accepted gameplay outcomes rather than real-world waiting. The world does not continue simulating while the phone is suspended. Initial content ships with the app. Cloud saves, a graphical map, a save-branch interface, and on-device inference are not required.

These assumptions reduce concurrency and lifecycle ambiguity without making the domain model depend on an iPhone screen or a single NPC. Revisit them only through an explicit design decision with migration and test consequences.

## 3. Decisions to establish early—and what to leave replaceable

| Area                      | Initial direction                                                                       | Future option preserved                                                        |
| ------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Player experience         | SwiftUI shell; transcript, compact status header, native composer                       | Replace a difficult scrolling/text component without replacing game logic      |
| Game authority            | One embedded Rust runtime owns mutable world state                                      | Other clients can reuse the engine without duplicating rules                   |
| UI contract               | Versioned semantic events and read-only projections                                     | Richer behaviors and optional secondary views without leaking engine internals |
| Request execution         | Durable command receipt; staged outcome; one atomic world commit                        | Safe retry, future undo, and reliable long-running inference                   |
| Local storage             | Prefer one transactional SQLite-backed save boundary, subject to existing-engine review | Migrations, paging, consistent export, and later synchronization               |
| Foreign-function boundary | Small owned-value interface; evaluate generated Swift bindings before choosing          | Change binding tooling without changing the game contract                      |
| Networking                | Parish Endpoint adapter behind a narrow transport capability                            | Provider changes and endpoint evolution without provider SDKs in the client    |
| Time and randomness       | Explicit game clock and persisted/versioned randomness state                            | Reproducible schedules, controlled catch-up, and deterministic tests           |
| Content                   | Stable entity IDs; immutable content definitions; separate mutable state                | New content packs and save-compatible expansion                                |
| NPC cognition             | Structured knowledge/provenance; model output is a proposal                             | Gossip, memory, and tasks without transcript-driven state inference            |
| Testing                   | Shared semantic fixtures, headless engine tests, real iPhone gates                      | Implementation changes without relying on live models for correctness          |
| Deployment                | Reproducible iOS builds; immutable Endpoint versions with recorded resolution           | Safe rollback and diagnosis across independently deployed components           |

Do not build a plugin marketplace, distributed simulation, generic workflow engine, entity-component framework, vector database, CRDT layer, or arbitrary scripting system merely to preserve optionality. Prefer a specific extension point at a known boundary over speculative infrastructure.

## 4. System boundaries and ownership

### 4.1 Logical components

These are responsibility boundaries, not a requirement to create a separate package or class for each row.

| Component                  | Owns                                                                                                | Must not own                                                   |
| -------------------------- | --------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| iOS presentation           | Transcript rendering, composer, focus, scrolling, accessibility, transient UI state                 | World rules, NPC memory, provider routing, save-state mutation |
| iOS platform services      | App lifecycle, native networking, credential access, sandbox paths, OS integration                  | Game-specific prompt selection or adjudication                 |
| Parish application runtime | Request lifecycle, intent resolution, validation, orchestration, commit ordering                    | UIKit/SwiftUI layout and provider secrets                      |
| Parish simulation          | Locations, presence, time, schedules, weather, relationships, knowledge, tasks                      | HTTP, view identity, or app navigation                         |
| Local persistence boundary | Durable requests, committed world state, transcript records, migration/export                       | Inferring state from prose or making network calls             |
| Rundale content            | Authored definitions, initial conditions, bounded rules and fixtures                                | Mutable player progress or hidden per-device state             |
| Parish Endpoints           | Authorized inference execution, provider credentials/routing, published behavior versions, metering | Authoritative game state or autonomous changes to the save     |

The client-to-engine dependency points inward. Platform capabilities are injected through narrow interfaces where the engine needs them. The simulation must be testable without SwiftUI, a network connection, or a model provider. The iOS renderer must be testable without compiling or launching Parish.

### 4.2 Keep three representations separate

Authoritative domain state says what is true in the game: where someone is, what they know, whether a task is complete, and what game time it is.

Interaction history records what the player submitted and what was presented, including interrupted attempts. A failed command belongs in interaction history even when it changed no world state.

Presentation state says which transcript entries are loaded, which row is streaming, where the player is reading, and what is in the draft composer. It may be rebuilt from durable records, but it is not the save itself.

These representations may share IDs and a storage transaction without becoming the same data structure. Rendering an old scene heading must never move the player. Loading a transcript must never replay a command against the current world. Trimming an inference context must never delete an NPC's authoritative memory.

### 4.3 Portable engine boundary

Keep Tauri, Axum hosting, desktop process control, local-model launching, web authentication infrastructure, and developer tools outside the iOS gameplay dependency graph. Prefer explicit build targets or feature separation over importing a desktop runtime and hoping unused paths never execute. Retain established desktop behavior where practical without making desktop feature parity a condition of the mobile reset. [P1]

A headless host for tests and diagnostics can use the same portable engine. It is a development tool, not a second implementation of the game or a requirement for a player-facing debug screen.

## 5. Stable contracts across the Swift/Rust boundary

### 5.1 An application-facing contract, not an engine mirror

The boundary needs capabilities to create/resume a local session, submit player intent, answer a clarification, stop a request, retrieve a consistent view of current state, page through transcript history, and receive correlated events. Exact function names and serialization are implementation choices.

The contract should transfer owned values or well-defined opaque handles. It must define lifetimes, disposal, error propagation, callback threading, cancellation, and maximum payload sizes. Swift must not hold mutable pointers into an NPC graph or call arbitrary engine internals to populate a screen.

Expose semantic commands and queries, not a generic “execute Rust function” mechanism. For example, completion asks for currently relevant entity references; it does not fetch every internal NPC object. A location summary carries player-visible facts, not hidden NPC knowledge.

### 5.2 Identity and versioning

Use stable identifiers independently of display names and array positions. The earliest real save should distinguish a game/session identity, content version, state revision, logical request ID, execution attempt ID, transcript item ID, and committed event ordering. A future branch/checkpoint must be referencable without renumbering historical identities.

A logical request is the player's intended submission. An attempt is one execution effort for that request. A transcript item is a visible unit such as a command or an NPC response. Multiple stream updates modify one item; they are not separate dialogue messages. A committed state revision identifies the exact world against which an outcome was validated.

Version the presentation contract, save format, content schema, and Endpoint input/output contract separately. Their release cycles differ. A provider/model name is not a substitute for an inference contract version. Unknown mandatory behavior should produce a controlled compatibility error; an unknown optional presentation decoration may degrade to a safe text fallback.

### 5.3 Semantic events and projections

Start with the concepts in the product spec: scene change, player command, interpretation receipt, narration, NPC dialogue, action result, clarification, progress, error, and response completion. Keep terminal success, cancellation, interruption, and failure distinguishable even if represented by a shared event envelope. [P1]

Events need sufficient context to identify their request/attempt, speaker or location when relevant, logical order, provisional/committed status, and resulting state revision where applicable. Hidden state is not carried merely because the engine knows it. Use bounded structured metadata for entity references and interaction choices rather than encoding clickable semantics into arbitrary Markdown.

A read model for the current header, nearby people, and exits must come from one consistent state revision. Update it together with, or immediately after, the corresponding committed action. Never compute it by scanning the last generated paragraph.

### 5.4 Gap-free restoration and bounded delivery

The runtime must support a consistent snapshot plus an event cursor, or an equivalent subscription handshake that cannot miss a commit between “read current state” and “start listening.” Repeated delivery of the same committed record must be harmless.

Do not persist every token as a permanent transcript event. Maintain a bounded provisional stream buffer with per-attempt sequencing and a final durable item. Chunk boundaries must not split or corrupt Unicode. Coalesce UI updates, but do not discard terminal transitions or state-changing records under backpressure.

Full token-level stream resumption is not required initially. Restoring the committed transcript and showing a clearly interrupted provisional attempt is sufficient. Preserve the identifiers needed to add stronger recovery later without pretending a network stream is a durable event log.

## 6. Runtime execution, atomicity, and cancellation

### 6.1 One authority, one commit lane

Use a serialized execution context for authoritative mutations. Expensive inference and transport run asynchronously outside that context; their results return as candidates associated with an expected state revision. Do not hold a database transaction or engine lock open while waiting for a remote model.

Initially, allow one active state-changing player request. The player can continue composing while it runs. Read-only inspection may operate on the last committed snapshot, provided it cannot advance time or mutate the pending turn. Do not introduce parallel NPC agents or concurrent player turns in order to make a tiny game feel sophisticated.

The logical single-writer rule does not require all work to run on the UI thread. Swift presentation updates belong on the appropriate UI isolation context; storage, parsing, validation, simulation work, and FFI operations that can block must not stall typing or scrolling.

### 6.2 Two durable boundaries

Acceptance boundary: record the logical request, original text, relevant targeting references, base state revision, and its accepted status. The player-command transcript item becomes durable here. Clear the composer only after acceptance, and use the same submission ID to reconcile a crash or duplicate acknowledgment.

Gameplay commit boundary: after interpretation, inference where required, and validation, commit the resulting domain changes, final authoritative transcript records, request terminal outcome, and new state revision together. This is the point at which the game has actually changed.

The accepted command may therefore survive a failed turn. That is not a partial world commit; it is an accurate record of an attempted action. A successful UI completion indicator must follow the durable gameplay commit, not precede it. Optional cloud upload happens later and cannot delay local completion.

### 6.3 Recommended request state machine

A request moves through accepted, interpreting, possibly awaiting clarification, executing or awaiting inference, validating, and then a terminal outcome. Internal state names are not mandated, but the legal transitions and their persistence meaning must be testable.

Terminality is per attempt, except that a successful gameplay commit permanently settles the logical request. Retrying an uncommitted request creates an explicit new attempt; it does not erase or rewrite the previous attempt’s terminal record. Only the currently authorized attempt can become the logical request’s committed outcome.

Clarification preserves the original intent and identifies bounded choices. Selecting a choice continues that request rather than creating an unrelated command. On resume, revalidate the choice against the saved state. A stale target or changed precondition must not silently redirect an action to a different NPC.

Before commit, the engine checks the expected revision, entity existence, presence, permitted actions, task preconditions, output bounds, and the request's cancellation/terminal state. A response from an obsolete attempt cannot win merely because it arrives last.

### 6.4 Stop and retry semantics

Stop is a request to cancel uncommitted work, not a general rollback feature. It should immediately change visible activity state, propagate cancellation through the runtime and transport, and prevent a late result from committing. The runtime serializes the final cancel-versus-commit decision.

If commit has already won, the action stays committed and the UI reports completion. Do not pretend that a late Stop undid the world. Future undo is a separate operation with a separate checkpoint/branch policy.

A retry of a failed, uncommitted action retains its logical request identity and original player intent, but receives a new attempt identity. A network retry of the same remote invocation should retain the same remote idempotency key and request fingerprint where the Endpoint contract supports it. A deliberate new model generation is a new remote invocation/attempt, not a fabricated replay of the old one.

After subsequent gameplay has changed the base state, the old request cannot simply commit against its old assumptions. Revalidate and obtain a new interpretation as necessary, or offer an explicit new submission. Copying a completed command into the composer is always a new player action when sent.

### 6.5 Failure cases that define the design

| Interruption point                                | Required result                                                                                                           |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Before durable acceptance                         | Keep recoverable draft text; do not claim the command was accepted                                                        |
| After acceptance, before gameplay commit          | Restore the command as pending/interrupted; world remains at its last committed revision                                  |
| During incremental model output                   | Preserve or discard the provisional excerpt according to a documented UI policy; never promote it to memory or task state |
| While committing                                  | Recover either the complete old state or complete new state, never a mixture                                              |
| After commit, before UI acknowledgment            | Restore the committed result and deduplicate delivery; do not execute again                                               |
| Endpoint completed but client lost the connection | Recover the same result when supported, otherwise report interruption; local commit protection remains authoritative      |
| Stop races a final response                       | Exactly one local terminal decision wins; late callbacks cannot mutate the session                                        |
| Save write fails or disk is full                  | Preserve the last valid save and expose a recoverable error; never show an unsaved action as durably completed            |

The achievable promise is at-most-once committed local effects for a logical request, backed by durable deduplication. Do not promise exactly-once execution or exactly-once billing across an unreliable network and a remote provider. Endpoint idempotency reduces duplicate work but cannot repair incorrect local commit semantics. [P2]

## 7. Persistence, history, migrations, and future saves

### 7.1 Storage recommendation

Prefer a single transactional SQLite-backed persistence boundary owned by the Parish runtime for requests, authoritative world state, and committed transcript history. A consistent transactional store makes the gameplay commit boundary practical; SQLite documents atomic transaction behavior and recovery mechanisms. This recommendation still requires verification of the chosen binding, journal configuration, durability settings, and iOS file behavior. [T4]

First inspect existing Parish persistence and branching. Reuse it if it satisfies these boundaries and works on-device. Do not add a second authoritative database because SQLite appears in this document. If replacement is necessary, identify the missing property and migrate deliberately rather than maintaining two save systems indefinitely.

Avoid making SwiftData/Core Data the authoritative game model while Rust maintains a separate mutable copy. Avoid a single ever-growing JSON save rewritten for every token, or an append-only prose file from which game state must be guessed. Simple JSON remains appropriate for test fixtures, content exchange, or an export format; it is not a substitute for defined transaction semantics.

### 7.2 Durable records and projections

Persist enough to restore accepted commands, terminal request outcomes, state revisions, final transcript items, content/schema versions, game time, and any randomness state required for future deterministic progression. Knowledge, task state, schedule position, and weather are domain data, not incidental text fields in a dialogue log.

A materialized current state plus a durable interaction/commit history is sufficient. Full event sourcing from the first prototype is not required. Historical reconstruction must use recorded outcomes or checkpoints, not rerun past prompts against a current model. Generated dialogue is nondeterministic; saved output is the historical fact that it was shown.

Keep full interaction history separate from the bounded window loaded by the UI and the much smaller context selected for inference. Paging is a read operation, not an excuse to remove old committed turns. Introduce compaction or archival only with an explicit retention policy that preserves required history and future checkpoint references.

### 7.3 Draft and viewport restoration

Treat the draft as durable player input with its own lightweight persistence policy. Save it independently of gameplay completion, including selection/targeting information needed to restore an intelligible command. Flush at safe lifecycle boundaries and use a short debounced write policy during editing; do not rely exclusively on a termination callback.

During submission, associate the outgoing draft with its request ID until the accepted receipt is durable. A crash after acceptance but before clearing the local draft must not show two accepted commands or trigger resubmission. Restore the appropriate empty/new draft while leaving the accepted command in history.

Restore reading position using a stable transcript item anchor and an offset or equivalent logical position. Raw pixel offsets alone are fragile when font size or device geometry changes. Preserve the difference between following new output and deliberately reading history.

### 7.4 Migration and content compatibility

Give save-format changes explicit, ordered migrations. Test them using real prior-format fixtures. Preserve the original valid save until the migrated copy has been validated and adopted; failed migration must not quietly create a fresh game over the player's data. A newer unsupported save must produce a clear compatibility error.

Content updates are a separate problem from database schema changes. A save should identify the content definition version or fingerprint it depends on. Renaming a location should not change its stable ID. Removing or changing an entity referenced by a save requires a content migration, a pinned old content version, or an explicitly incompatible new-game path—not best-effort guessing.

Resetting legacy frontend/content does not imply that every existing pre-reset save needs migration. Decide legacy-save compatibility explicitly before distributing the first mobile save format. Once players have relied on mobile saves, preservation becomes an ongoing engineering obligation.

### 7.5 Future checkpoints, undo, and cloud synchronization

Preserve checkpoint identity and ancestry where existing engine support already exists, or leave a small explicit place for them in save metadata. Do not expose a save DAG or build a general branching UI in early phases. Future undo should select or create a valid alternate state lineage; deleting the last transcript paragraph is not undo.

Optional synchronization should transfer consistent, versioned save snapshots or logical change packages, never an actively mutating database file. SQLite's backup facilities provide a consistent snapshot approach; in WAL mode, committed data may still reside outside the main database file, so copying that file alone is not an acceptable export protocol. [T5, T6]

A future sync adapter needs save identity, checkpoint/state revision, content version, format version, integrity metadata, and a clear point-in-time boundary. Add only inexpensive identity/version metadata now. Defer synchronization queues, conflict UI, and transport selection until cloud saves are authorized.

For divergent offline progress, the safe default is separate branches or an explicit choice of a complete history—not last-write-wins merging of individual NPC locations, memories, or tasks. The cloud remains a replica. Loss of connectivity, expired cloud credentials, or service downtime must not prevent local launch or local gameplay.

## 8. Native presentation without simulation leakage

### 8.1 A thin but capable SwiftUI client

Use SwiftUI for the application structure and primary screen. Keep a replaceable rendering/composer boundary so a native UIKit-backed text or scrolling component can be introduced if measured behavior demands it. That is an implementation detail inside a SwiftUI application, not permission to rebuild gameplay in a web view.

A presentation reducer or equivalent state transformation consumes semantic events and consistent read models. It must be usable with fixtures in Phase 1 and the real engine in Phase 2. Do not spread ad hoc engine subscriptions, network callbacks, and derived world state across individual views.

The only persistent gameplay regions remain the compact status header, transcript, and composer. Temporary completion, clarification, retry, and “New text” controls belong to those interactions. Optional later features first use text queries or contextual presentation; a permanent tab bar is not the assumed destination. [P1]

### 8.2 Transcript rendering and streaming

Give each visible transcript item stable identity. Incremental output updates an existing provisional item rather than repeatedly replacing the entire transcript collection. Separate immutable history from the small active tail so long sessions do not cause full-history layout work on every token.

Follow new output only while the player is intentionally following the bottom. Once the user scrolls upward, retain their anchor and offer a “New text” affordance. Account for changes in keyboard height and Dynamic Type without treating every geometry update as a command to scroll to the end.

The renderer needs explicit styles for player intent, interpretation receipts, NPC speech, narration, deterministic facts, scene transitions, errors, and interrupted output. Typography and spacing communicate these roles; the underlying distinction must not depend on font choice or color. Do not parse provider-specific response syntax in the view layer.

Partial output is visibly part of an active attempt. If it is canceled or rejected, mark it as incomplete/not applied, or replace it with a clear interrupted-state representation while retaining the command. Never silently turn an unfinished NPC promise into canonical memory. Persist final accepted wording so restoration does not regenerate a different past.

### 8.3 Composer and completion

Maintain native multiline editing, selection, dictation, input-method composition, copy/paste, predictable focus, and normal keyboard behavior. Do not clear an unaccepted command. Do not discard text typed while the previous request is still completing.

Slash-command help and completion should derive from the same capability registry used by the runtime, with a fixture equivalent in Phase 1. Initial commands remain /look, /people, /exits, and /help; expose only implemented behavior. When deterministic travel is introduced in Phase 3, choose and document its discoverable syntax rather than depending on an LLM to make movement possible offline.

NPC completion passes an entity reference plus a display label. The engine remains responsible for availability and final resolution. A name tapped in old history may refer to someone no longer present; insertion into the composer is not authorization to converse remotely. Handle accents, aliases, duplicate names, and ordinary natural phrasing without using labels as primary keys.

### 8.4 Accessibility is a contract property

Preserve semantic speaker labels, scene headings, action availability, and error meaning for VoiceOver. Do not announce every streaming token or unexpectedly move accessibility focus. Prefer coherent announcements at message or status boundaries, with the full transcript still navigable.

Dynamic Type must not hide Send/Stop, break completion, or make the draft unusable. Light/dark appearance and contrast must work without changing semantics. Physical-device validation starts in Phase 1 and continues throughout development; Phase 4 expands the reliability matrix rather than introducing accessibility for the first time. [P1]

## 9. Rust integration, concurrency, and platform services

### 9.1 Prove the actual iOS build path early

The Rust toolchain documents separate ARM64 device and ARM64 simulator targets. Build and link the real portable engine for both; a macOS test build is not evidence of iOS compatibility. Verify native dependencies, target configuration, symbol export, linking, signing, and startup on a physical iPhone. [T1]

Prefer a reproducibly generated XCFramework or equivalent supported binary integration with separately built device and simulator variants. Apple supports distributing XCFramework binaries through Swift packages. This is a packaging choice, not a requirement to publish an independent SDK. Keep generated bindings and the Rust binary from the same build revision. [T2]

Choose a minimum iOS version and supported physical-device set during Phase 1 based on required interactions and actual test access. Do not derive the application's minimum version from the lowest version a Rust target can theoretically compile for. Pin the Xcode, Swift, Rust, and binding-tool versions used by CI.

### 9.2 Binding strategy

Evaluate UniFFI as the preferred starting candidate for generated Swift bindings, but make the decision through a small integration spike. Its documentation describes Swift type/error mappings and also notes Swift concurrency limitations that must be checked against the selected toolchain. Treat successful code generation as the beginning of validation, not the end. [T3]

The spike must exercise asynchronous completion, cancellation, errors, Unicode, repeated session creation/disposal, large but bounded event batches, and callback delivery under the intended isolation model. A minimal C ABI is an acceptable fallback if generated bindings add unacceptable constraints. Do not spread whichever choice wins throughout gameplay and UI code.

Never allow a Rust panic to unwind unsafely across a foreign boundary. Use fallible operations for recoverable conditions, establish explicit panic handling at the boundary where supported, and rely on durable recovery if an unrecoverable failure terminates the process. Do not claim arbitrary internal corruption is recoverable merely because an error can be caught.

### 9.3 Transport ownership

The engine owns inference decisions and the logical request/response contract. An iOS platform adapter may own HTTP streaming, lifecycle cancellation, TLS handling, and credential retrieval. URLSession exposes incremental asynchronous byte delivery suitable for consuming a framed response. Keeping these platform mechanics in Swift does not move game orchestration out of Rust. [T7]

If an existing Rust transport is retained, apply the same separation and prove its iOS cancellation/lifecycle behavior. Do not add both a Rust provider SDK path and a Swift provider SDK path. The production destination remains Parish Endpoints either way.

Use one documented concurrency model. Avoid a task/thread per NPC, synchronous callback chains into SwiftUI, and several independent async runtimes created accidentally by dependencies. Bound queues and make session shutdown cancel and drain outstanding callbacks before releasing associated state.

## 10. Parish Endpoints: the required remote contract

### 10.1 Responsibility split

The on-device engine decides whether inference is needed, the inference role, which context is permitted, the game-specific interpretation of output, and whether resulting changes may commit. It constructs the logical prompt/context for that role. Parish Endpoints owns the provider call, credentials, provider/model routing, and the remote execution boundary. [P1]

Published Endpoint definitions may hold reusable private instructions and provider-formatting templates, but must not independently redefine game rules or receive an entire save merely for convenience. Define the split once: the engine supplies authoritative role-specific facts and constraints; the Endpoint applies a versioned inference definition. Avoid contradictory copies of the same rule in client code, client prompt text, and Endpoint templates.

Start with one real Rundale Endpoint integration in Phase 2. Later roles such as interpretation or memory extraction may use additional published definitions or a bounded role contract. Do not create a generic mobile-accessible “arbitrary prompt to any model” proxy that defeats server-side authorization and cost controls.

### 10.2 Minimum request and result semantics

The agreement needs an input/output contract version, published Endpoint version or compatible alias policy, logical request/attempt correlation, bounded input context, output limits, and cancellation/deadline behavior. The engine retains the base world revision locally; include only an opaque correlation value remotely when needed, not an assumption that the Endpoint can validate the world.

A successful final response includes validated role output, resolved Endpoint version, and sufficient invocation metadata for diagnosis. Provider/model identification and usage metadata belong in diagnostics where available, not in gameplay prose. Immutable Endpoint versions and deployment aliases already appear in the Parish architecture; record the resolved version so an alias change does not erase reproducibility. [P2]

Initial release behavior should prefer a pinned compatible Endpoint version or a rigorously compatibility-tested alias. Retrying one invocation must not silently resolve to a new behavior version. Rollback of provider/prompt behavior must not require rewriting player saves.

### 10.3 Streaming is not final validation

Use a documented framed stream that separates progress and text deltas from the final validated result and terminal error. SSE or another bounded HTTP streaming format can work; the semantic contract matters more than the wire syntax. Test it through the actual hosting/proxy path, including buffering and disconnect behavior.

Stream only renderable text and safe progress information. Do not expose raw partial JSON as game narration or execute partially received structured actions. The terminal result is the sole candidate for gameplay validation. The Endpoint's schema validation does not replace the engine's checks of presence, knowledge, task preconditions, or permitted state transitions.

Specify how output limits, malformed frames, missing terminal frames, duplicate frames, validation failure after partial text, and provider interruptions are represented. Automatic model repair/retry must not splice output from two different attempts into one apparently continuous NPC statement. Once text has streamed, a restarted attempt must be distinguishable.

Before Phase 2 acceptance, demonstrate real incremental delivery through Parish Endpoints. Simulating a typewriter effect after receiving a complete response does not prove the required streaming integration.

### 10.4 Authentication and secret handling

No provider credential or reusable shared Parish invocation key may be embedded in a distributed iOS binary, resource file, or remotely fetched public configuration. Storing a shared secret in Keychain after shipping it inside the app does not make that original distribution safe. [P1, P2]

The documented direction for client-safe Parish invocation is trusted-issuer token validation. For an interactive native login, use a standards-based public-client flow with an external authorization agent and Authorization Code + PKCE. Send an access token intended for the Endpoint audience; do not substitute an arbitrary identity token for API authorization. Validate issuer, signature, audience, expiry, and authorization claims server-side. [P2, T8]

Store acquired tokens in appropriate platform credential storage and keep them out of save exports, logs, fixtures, and source control. The exact identity provider and token acquisition experience are decisions to close before a distributed inference-enabled build. Owner-only development provisioning may be separate, but no secret-bearing development path may leak into a shipped app.

Local launch, local saves, deterministic inspection, and local navigation must not depend on successful authentication. The initial screen still opens directly into play. Any required remote authorization should be scoped to accessing inference rather than becoming a new requirement for a local game server or an always-online startup sequence.

### 10.5 Retries, quotas, and cost controls

Set finite time, context, output, and retry budgets. Distinguish a transport retry from a new inference attempt; prevent retries at the client, Endpoint, and provider-adapter layers from multiplying independently. The Endpoint owns provider-level policy within an agreed overall limit; the engine owns whether the gameplay request is retried.

Persist correlation and remote idempotency information before starting the call when recovery depends on it. The Parish architecture contemplates bounded result reuse by idempotency key and request fingerprint, but does not guarantee it exists. Do not design Phase 2 around unverified result lookup, stream resumption, or remote cancellation features. Where unavailable, interruption must still leave a coherent local game. [P2]

Enforce per-principal authorization, rate limits, concurrency limits, and output ceilings at Parish Endpoints. Client limits improve UX but are not an abuse boundary. Cancellation may stop local work before a provider stops billing; measure canceled/failed usage rather than promising cost-free Stop.

Avoid whole-world and whole-transcript prompts. Build context from the current scene, the addressed NPC's permitted knowledge, relevant relationships and memories, a bounded recent exchange, and the current action. Cache only when the role, content, Endpoint version, and relevant state/context fingerprint make reuse valid. Stale cached dialogue must never revive obsolete world facts.

## 11. World content, time, and living-world behavior

### 11.1 Separate definitions from instances

Represent locations, NPC identities, relationships, allowed connections, schedules, authored facts, and task definitions as versioned content. Mutable position, acquired knowledge, task progress, and player-specific memories belong to a game instance. The content loader establishes initial state; loading a save must not rerun initialization and overwrite progress.

Use stable IDs and explicit references. Validate uniqueness, referenced entity existence, navigable graph connections, schedule destinations, task dependencies, and knowledge sources before starting a game. Give authors useful errors that identify the content record and violated rule. The exact source format can follow existing well-supported tooling; an elaborate content DSL is unnecessary.

Phase 2's one-location/one-NPC world and Phase 3's three-location/three-NPC world should both use this model. Small data sets are test fixtures, not hard-coded cardinality assumptions in the simulation or UI. Keep the canonical tiny world available after production content grows.

### 11.2 The canonical world sheet is an oracle, not another database

Maintain the concise human-readable world sheet required by the product spec. It should explain expected initial state and the intended transitions that tests demonstrate. A generated factual listing from the content bundle can help keep IDs and locations synchronized, but expected outcomes also need independent review; deriving both the implementation and every assertion from the same erroneous source proves little.

Finalize actual canonical content before Phase 3. The specification's illustrative sheet mentions “fields” outside its example three-node topology and gives Peig potentially inconsistent initial/morning placement. Treat these as examples to reconcile, not authorization to add an unplanned fourth location or accept contradictory presence. Document exactly three locations, three NPCs, three authored relationships, starting knowledge, and the intended schedule/weather precedence. [P1]

Do not expand biographies, family trees, geography, lore, or quests to make the fixture feel realistic. Each initial content record must help prove a specific behavior.

### 11.3 Explicit game time

Use a simulation clock distinct from wall-clock time and network timeout clocks. Recommended initial policy: observation/help does not advance game time; completed gameplay actions advance it according to explicit local rules; failed or canceled uncommitted actions do not. Reading, typing, waiting for inference, and background suspension do not make NPCs leave or trigger unseen gossip.

Persist game time, schedule progress, and the randomness state or recorded random outcomes needed for reproducibility. Use stable tie-breaking for simultaneous scheduled events. Avoid dependence on hash iteration order or platform-local time zones. A seed alone is not sufficient for historical replay across changes to algorithms or content; keep versions and committed outcomes/checkpoints.

Choose a consistent turn ordering. A practical default is to validate the action against its start state, apply the accepted outcome, advance game time, then process due scheduled/weather transitions in a documented order before publishing the next current-state projection. Dialogue describes the interaction at its defined time; subsequent departure or weather changes appear as separate ordered consequences.

Long actions and multi-step travel may later require intermediate simulation boundaries. They should extend explicit action/time semantics rather than introduce an unrelated wall-clock loop. Do not implement continuous background simulation now to reserve that possibility.

### 11.4 Schedules and weather

Use one deterministic scheduling mechanism, not a timer or async agent per NPC. An NPC's presence is a query over authoritative state; schedules and weather rules change that state through normal validated transitions. Weather is simulated game data, not a new external weather-service dependency.

Define precedence when a scheduled destination conflicts with a weather-dependent behavior. For example, a shelter rule may override a routine destination under the authored condition. Whichever rule is selected must be visible in the canonical sheet and reproducible without a model call.

Phase 3 establishes simple scheduled movement, including the initial per-NPC schedules. Phase 5 adds or demonstrates the specific living-world proof cases; it is not an excuse to defer basic time/presence consistency until then.

### 11.5 Memory, knowledge, and gossip

Distinguish world facts, NPC beliefs/knowledge, statements made by the player, and text the player has seen. A statement such as “the bridge is closed” is not automatically true because it appeared in conversation. Record source, acquisition event/time, relevant participants, and whether a proposition is authored truth, observation, or a reported claim.

For the first gossip case, use an authored fact ID with a known source and a single intended propagation path. Before propagation, the receiving NPC lacks that knowledge. After the legitimate transition, the knowledge record includes provenance. Test the absence as carefully as the presence; showing an NPC knows something is insufficient if every NPC knew it from the start.

A remembered interaction is a committed domain record linked to a completed request. Interrupted or rejected output must not be remembered as a completed conversation. Summaries and retrieval indexes are derived aids; they must not replace the only authoritative record of what was learned or permit an NPC to recall another branch's future.

Construct prompts from the addressed NPC's permitted perspective. Do not send the complete omniscient world state or the player's entire transcript to every NPC and ask the model to ignore what it should not know. Relationship-specific willingness to share a fact is separate from possession of the fact.

### 11.6 Tasks and generated behavior

The first task has an authored definition and authoritative states for assignment, relevant progress, and completion. The engine checks completion against world actions and preconditions. A model's statement that a task is complete does not complete it.

LLM output may propose a bounded memory addition, dialogue act, or permitted game action, but the engine validates it against an allowlist and current state. Never accept generated database operations, arbitrary code, unrestricted entity creation, or implicit changes hidden inside prose.

Use deterministic text for exact facts where practical, and grounded generation for expression. Structured validation cannot guarantee that every natural-language sentence is semantically correct. Measure dialogue grounding separately, reject known contradictory structured claims, and make regressions visible rather than treating successful JSON parsing as proof of believable behavior.

## 12. iOS lifecycle, recovery, and operating limits

### 12.1 Suspend safely rather than depend on running forever

Apple documents finite background execution opportunities and expiration handling. Ordinary streaming data tasks are not a guarantee of continued execution after suspension; URLSession background support does not turn an active conversational stream into a permanently running process. [T9]

The recommended initial policy is to stop beginning new inference when the app backgrounds, finish only bounded critical persistence work when the OS permits, and interrupt an uncommitted active turn safely. On return, show the saved outcome or an explicit interrupted request with retry. A later ability to recover a completed remote result can improve this without changing local commit rules.

Do not rely on a final lifecycle callback to save everything. Persist each accepted request and completed state-changing action at its defined boundary. Background execution is an optimization for orderly cleanup, never the sole mechanism preventing save loss.

### 12.2 Storage and credential unavailability

Test locked-device/file-protection conditions, insufficient storage, corrupt or unsupported saves, interrupted migrations, and expired credentials. An inability to open a protected file is not evidence that there is no save. An inference-authentication failure is not a reason to discard a local session.

Choose file protection and credential accessibility deliberately with the lifecycle policy, keeping tokens separate from game saves. Logs must not include bearer tokens, provider credentials, or raw player conversations by default. Any diagnostic export containing text requires deliberate user/developer action and an explicit retention policy; it is not an always-on telemetry feature.

### 12.3 Performance and cost budgets

Set initial measurable budgets during Phase 1 and calibrate them on supported devices. Suggested starting targets—not measured claims—are immediate visible submission/Stop feedback within roughly 100 ms, ordinary local query/commit completion within roughly 250 ms for the tiny world, and responsive typing/scrolling while incremental output arrives. Measure durable acceptance separately from optimistic visual feedback.

Use synthetic long histories, for example 10,000 to 50,000 transcript items, to expose full-history rendering and unbounded loading early. These are stress fixtures, not a promise of a particular production retention limit. Track memory, launch/resume latency, event-buffer growth, storage growth, and time spent on the main thread; choose device-specific acceptance budgets before Phase 4 sign-off.

Inference first-token and completion latency are external measurements, not guarantees controlled solely by the client. Record them along with input/output usage, retries, and canceled attempts. Phase 6 may introduce priority-based cognition or lower-detail simulation for distant NPCs, but first prove a measured cost/performance problem. Reduce unnecessary inference before adding infrastructure.

## 13. Testing, diagnostics, and release discipline

### 13.1 A layered test system

Pure engine tests validate movement, presence, game time, schedules, weather, knowledge, tasks, and deterministic outcome rules using an injected clock/randomness source and a tiny world.

Request/persistence tests inject failures around acceptance, inference completion, validation, commit, migration, and recovery. Assert that authoritative effects occur at most once and that every visible completed action has durable backing.

Contract tests check Swift/Rust value compatibility, event ordering, snapshot/cursor behavior, bounded buffers, and Endpoint schemas. The same semantic fixtures must drive the Phase 1 renderer and the Phase 2 integration tests.

UI tests and physical-device checks exercise keyboard/focus, history recall, completion, scrolling while streaming, safe areas, Dynamic Type, VoiceOver, relaunch, and interruptions. Simulator results complement but do not replace the physical-iPhone gates in the product spec. [P1]

Live inference evaluations are a small separate suite for connectivity, real streaming, actual authorization, output validity, grounding, and model behavior. They cannot be the sole regression tests for save safety or UI correctness.

### 13.2 Cross-phase invariant suite

Maintain durable fixtures for these properties: duplicate delivery does not duplicate a command or effect; a stopped obsolete attempt cannot commit; a final success is restorable after immediate termination; an unknown save version is not silently reset; header and world presence agree; clarification does not act early; the same local world operations work offline; an NPC cannot gain knowledge from a failed turn; task completion follows authoritative actions; replay never calls a provider; unsupported Endpoint output cannot mutate the save; and content changes do not silently retarget stable IDs.

Add generated/property-based request sequences where useful, especially for interleavings of retry, Stop, resume, and duplicate messages. Fault injection should target deterministic boundaries, not depend only on manually timing force-quits.

A test must assert domain state, not only that a plausible sentence appeared. Likewise, a state assertion alone does not establish that the mobile interaction was understandable. Keep these two kinds of acceptance evidence separate.

### 13.3 Diagnostics without a debug-first product

Use structured diagnostic events correlated by session, logical request, attempt, Endpoint invocation, and state revision. Record durations, terminal status, validation reasons, version identifiers, and resource/cost metrics. Keep developer traces separate from player-visible transcript events.

Provide headless inspection of current state and the canonical world sheet for developers. Capture enough information to reproduce a fault using mock inference or a recorded validated result. Do not require a player-facing diagnostics panel, provider settings screen, or manual log review for normal play.

### 13.4 Build and deployment controls

Use reproducible builds with lockfiles and pinned toolchains. Continuous integration should include headless Rust tests, content validation, contract fixtures, a clean iOS simulator build/test run, and a real iOS device-target build. Physical-device acceptance remains a recorded human/test-device gate rather than an automated claim without evidence.

Keep the iOS app, portable engine, content bundle, and published Endpoint behavior identifiable in each release. Test compatibility before promoting an Endpoint alias. Record the actual resolved version for live invocations. Do not change model behavior, save format, and content semantics simultaneously without a migration/rollback plan that can isolate the cause of a regression.

Keep signing credentials and service secrets out of source and build artifacts. Owner-only development and distributed/TestFlight builds must have explicit credential and environment policies. Do not make an unsigned simulator success the release gate for native integration.

## 14. All-phase technical delivery vision

The six phases below retain the scope and order of the product spec. Each phase adds the architecture needed for its own acceptance and establishes the minimum durable boundary needed by later work. They are not six opportunities to rebuild the application. All existing product checklists remain applicable. [P1]

### Phase 1 — Static native interaction prototype

Player outcome. A fixture-only SwiftUI screen proves the reading-and-typing interaction on a physical iPhone: header, transcript, composer, streaming, Stop, history recall, temporary completion, and accessibility. There is no live LLM and no embedded Parish runtime in the delivered prototype.

Technical work now. Define the first semantic event/projection contract and stable visible-item identity. Build a renderer/presentation state layer that accepts a fixture session through the same boundary the real session will later implement. Include fixtures for command interpretation, clarification, failed response, interrupted partial output, scene transition, restoration, and long history—not only a successful NPC greeting.

Establish composer state ownership, acceptance-versus-optimistic-display semantics, scroll-follow behavior, and accessibility structure. Prove the behavior at large text sizes and on a small supported screen. Choose the initial iOS/toolchain/device test baseline and establish repeatable builds. A saved fixture/draft demonstration may exercise restoration without introducing engine persistence.

Automated verification foundation. Phase 1 must establish a repository-level verification entry point that a coding agent can run without interpretation, preferably ./verify with a phase selector such as ./verify --phase 1. The exact implementation may be a small script or task runner, but the command is the stable developer/agent contract. It must build the relevant targets, run all deterministic Phase 1 tests, return a nonzero exit status on any required failure, and emit both a concise human summary and machine-readable results such as JSON plus JUnit-compatible output. The report must identify passed, failed, skipped, unavailable, and not-automatable gates separately; a missing simulator or other infrastructure problem must never be reported as a passing test.

The Phase 1 automated suite should exercise the presentation contract with deterministic semantic fixtures rather than bespoke UI mocks. It should include XCTest/XCUITest or equivalent automation for submission and composer clearing only after acceptance, multiline editing, history recall, command/NPC completion, Send/Stop state changes, clarification selection, interrupted and failed responses, restoration of a saved draft/fixture session, scroll-follow versus reading history, the “New text” affordance, long-history behavior, Dynamic Type configurations, accessibility identifiers/semantics, and repeated deterministic stream chunks including Unicode boundaries. Use controllable clocks, fixture stream schedules, and explicit failure/cancellation triggers so race-sensitive behavior can be reproduced without sleeps or manual timing. Screenshot or visual-diff tests may supplement these checks but must not be the sole assertion of interaction correctness.

Agent completion rule. A coding task that affects Phase 1 is not complete until the applicable automated verification command passes. Codex or another coding agent may add tests as part of implementation, but it may not make a task pass by deleting, skipping, quarantining, weakening, broadening tolerances in, or rewriting an existing test, fixture, invariant, performance threshold, or acceptance check unless the task explicitly changes the requirement that the check represents. Legitimate test changes must state the requirement change and preserve equivalent or stronger coverage. Physical-device usability, VoiceOver judgment, and other explicitly human/real-device acceptance gates remain separate; automation should prepare a reproducible checklist/report for them but must not mark them passed without recorded evidence.

Protect later phases. Do not use one giant attributed string or a flat array of anonymous text messages as the only model. Do not bake NPC names, locations, provider response shapes, or future task logic into views. The fixture adapter may simulate outcomes, but it must not become a second game engine.

Deliberately deferred. Rust integration, real inference, production save storage, simulation, full NPC content, cloud synchronization, maps, portraits, secondary navigation, and legacy feature parity.

Exit evidence. The product's physical-device interaction gate passes; fixture playback is deterministic; new output does not steal scroll position; canceled/error states are intelligible; and replacing the fixture session with a real adapter does not require redesigning the presentation model. Any hard native text/scrolling limitation is resolved before engine complexity obscures it.

### Phase 2 — Embedded Rust vertical slice

Player outcome. Exactly one location and one interactive NPC form a real local game. The player can start/resume, inspect with /look, converse through a real Parish Endpoint, stop/retry, quit, and continue from correct local state.

Technical work now. Inspect the existing Parish code and isolate the smallest portable runtime without rewriting unrelated engine systems. Complete the Swift/Rust binding and device/simulator build spike. Establish one mutation authority, accepted-request persistence, atomic outcome commit, durable transcript IDs, and the distinction between logical request and execution attempt.

Implement one real Endpoint role with versioned inputs/outputs, authentic incremental streaming, bounded context, error handling, and safe credential acquisition. Resolve the Parish streaming/authentication dependency; do not bypass it. Implement the preferred transactional save path or document why an existing mechanism provides equivalent guarantees. Establish content/save versioning even though the data set is tiny.

Protect later phases. The first NPC already has a stable identity and an authored definition separate from mutable state. The engine API is not “send message to the only NPC.” The turn model can stage a future memory/task update without changing the UI contract. A successful completed turn persists before completion is reported. One-request deduplication is a real guarantee, not an assumption about users pressing Send only once.

Deliberately deferred. Additional NPCs/locations, broad natural-language action coverage, gossip/task gameplay, cloud saves, continuous simulation, multiple simultaneous turns, and a generalized Endpoint platform roadmap.

Automated verification. Extend the Phase 1 verification entry point so a coding agent can run the complete deterministic Phase 2 suite with one command, preferably ./verify --phase 2 with ./verify remaining the repository-wide superset. The command must build the portable Rust engine for test, run headless engine and persistence tests, run Swift/Rust binding contract tests, build the iOS device and simulator artifacts, run simulator XCTest/XCUITest suites, validate content and Endpoint schemas, and produce both a concise human summary and machine-readable results such as JSON plus JUnit-compatible output. A nonzero exit status must mean the automated gate failed; partial execution, skipped required suites, unavailable dependencies, and infrastructure failures must be represented distinctly rather than reported as success.

Phase 2 fault automation. Provide deterministic injection points around durable acceptance, inference start, each stream stage, validation, gameplay commit, persistence, callback delivery, cancellation, and restoration. Automated cases must cover duplicate submissions/events, Stop racing the terminal response, a late obsolete callback, disconnect during streaming, malformed or truncated stream frames, validation failure after provisional text, force-termination after acceptance but before commit, force-termination immediately after commit but before UI acknowledgment, storage/write failure, failed migration fixtures when applicable, and retry after interruption. Assertions must inspect authoritative domain state and durable records, not merely the visible transcript, and must prove that an accepted/committed logical request produces at most one committed effect.

Inference testing must be split. The default verification gate uses deterministic Endpoint doubles, recorded validated results, and protocol fixtures so correctness does not depend on network availability, provider nondeterminism, latency, or model quality. A separate opt-in live-integration suite exercises real Parish authentication, real incremental streaming through the deployed path, schema compatibility, cancellation behavior available from the service, and basic grounding/output validity. Live inference may block the explicit Phase 2 integration acceptance gate when required by the product spec, but it must not replace deterministic regression coverage or become the oracle for persistence and game-state correctness.

Agent completion rule. Codex or another coding agent may use the verification output as its primary completion contract, but it may not make a task pass by deleting, skipping, quarantining, weakening, broadening tolerances in, or rewriting an existing test, fixture, invariant, fault case, performance threshold, or acceptance check unless the task itself explicitly changes the underlying requirement. Any legitimate test change must accompany the production change, state which requirement changed, and preserve equivalent or stronger coverage. The verification report must identify skipped tests and distinguish agent-executable checks from physical-device or qualitative gates that remain unverified.

Exit evidence. A physical device proves real local runtime execution and real Parish streaming. Tests cover Stop/final-result races, storage failure, force-quit after commit, interruption before commit, retry without duplicate effects, and /look without network. Shipping artifacts contain no provider/shared invocation secrets. The fixture renderer still works unchanged.

### Phase 3 — Tiny world navigation

Player outcome. Exactly three locations and three NPCs make geography, presence, schedules, and ambiguous references understandable. /look, /people, and /exits use local truth; deterministic and ordinary natural-language travel work through the same validated action path.

Technical work now. Finalize the canonical content bundle and world sheet, including three authored relationships, initial knowledge, explicit connections, and simple schedules. Validate every reference and reject the illustrative inconsistencies rather than inheriting them. Introduce explicit action time, stable schedule ordering, presence checks, and graph-based movement.

Resolve natural-language actions into a bounded local intent model. Deterministic commands bypass inference; unfamiliar natural phrasing may use the approved inference boundary, but cannot invent destinations or people. Show interpretation receipts when useful. Persist clarification requests and correlate selected choices with the original command. Revalidate availability at execution.

Protect later phases. Movement, scheduled transitions, and future weather/task effects use the same commit lane. The current header and location transcript landmark reflect the same committed outcome. IDs—not names or screen positions—identify actors and destinations. The world loader handles a small graph as data instead of exposing a three-room special case.

Deliberately deferred. More content, a graphical map, distant-NPC conversation, elaborate schedules, automatic offline time catch-up, and an unconstrained natural-language planner.

Exit evidence. All three places and NPCs can be accounted for against the sheet. At least one scheduled movement is observed; unavailable NPC interaction is rejected coherently; ambiguous commands wait for clarification; and movement/presence/schedule state survive resume. Offline deterministic traversal covers the complete tiny graph. An attempted move never updates location twice.

### Phase 4 — Mobile reliability

Player outcome. The existing tiny game behaves dependably through app switching, disconnection, large transcripts, accessibility settings, and relaunch. Gameplay breadth is frozen.

Technical work now. Expand lifecycle and fault-injection tests around every durable boundary. Close the exact policy for background inference, expired credentials, file protection, interrupted migration, disk pressure, and uncommitted stream restoration. Test event deduplication and snapshot/subscription reconciliation rather than only ordinary launch/resume.

Measure rendering, bounded loading, main-thread work, save latency, and memory growth on the supported devices. Make long-history paging and current-tail updates predictable. Harden dictation, keyboard resizing/dismissal, focus restoration, scroll anchors, and VoiceOver announcements. Make retry errors distinguish a failed uncommitted request from repeating a successful action.

Protect later phases. Freeze a documented minimum mobile save compatibility policy and preserve prior-format fixtures. Establish repeatable diagnostics that can explain lost/duplicate-looking requests without exposing secrets. Keep inference and storage failure independent: loss of remote service must not make local data inaccessible.

Deliberately deferred. New task systems, maps, art, inventory breadth, save-management UI, additional NPCs, cloud sync, and speculative optimizations that do not address measured problems.

Exit evidence. The specified 20-minute physical-device session passes on the primary test iPhone and a small supported iPhone. Representative crashes at acceptance, streaming, validation, and commit recover correctly. Long-history/accessibility tests meet the chosen budgets. No unresolved defect that loses, duplicates, corrupts, or materially misrepresents player actions is accepted as a later-phase cleanup item.

### Phase 5 — Living-world proof

Player outcome. Within the same three-location/three-NPC world, the player can deliberately demonstrate persistent memory, one gossip propagation, one simple task, one weather-dependent behavior, and scheduled movement.

Technical work now. Add the minimum structured domain records and transitions for those proof cases. Give the gossip fact a known initial source and a single intended transmission path. Persist the first meaningful player/conversation memory with provenance. Keep authored truth distinct from NPC knowledge and player claims. Constrain prompts to each NPC's permitted perspective.

Implement authoritative task assignment/progress/completion and weather behavior through existing action, time, and commit rules. Record explainable causes: the interaction that created a memory, the contact that carried gossip, the action that fulfilled a task, or the weather condition that redirected movement. Extend the canonical sheet and independent tests with those exact cases.

Protect later phases. Knowledge is not a bag of unstructured transcript snippets. Tasks are not inferred from dialogue wording. NPC scheduling is not a new autonomous network loop. The same identity, atomicity, save, UI, and Endpoint boundaries from Phase 2 remain in use. Rejected generation cannot mutate any of these systems.

Deliberately deferred. Large memory retrieval infrastructure, vector databases, many quests, elaborate social simulation, free-form generated world facts, extra locations/NPCs, and a new UI dashboard for every mechanism.

Exit evidence. Each proof can be reproduced from a known starting fixture and explained by authoritative state. Test both “should know” and “must not yet know.” Repeat after save/resume. Canceling a conversation does not grant knowledge or complete a task. Generated dialogue demonstrates the mechanisms without becoming the only evidence that they worked.

### Phase 6 — Controlled expansion

Player outcome. Each accepted increment makes the text adventure more expressive while preserving comprehensibility and mobile reliability. There is no required destination population, feature count, or graphical interface.

Technical work now. Keep the tiny world as a permanent regression fixture. Add content in reviewed, versioned batches with explicit expected effects on graph size, schedules, knowledge, and inference cost. Introduce capability-specific schema and contract extensions only when the feature is approved. Run migration, offline, accessibility, and fault tests for every increment.

When measured scale requires it, prioritize player-relevant work, index local history/knowledge, schedule bounded NPC cognition, and reduce expensive inference for low-salience activity. Every asynchronous result still passes through the single authoritative commit lane with revision/precondition checks. Performance work must not grant hidden knowledge or change outcomes merely because an NPC was not currently visible.

Options intentionally preserved. Textual journal/status/inventory views can query domain state; undo can use checkpoints; optional cloud sync can move consistent save packages; a future client can reuse the semantic engine boundary. A map or illustration remains optional and must consume the same world projections rather than creating a competing state model. These are possible extensions, not Phase 6 commitments.

Protect earlier work. Do not restore old UI/content wholesale. Do not treat a new screen as the default answer to every feature. Do not merge divergent cloud histories automatically at the individual-field level. Do not allow a provider/prompt update to reinterpret old saves or regenerate established dialogue.

Exit evidence for each increment. Acceptance criteria and tests are written before implementation; the change has a bounded migration/rollback plan; the canonical tiny-world suite remains green; physical-device checks cover affected interactions; and measured reliability, accessibility, and cost remain acceptable. Fix or revert an increment that materially weakens these properties before starting the next one.

## 15. Decision register: deadlines and revisit triggers

Use lightweight architecture decision records for consequential choices. Each should state the decision, alternatives actually considered, evidence, migration consequences, and the event that would justify revisiting it. Do not turn every implementation detail into a governance artifact.

| Decision                                                     | Close by                                         | Recommended direction / revisit trigger                                                                                                        |
| ------------------------------------------------------------ | ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Supported iOS/device baseline                                | Phase 1 acceptance                               | Choose based on required interactions and physical test access; revisit when an actual platform capability justifies dropping support          |
| Transcript/composer implementation                           | Phase 1 acceptance                               | SwiftUI first with isolated native fallback; revisit only for demonstrated input, scrolling, or accessibility limitations                      |
| Swift/Rust bindings and packaging                            | Early Phase 2                                    | Prove generated bindings and device/simulator binaries; fall back to a narrow C ABI if the selected toolchain exposes unacceptable limitations |
| Save storage and migration boundary                          | Before Phase 2 real saves                        | Reuse sound Parish persistence or prefer one transactional SQLite boundary; no parallel authoritative stores                                   |
| Endpoint streaming and mobile authentication                 | Before Phase 2 acceptance                        | Agree and test actual capabilities; missing support is an integration blocker, not permission for direct provider calls                        |
| Inference result/stream contract                             | Before Phase 2 acceptance                        | Distinguish provisional text from validated final output and bound retries; revisit for a concrete new inference role                          |
| Action time and schedule precedence                          | Before Phase 3 acceptance                        | Local action-driven time with deterministic ordering; revisit before long actions or concurrent simulation                                     |
| Canonical content and ID policy                              | Before Phase 3 acceptance                        | Finalize exact tiny-world definitions and stable IDs; changes require content-version handling                                                 |
| Background/interruption and save support policy              | Before Phase 4 acceptance                        | Safe interruption and durable local recovery; improve remote result recovery only when the Endpoint supports it                                |
| Knowledge provenance and task transitions                    | Before Phase 5 implementation                    | Model truth, belief, claim, and progress explicitly; do not begin with prose-only state that later needs reinterpretation                      |
| Cloud sync, undo UI, richer retrieval, NPC cognition scaling | Only after the corresponding feature is approved | Preserve identities and boundaries now; choose mechanisms when requirements and measured constraints are real                                  |

These decisions should be made at the last responsible point, not all in the first coding session. The early obligation is to avoid representations that make the later choice expensive or impossible.

## 16. Repository evolution and implementation handoff

### 16.1 Preserve useful work without inheriting the old product

At the start of engine integration, inventory existing simulation, persistence, branching, inference, content loading, and diagnostics. Classify each as reusable, adaptable, or outside the mobile runtime. Verify behavior with targeted tests before deciding to replace it.

Extract or expose the portable gameplay boundary incrementally. Keep old frontend/content as reference material or a separately buildable legacy target where that is inexpensive. Do not couple the mobile milestone to deleting every old system, migrating every legacy save, or achieving a perfect repository layout. Conversely, do not let legacy dependencies dictate the new UI or silently become the mobile runtime.

The iOS app and engine should evolve in coordinated changes with versioned contracts. Parish Endpoints remains a separate product/deployable; use a documented client contract and test fixtures rather than shared database access or a dependency on its private internal implementation. [P2]

### 16.2 What an implementation agent should receive

For each phase, provide the current product requirements, this technical vision, the relevant decision records, the canonical fixtures/content, and the explicit acceptance evidence required for that phase. Implementation tasks should identify the boundary being changed, the permitted scope, the failure cases to test, and the migration implications.

An agent may choose ordinary internal types, helper functions, package layout, and libraries within these boundaries. It may not silently relax local authority, skip physical-device evidence, embed secrets, convert provisional text into state, or restore out-of-scope legacy features. A green unit-test run is not permission to mark a physical-device requirement complete.

Keep mock/fixture execution available throughout development. When a new behavior cannot be exercised without a live provider, treat that as a testability problem to resolve before adding more content. When an integration requires a missing Parish Endpoint capability, create a bounded dependency task instead of working around the agreed architecture.

### 16.3 Definition of architectural readiness

The foundation is ready for controlled growth when a developer can explain, for any visible turn: what the player submitted, what the engine interpreted, which state revision it used, whether inference was called, which attempt produced output, which facts changed, why they were valid, where they were committed, and how the same result is restored after termination.

That explanation should come from code boundaries and recorded state—not from reading a model's prose and guessing what must have happened. The player need not see this machinery. Its purpose is to make the simple transcript/composer experience dependable as the world becomes richer.

## 17. Source references

Project sources establish requirements and prior architectural direction. External references substantiate platform capabilities and constraints, not unmeasured claims about Rundale's implementation. All other architectural choices in this document are proposals with the deadlines and validation gates described above.

[P1] Product requirements. Rundale Mobile Text Adventure — Product & Technical Specification, current native Google Docs copy in Projects/Rundale. Primary source for all six phases, the mobile reset, local authority, and the Parish Endpoints requirement. Rundale Mobile Text Adventure — Product & Technical Specification

[P2] Endpoint platform direction. Parish Endpoints — Software Architecture, in Projects/Parish. Relevant sections include product boundary, original MVP non-goals, immutable versions/deployment aliases, idempotency, and client-safe end-user authentication. This is an architecture source, not proof of deployed capabilities. Parish Endpoints - Software Architecture

[T1] Rust iOS target support. The rustc book, Apple iOS targets. Device/simulator targets, SDK requirements, and build/test distinctions. Official reference (doc.rust-lang.org)

[T2] Apple binary packaging. Apple Developer Documentation, Distributing binary frameworks as Swift packages. XCFramework integration and the platform scope of packaged binaries. Official reference (developer.apple.com)

[T3] Swift/Rust binding support. Mozilla UniFFI user guide, Swift Bindings. Type/error mappings and documented Swift concurrency caveats. Official reference (mozilla.github.io)

[T4] Transaction durability. SQLite, Atomic Commit in SQLite. Atomic transaction behavior, recovery, and the assumptions on which durability depends. Official reference (sqlite.org)

[T5] Consistent save snapshots. SQLite, Online Backup API. Consistent database backup rather than copying a live file ad hoc. Official reference (sqlite.org)

[T6] WAL and save export. SQLite, Write-Ahead Logging. Journal behavior and implications for copying a database. Official reference (sqlite.org)

[T7] Incremental native transport. Apple Developer Documentation, URLSession bytes(for:delegate:). Asynchronous response-byte delivery. Official reference (developer.apple.com)

[T8] Native-app authorization. IETF RFC 8252, OAuth 2.0 for Native Apps. External authorization agents, public-client treatment, and PKCE. Official reference (www.rfc-editor.org)

[T9] Background execution limits. Apple Developer Documentation, Choosing Background Strategies for Your App; URLSessionTask. Finite execution opportunities, expiration handling, and the distinction between ordinary data tasks and background sessions. Official reference (developer.apple.com) and Official reference (developer.apple.com)
