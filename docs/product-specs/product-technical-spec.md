# Rundale Mobile Text Adventure — Product & Technical Specification

## 1. Purpose

Rundale will reset its player experience around a mobile-first, pure-text adventure. The new client should feel less like a conventional game UI and more like a polished coding harness applied to an interactive living world: a persistent transcript, a powerful text composer, clear interpretation of player intent, streaming responses, and dependable recovery.

This is a deliberate product reset. Existing Parish engine capabilities remain available, but the new player experience will not attempt feature parity with the current UI or content set. Features and content will be reintroduced only after the smaller experience is stable, understandable, testable, and pleasant to use.

## 2. Product Principles

### 2.1 The transcript is the game

The primary interface is a chronological living transcript containing player commands, narration, NPC dialogue, movement, deterministic game output, errors, and other meaningful events.

Secondary game state should be expressed through text wherever practical rather than permanent panels.

### 2.2 Mobile first

The primary target is iPhone. The design assumes that typing on a phone is a normal, comfortable interaction rather than something to minimize.

The experience must prioritize:

- excellent keyboard behavior;
- stable scrolling while content streams;
- readable typography;
- fast one-handed interaction;
- draft preservation;
- app lifecycle recovery;
- Dynamic Type;
- VoiceOver;
- native light and dark appearance.

### 2.3 Harness behavior, not terminal aesthetics

Rundale should borrow interaction patterns from modern coding harnesses:

- explicit command submission;
- visible interpretation of requests;
- streaming output;
- clear activity state;
- stop;
- retry;
- editable command history;
- deterministic slash commands;
- strong error recovery.

It should not imitate a literal terminal. The visual style should be literary and restrained.

### 2.4 Local-first gameplay

The authoritative game runtime lives on the device.

A gameplay action that does not inherently require LLM inference must not require a network connection.

The network is used for:

1. remote LLM inference; and
2. optional future cloud save synchronization.

Local saves remain authoritative even if cloud synchronization is later added.

### 2.5 Grow only from proven foundations

The new UI and Rundale content begin deliberately small. No feature or content expansion should occur merely because the engine already supports it.

Each addition must have:

- a clear player need;
- explicit acceptance criteria;
- automated tests where practical;
- physical-device validation where appropriate;
- evidence that existing behavior remains reliable.

## 3. Scope of the Reset

The reset covers both:

1. the player-facing UI; and
2. the initial Rundale world/NPC data.

The existing Parish simulation engine is retained and adapted for portable on-device use.

The existing Rundale content and frontend remain reference material. They do not define compatibility requirements for the new experience.

## 4. Initial Player Experience

### 4.1 Primary screen

The first version has only three persistent regions.

**Compact status header**
**Displays:**

- current location;
- time of day;
- weather.

Tapping the header may reveal a concise deterministic summary of the current location, nearby people, and exits.

**Transcript**

A single vertically scrolling history containing all meaningful play.

**Example:**

```text
KILTEEVAN VILLAGE

Rain has darkened the road through the village.
Smoke hangs low above the cottages. Peig Hannigan
stands beneath the letter-office awning.

PEIG HANNIGAN

“You’ll find no dry road west today.”

> ask Peig who came through the village last night

 ↳ Talking to Peig Hannigan about last night’s travellers

PEIG HANNIGAN

“Two men passed before dawn. Neither stopped to
give a name.”
```

**Composer**

A multiline native text input anchored above the iOS keyboard.

**Initial supporting controls:**

- Send;
- Stop while a request is active;
- @ NPC targeting/completion;
- / command completion;
- command history.

There is no permanent bottom tab bar in the initial design.

### 4.2 Typography

The visual identity should come primarily from typography and spacing.

**Recommended direction:**

- literary serif for narration and dialogue;
- clean sans-serif for labels and controls;
- restrained monospace treatment for player commands and deterministic system output;
- warm, clean light appearance;
- native dark appearance;
- generous readable spacing.

**Avoid:**

- fake parchment;
- decorative borders;
- chat bubbles;
- card-heavy layouts;
- terminal-green aesthetics;
- unnecessary ornament.

### 4.3 Scene transitions

Location changes are represented as strong transcript landmarks.

```text
────────────────────────────
THE OLD STONE BRIDGE
Late morning · Rain easing
────────────────────────────
```

The transcript itself should communicate geography and progress without requiring a graphical map.

## 5. Input Model

### 5.1 Natural language is primary

Players should be able to type ordinary instructions:

```text
ask Peig about the old church
walk over to the bridge
tell Michael what Peig told me
```

### 5.2 Interpretation receipts

After submission, the client immediately displays the engine's interpretation when useful:

```text
> head over to Michael’s place

 ↳ Walking to Mícheál Connolly’s cottage
```

This makes natural-language interpretation observable instead of silently hiding mistakes.

### 5.3 Ambiguity

If an action cannot be resolved confidently, the game asks rather than guessing.

**Example:**

```text
I’m not sure which Connolly you mean.

[ Mícheál Connolly ]   [ Róisín Connolly ]
```

Temporary choice controls are appropriate for genuine ambiguity.

### 5.4 Slash commands

Slash commands provide deterministic access to game information and advanced actions.

**Initial set:**

```text
/look
/people
/exits
/help
```

**Possible later additions:**

```text
/journal
/inventory
/status
/history
/undo
```

Typing / opens completion immediately above the keyboard.

### 5.5 NPC targeting

Typing @ opens completion for relevant nearby NPCs. NPC names in the transcript may also be tappable to insert an appropriate reference into the composer.

## 6. Request Lifecycle

Every submitted request has a clear lifecycle:

1. Player submits text.
2. A PlayerCommand event is committed.
3. Parish interprets the command.
4. An interpretation receipt is emitted if appropriate.
5. If clarification is required, execution stops pending player choice.
6. Deterministic work executes locally.
7. If inference is required, Parish makes a remote inference request.
8. Output streams into the transcript.
9. State changes are committed atomically.
10. A completion event marks the request finished and eligible for persistence/synchronization.

Interrupted or failed requests must not leave partially committed world state.

## 7. Stop, Retry, and History

The client must support:

- stopping an active streaming response;
- retrying a failed request;
- preserving the original request when retrying;
- tapping a previous player command to copy it into the composer;
- editing and resubmitting copied commands;
- preserving unsent drafts across backgrounding and relaunch.

Retry semantics must be explicit so that retrying cannot accidentally duplicate an already committed gameplay action.

## 8. Transcript Behavior

Transcript correctness is a core product feature.

Requirements:

- streaming text must not cause visible scroll instability;
- if the player is at the bottom, new output follows naturally;
- if the player has scrolled upward, streaming must not pull them back to the bottom;
- new content while reading history produces a clear “New text” affordance;
- transcript restoration preserves sensible position;
- events must never duplicate after reconnect/relaunch;
- partial streaming content must be distinguishable internally from committed final content.

## 9. Minimal Semantic Event Protocol

SwiftUI should not consume arbitrary internal Parish structures. Parish exposes a small, presentation-oriented semantic event protocol.

**Initial event kinds should include concepts equivalent to:**

```text
SceneChanged
PlayerCommand
CommandInterpreted
Narration
NpcDialogue
ActionResult
ClarificationRequired
Progress
Error
ResponseCompleted
```

**Each event should carry only the fields needed by the presentation layer, such as:**

```text
event_id
request_id
game_time
kind
content
speaker
state
```

Exact serialization and FFI representation are implementation decisions, but event identity and request correlation are required.

The protocol must support:

- replay;
- deduplication;
- streaming;
- reconnect/restoration;
- deterministic UI fixtures;
- testing the Swift UI without live inference.

## 10. iOS Architecture

### 10.1 Native client

The new primary client is implemented in SwiftUI.

Reasons include direct control over:

- keyboard and focus;
- scrolling;
- Dynamic Type;
- VoiceOver;
- safe areas;
- app lifecycle;
- background/foreground transitions;
- native persistence;
- haptics;
- platform conventions.

### 10.2 Embedded Parish runtime

Parish runs on-device as compiled Rust code exposed to Swift through a deliberately small FFI boundary.

**Conceptual API:**

```text
start_game()
resume_game()
submit(text)
respond_to_clarification(choice)
cancel(request)
snapshot()
```

The Swift layer should not manipulate internal NPC, world, inference, or persistence structures directly.

### 10.3 Portable runtime boundary

The iOS build should include only Parish components required for gameplay.

Desktop/server infrastructure must not become accidental dependencies of the portable runtime.

**Examples of capabilities to keep outside the iOS runtime unless specifically needed:**

- Tauri;
- Axum server;
- desktop process launching;
- local model management;
- desktop diagnostics;
- web authentication;
- browser-specific code;
- developer-only tooling.

This portability boundary should improve the architecture of Parish generally.

## 11. Inference Architecture

Inference is the primary remote runtime dependency.

Parish remains responsible for deciding:

- whether inference is necessary;
- which inference role is required;
- prompt construction;
- structured-output validation;
- timeout/retry policy;
- interpretation of the response;
- resulting state changes.

SwiftUI should not contain game-specific LLM orchestration.

All production remote inference from the Rundale mobile client must be performed through Parish Endpoints. The iOS application must not communicate directly with model-provider APIs or contain provider credentials. Parish Endpoints are responsible for securely holding provider credentials and providing the authenticated remote inference boundary.

The on-device Parish runtime remains responsible for game-specific inference orchestration and authoritative game state. It prepares the inference request and sends only the context needed for that inference role to the Parish Endpoint. The Parish Endpoint handles the remote provider call, including provider/model routing and streaming the response back to the device. The Endpoint does not become authoritative for the game world or require the complete save state.

On-device LLM inference is explicitly out of scope for the initial reset.

## 12. Persistence

### 12.1 Local authoritative save

The game saves locally and resumes immediately.

The player should not normally need to think about saving.

Requirements:

- autosave after completed state-changing actions;
- safe recovery after app termination;
- no partially applied request state;
- durable transcript/event identity;
- preservation of unsent composer drafts;
- schema/version migration strategy.

### 12.2 Cloud synchronization

Cloud saves are optional future functionality.

If introduced, cloud storage synchronizes/replicates local saves. It does not turn the remote service into the runtime authority.

Conflict handling and multi-device branching are later design problems and should not complicate the first implementation.

### 12.3 Branching

Parish's existing branching capability may remain underneath the system, but the initial player UI should not expose a save DAG.

A future /undo or alternate-timeline feature may use branching internally without requiring players to understand the implementation.

## 13. Minimal Rundale World

The content reset begins with a world small enough for a developer or tester to understand completely.

Initial target:

- 3 locations;
- 3 NPCs;
- 3 explicitly authored NPC relationships;
- 1 shared fact capable of becoming gossip;
- 1 simple player task;
- 1 weather-sensitive behavior;
- 1 simple scheduled movement per NPC per day.

No expansion occurs until this world behaves reliably.

### 13.1 Example topology

Kilteevan Village

├── Letter Office

└── Connolly Cottage

Every connection is explicit and easy to reason about.

### 13.2 NPC definition

Each initial NPC needs only:

- identity;
- home;
- occupation/role;
- concise personality;
- relationships to the other initial NPCs;
- simple schedule;
- explicitly authored starting knowledge.

Avoid elaborate biographies until they serve proven gameplay.

### 13.3 Content exclusions

The initial world does not require:

- large family trees;
- dozens of NPCs;
- sprawling geography;
- festivals;
- mythology systems;
- complicated seasonal schedules;
- extensive lore;
- large inventories;
- numerous concurrent quests;
- generated background facts.

Existing content can later be selectively reintroduced after the foundation is proven.

## 14. Canonical World Sheet

During early development, maintain a concise human-readable representation of the entire test world.

**Example:**

```text
KILTEEVAN VILLAGE
 exits: Letter Office, Connolly Cottage
 present at 08:00: Peig

LETTER OFFICE
 exits: Kilteevan Village
 occupant: Peig

CONNOLLY COTTAGE
 exits: Kilteevan Village
 residents: Mícheál, Róisín

PEIG
 home: Letter Office
 morning: Letter Office
 knows: Mícheál, Róisín

MÍCHEÁL
 home: Connolly Cottage
 morning: fields
 afternoon: village

RÓISÍN
 home: Connolly Cottage
 morning: cottage
 afternoon: village
```

The sheet is a development oracle. Unexpected contradiction between authoritative game state and the sheet indicates either a bug or an intentional change that requires the sheet to be updated.

## 15. Initial Non-Requirements

The following existing or proposed capabilities do not define the new UI and should not be restored merely for feature parity:

- AI-generated scene art;
- NPC portraits;
- graphical world/map view;
- MapLibre player UI;
- NPC sidebar;
- emoji reactions;
- save DAG UI;
- Parish Designer;
- player-facing debug panels;
- inference-provider configuration UI;
- multiple custom visual themes;
- demo/auto-player mode;
- bug-report UI;
- rich secondary dashboards;
- broad command completion;
- desktop feature parity.

These systems may remain in the repository and may be reconsidered individually later.

## 16. Definition of Done

A milestone is Done only when:

• every applicable requirement in its checklist is satisfied;

• its Exit Criteria can be demonstrated;

• relevant automated tests pass;

• no known defect violates the Quality Gate;

• changes affecting mobile interaction have been exercised on a physical iPhone;

• persistence and recovery behavior introduced or affected by the milestone has been verified; and

• documentation and the canonical world sheet reflect the implemented behavior where applicable.

Passing tests alone does not establish Done. A milestone must produce the observable player experience described by its Exit Criteria.

## 17. Incremental Delivery Plan

Each milestone is intentionally narrow. The checklist describes required outcomes and observable behavior, not a prescribed internal implementation. A milestone is complete only when all applicable requirements are satisfied and the resulting build is stable enough to serve as the foundation for the next milestone.

### Phase-end demonstrations

Repository requirement added 2026-09-07 at the user's request: conclude every
phase with a demonstration for the user. Walk through the phase's observable
player experience in the running application, including the relevant failure
and recovery behavior. Present the build and evidence being demonstrated,
summarize verification, and identify any acceptance gates still pending.
A demonstration does not replace the phase's tests, Exit Criteria, or required
physical-iPhone validation. An interim demo may show completed implementation
while those gates remain pending, but must not be described as phase completion.

Use the [phase demo plan](phase-demo-plan.md) to prepare each demonstration.

Milestone 1 — Static native interaction prototype

Build a SwiftUI prototype using fixture data only. The purpose is to establish the fundamental iPhone reading-and-typing experience before integrating Parish.

**Requirements checklist**

- The app launches directly into the primary play screen without onboarding, configuration, or secondary navigation being required.
- The screen contains only the compact status header, transcript, and composer as persistent gameplay regions.
- The transcript clearly distinguishes narration, NPC dialogue, player commands, deterministic/system output, and scene transitions without relying on chat bubbles.
- The composer uses normal iOS text-entry behavior and remains correctly positioned when the software keyboard appears, changes size, or is dismissed.
- The composer supports multiline input without making short commands cumbersome.
- Sending text immediately produces a visible player-command entry in the transcript.
- Fixture responses can stream incrementally so the real streaming experience can be evaluated before backend integration.
- An active fixture response can be stopped from the primary screen.
- Long transcripts remain readable and responsive.
- When the player is following the newest content, streaming output remains naturally visible.
- When the player scrolls upward to read history, incoming output does not force the transcript back to the bottom.
- When new content arrives while the player is reading earlier history, the UI provides a clear way to return to the newest content.
- Previous player commands can be recalled into the composer and edited.
- @ and / affordances can be exercised with fixture completion data without requiring a real game engine.
- The layout works in portrait on supported iPhone screen sizes, including small screens.
- The interface respects iPhone safe areas.
- Light and dark appearances are both usable and intentional.
- Dynamic Type remains usable through accessibility text sizes without hiding essential controls or making the composer unusable.
- Core transcript and composer interactions are usable with VoiceOver.
- Basic focus behavior is predictable when entering text, submitting, stopping, scrolling, and returning to the composer.
- The prototype uses no live LLM and no Parish runtime; all behavior needed for this milestone is reproducible from fixtures.
- No graphical map, portrait, scene art, tab bar, NPC sidebar, save UI, debug UI, or other legacy player surface is introduced.

**Exit criteria**

A tester can spend several minutes reading, typing, submitting, stopping, recalling commands, and navigating a long simulated transcript on a physical iPhone without encountering keyboard, scrolling, readability, or accessibility problems serious enough to undermine the basic interaction model.

Milestone 2 — Embedded Rust vertical slice

Establish the Swift/Rust boundary and make the prototype into a tiny real game. This milestone deliberately supports only one location and one NPC.

**Requirements checklist**

- Parish gameplay code runs locally on the iPhone rather than through a remote Parish game server.
- SwiftUI communicates with Parish through a small, presentation-oriented boundary rather than depending directly on internal engine structures.
- The iOS gameplay runtime does not require Tauri, the web server, desktop process management, local-model launching, or other desktop-only facilities.
- A new game can be created locally.
- An existing local game can be resumed.
- The initial world contains exactly one playable location and one interactive NPC for this milestone.
- /look returns authoritative local game information through the same transcript event path used by the UI.
- The player can address the NPC using ordinary free text.
- The game makes its interpretation of the player's request visible when doing so helps the player understand what will happen.
- A real remote inference request through a Parish Endpoint can produce an NPC response.
- NPC output streams into the transcript.
- The player can stop an active inference-backed response.
- Stopping a response leaves the game in a coherent, resumable state.
- A failed inference request produces a comprehensible player-facing error without corrupting the session.
- Failed requests can be retried without unintentionally applying the same game action twice.
- Gameplay that does not inherently require inference works without network access.
- Provider credentials or other long-lived secrets are not embedded in the shipped application; production inference reaches model providers only through Parish Endpoints.
- Completed state-changing actions are persisted locally.
- Force-quitting after a completed action and relaunching restores the correct game state.
- An interrupted request cannot leave partially committed authoritative world state.
- Transcript events have durable identities sufficient to prevent duplicate display after restoration.
- Request-related events can be correlated so the UI can associate a command, its interpretation, streaming response, errors, and completion.
- The same semantic event fixtures used for UI testing can represent real Parish output.
- The one-location/one-NPC game can be played without exposing engine configuration, provider selection, debugging tools, or other developer infrastructure.

**Exit criteria**

On a physical iPhone, a tester can launch or resume the game, inspect the location, converse repeatedly with one NPC using real inference, stop or retry requests, quit the app, relaunch it, and continue from correct local state. This should already feel like a small but genuine text game rather than an engine demonstration.

Milestone 3 — Tiny world navigation

Expand to the complete canonical three-location/three-NPC test world. The purpose is to prove spatial state, NPC presence, schedules, and natural-language navigation while the entire world remains understandable by one person.

**Requirements checklist**

- The canonical test world contains exactly three locations and three NPCs at the start of this milestone.
- Every location and connection is explicitly represented in the canonical world sheet.
- Every NPC has an explicitly authored home, simple role/occupation, concise personality, starting knowledge, relationships, and schedule needed by this milestone.
- /people reports who is actually present at the player's current location.
- /exits reports the authoritative destinations currently reachable from the player's location.
- /look, /people, and /exits work without network access.
- The player can travel using deterministic commands or ordinary natural-language phrasing.
- A successful move updates authoritative player location exactly once.
- A move creates a clear scene transition in the transcript.
- After travel, the status header and subsequent transcript output agree about the current location.
- NPC presence agrees with authoritative NPC location rather than being invented by generated prose.
- NPC scheduled movement can change who is present as game time advances.
- The player cannot converse normally with an NPC who is not actually available for that interaction.
- When a natural-language destination or NPC reference is confidently resolved, the game shows an understandable interpretation receipt where useful.
- When a materially ambiguous destination or NPC reference cannot be resolved confidently, the game asks the player to choose rather than guessing.
- Clarification does not execute the underlying action until the player resolves the ambiguity.
- Choosing a clarification continues the original request without requiring the player to retype it.
- Travel, presence, and schedule state survive save/resume.
- The entire world can be inspected against the canonical world sheet during testing.
- Unexpected contradictions between the canonical sheet and authoritative runtime state are treated as defects unless the sheet is intentionally updated.
- No additional locations or NPCs are added merely to make the world feel fuller.

**Exit criteria**

A tester can understand the entire world, move among all three locations, find each NPC where expected, observe at least one scheduled movement, converse only with available NPCs, resolve ambiguous requests, and quit/resume without spatial or presence inconsistencies.

Milestone 4 — Mobile reliability

Stop feature growth and harden the existing game as a native mobile application. No new gameplay breadth is required in this milestone.

**Requirements checklist**

- The current game survives normal app backgrounding and foregrounding without losing committed state.
- An unsent composer draft survives backgrounding and restoration.
- An unsent composer draft survives ordinary app termination/relaunch where iOS restoration is reasonably expected.
- Backgrounding during an inference request has defined, tested behavior and never silently duplicates the request.
- Returning after an interrupted or completed background inference leaves the transcript and authoritative world state consistent.
- Loss of connectivity before an inference request produces a recoverable state.
- Loss of connectivity during an inference request produces a recoverable state.
- Restoring connectivity allows the player to continue without restarting the game.
- Retry behavior clearly distinguishes retrying an uncommitted request from repeating an already completed player action.
- Reconnection or restoration does not duplicate transcript events.
- Reconnection or restoration does not duplicate authoritative game actions.
- Force-quitting at representative points in the request lifecycle cannot corrupt the save.
- Long transcripts remain responsive over a realistic play session.
- Streaming remains scroll-stable in long transcripts.
- Reading earlier history remains possible while new output is streaming.
- The newest-content affordance remains understandable and reliable.
- Keyboard presentation, dismissal, rotation where supported, dictation, and common iOS text-entry behavior do not break the composer.
- Incoming interruptions such as app switching do not lose submitted or unsent text.
- The app remains usable at supported Dynamic Type sizes, including accessibility sizes.
- The core game loop remains usable with VoiceOver.
- Interactive elements have sensible accessibility labels and focus order.
- Error messages tell the player what happened and what they can do next without exposing irrelevant implementation details.
- A 20-minute physical-device play session can be completed without needing a secondary screen for normal gameplay.
- Reliability testing covers at least one small-screen supported iPhone and the primary development/test iPhone.
- No new map, art, inventory, quest, save-management, or other major gameplay UI is added during this hardening milestone.

**Exit criteria**

The existing tiny game behaves like a dependable iPhone application under realistic lifecycle, connectivity, accessibility, keyboard, scrolling, and failure conditions. Known defects that can lose, duplicate, corrupt, or substantially confuse player actions block progression to the next milestone.

Milestone 5 — Living-world proof

Add only enough simulation to demonstrate Rundale's central living-world premise inside the tiny canonical world.

**Requirements checklist**

- Exactly one initial authored fact is designated as the first gossip case.
- The fact has a known starting source so testers can determine who should and should not know it initially.
- A player interaction can cause an NPC to remember one meaningful fact about the player or conversation.
- That memory persists across save/resume.
- A later interaction can demonstrate that the NPC's response is informed by the persisted memory.
- The chosen gossip fact can propagate through one intended NPC-to-NPC path.
- Before propagation, an NPC who should not know the gossip does not behave as though they know it.
- After propagation, the newly informed NPC can demonstrate knowledge of it through normal play.
- Gossip state is authoritative game state rather than inferred solely from generated prose.
- One NPC can assign one simple, concrete task to the player.
- The task has an authoritative state that can distinguish at least assignment, progress where applicable, and completion.
- The task state survives save/resume.
- Completing the task through the intended world action is reflected in later NPC interaction.
- One NPC has one clearly defined weather-dependent behavior.
- Changing to the relevant weather condition causes the expected behavior in authoritative state.
- One NPC has one scheduled movement that can be observed by the player through location/presence changes.
- The scheduled movement remains consistent with /people, conversation availability, and save/resume.
- The five living-world proofs—memory, gossip, task, weather behavior, and scheduled movement—can each be reproduced without adding extra NPCs or locations.
- Generated dialogue is constrained by authoritative state sufficiently that it does not casually contradict who is present, what has happened, or what an NPC knows.
- Failures in one living-world mechanism are diagnosable within the tiny world rather than hidden by large amounts of content.
- The canonical world sheet is extended only with the facts needed to describe these behaviors and remains concise enough for a tester to understand as a whole.

**Exit criteria**

A tester can deliberately demonstrate all five living-world behaviors in the three-location/three-NPC world and explain why each observed outcome occurred. Save/resume and repeated tests produce coherent authoritative state, and generated dialogue does not obscure whether the underlying mechanisms worked.

Milestone 6 — Controlled expansion

Only after the previous milestones are stable may Rundale begin growing beyond the canonical tiny world. Expansion is incremental rather than a return to the previous content scale.

**Requirements checklist**

- The tiny canonical world remains available as a deterministic regression fixture even after production content expands.
- New player-facing capabilities are proposed in terms of a concrete player need or gameplay outcome rather than existing-engine feature availability.
- Every proposed capability has explicit acceptance criteria before implementation begins.
- Every proposed capability identifies how it will be tested before implementation begins.
- Content additions are introduced in small enough batches that testers can still reason about new relationships, locations, schedules, and knowledge changes.
- NPC count is increased gradually rather than restoring the previous population wholesale.
- Location count is increased gradually rather than restoring the previous world graph wholesale.
- Existing NPCs or locations are reintroduced only after their data is reviewed against the new content model and current gameplay needs.
- Legacy UI features are not restored merely because corresponding engine support exists.
- Legacy content is not assumed correct merely because it previously shipped.
- A graphical map is considered only if demonstrated player needs cannot be served well by the text-first location/exits experience.
- Art and portraits are considered optional enhancements rather than prerequisites for atmosphere or comprehension.
- New secondary screens are added only when the transcript/composer model is demonstrably inadequate for the required interaction.
- Every expansion preserves offline operation for gameplay that does not inherently require inference.
- Every expansion preserves local authoritative save behavior.
- Every expansion preserves transcript stability, request atomicity, and duplicate prevention.
- Physical-iPhone validation remains required for changes affecting core interaction.
- Accessibility remains part of acceptance rather than deferred polish.
- A feature or content batch that makes the current experience materially less reliable or understandable is fixed or reverted before further expansion.
- The project may intentionally remain text-only indefinitely; graphical feature parity is not an end-state requirement.

**Definition of Done for each expansion increment**

There is no single feature-count target for this milestone. Each expansion increment is Done only when its explicit acceptance criteria and planned tests pass, all applicable project-wide Definition of Done requirements are satisfied, and the increment leaves Rundale more useful or expressive without sacrificing the reliability, comprehensibility, mobile usability, and text-first identity established by the earlier milestones.

## 18. Quality Gate

Before adding a new player-facing capability, the current build must satisfy the relevant portions of this baseline:

- keyboard never obscures active input or critical response content;
- no submitted command is lost;
- no command is executed twice unintentionally;
- transcript does not jump while the player reads earlier content;
- streaming can be stopped safely;
- failed inference leaves coherent game state;
- relaunch restores the correct story state;
- unsent drafts survive normal lifecycle transitions;
- player can determine current location;
- player can determine who is present;
- player can determine what action the game interpreted;
- every persistent visible control has a clear purpose;
- basic play remains usable with VoiceOver and large Dynamic Type;
- a normal 20-minute play session does not require navigating secondary screens.

Passing unit tests alone is insufficient for interaction changes. Mobile UX changes require validation on a physical iPhone before being considered complete.

## 19. Testing Strategy

The reset should favor small, deterministic fixtures.

Swift UI tests

Use fixture event streams to test:

- rendering;
- streaming;
- scrolling;
- error states;
- clarification;
- restoration;
- accessibility.

No live LLM should be necessary for UI regression tests.

Parish runtime tests

Use the tiny canonical world to test:

- movement;
- presence;
- schedules;
- memory;
- gossip;
- task progression;
- persistence;
- request atomicity;
- event ordering.

Integration tests

Exercise Swift-facing Parish APIs with deterministic/mock inference.

Live inference tests

Keep a small, intentional set of tests proving the real inference path. Do not make routine UI correctness depend on nondeterministic external models.

## 20. Definition of Success

The reset succeeds when Rundale can be handed to someone on an iPhone with minimal explanation and the player can:

1. understand where they are;
2. understand who is around them;
3. type naturally;
4. understand what the game thought they meant;
5. converse with a believable NPC;
6. move through the tiny world;
7. observe that NPCs remember and act;
8. leave the app and return without losing their place;
9. continue playing without needing graphical interfaces or secondary dashboards.

The first objective is not breadth. It is to make a tiny Rundale experience feel exceptionally reliable and natural.

Only then should the world grow.
