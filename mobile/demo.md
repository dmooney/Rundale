# Phase 1 demonstration

This walkthrough demonstrates the native fixture prototype. It is not embedded
Parish gameplay and does not use a model or a network connection. Follow the
[phase demo plan](../docs/product-specs/phase-demo-plan.md) and report pending
[acceptance gates](acceptance.md) alongside the demo.

## Phase 1 launch

Generate the project and run the Rundale scheme on an iPhone simulator or
development device as described in [README.md](README.md), with the launch
argument `--fixture=standard`. That explicit fixture launch plays output
incrementally. Normal launch now selects the Phase 2 Rust runtime. Manual
fixture stepping is reserved for UI tests.

1. Show the compact location/time/weather header, transcript, and empty native
   composer. Open the keyboard and enter a multiline draft.
2. Send `ask Peig about the old church`. Show the interpretation and incremental
   NPC text. Type a new draft while the response is arriving; stop the response
   and show that the new draft remains.
3. Recall a prior command, edit it, and send it. Show that the old transcript
   entry remains intact.
4. Type `@` and select a fixture NPC. Type `/`, select `/look`, and show the
   deterministic-style result. Explain that these are authored UI fixtures.
5. Send `ambiguous`, choose a person, and show that the original request
   continues after clarification.
6. Send `fail`, then use Retry. The deterministic failure fixture fails again;
   demonstrate safe retry presentation without pretending that a remote service
   recovered. Stop a normal response and retry it to demonstrate a successful
   new attempt.
7. Leave an unsent draft, background/relaunch, and show the restored fixture
   transcript and draft. Repeat with an accepted response interrupted before
   completion; show its interrupted status and retry.
8. Show light/dark appearance and accessibility text sizes. Keep the separate
   physical VoiceOver and iPhone usability gates explicit.

## Long-history segment

Use the `--fixture=long-history --no-auto-focus --reset-fixture` launch arguments
for an isolated simulator demo. Read older entries, begin a response while
remaining in history, and show that the visible passage stays in place. Use
“New text” to return to the latest response. Demonstrate keyboard dismissal and
multiline composer growth while following the newest text.

Do not use `--reset-fixture` against a fixture session that should be retained.
Use the ordinary app launch for restoration demonstrations.

Capture screenshots or a recording of the actual running build when sharing
the demonstration. Identify whether the evidence came from a simulator or a
physical iPhone; neither a screenshot nor an automated pass establishes the
full milestone's physical-device acceptance.

## Recorded implementation preview — 2026-09-07

The working-tree build was demonstrated in the iPhone 17 Pro simulator on
iOS 26.5. The user viewed the running app. The capture shows native command
entry, the unmatched-command fallback, incremental `long stream` dialogue,
and a new draft retained when the response was stopped. Native autocorrection
changed the typed NPC name in the first command; use the `@` completion for
exact fixture names. This recording is an implementation preview, not a full
physical-device acceptance session.

Local captures are `mobile/.build/phase1-demo.mp4` and
`mobile/.build/phase1-demo.png`. Generated captures remain outside source
control. The walkthrough above covers the remaining demo interactions.

## Phase 2 native implementation preview — 2026-09-07

The iPhone SE (3rd generation) simulator recording runs the embedded Parish
Rust engine through the production Swift/C boundary, with SQLite persistence.
Dialogue uses the explicitly selected deterministic Endpoint test transport;
it does not establish live remote inference or physical-iPhone acceptance.

The recording shows `/look` producing the canonical local result, a `Hello`
request streaming into one Peig dialogue row, replacement by validated final
text, and the committed transcript restored after process termination and
relaunch. The Phase 2 native UI suite separately checks Stop/retry and nearby
NPC completion.

Local captures are `mobile/.build/phase2-native-demo.mp4` and
`mobile/.build/phase2-demo.png`. They are generated evidence outside source
control. To reproduce the isolated simulator preview, launch with
`--phase2 --ui-tests --phase2-mock --reset-fixture --no-auto-focus`; omit
`--reset-fixture` on relaunch to retain the SQLite session.

Live integration work follows the [Endpoint handoff](endpoint/phase2-handoff.md).
Phase 2 remains open until live Endpoint and physical-device evidence is recorded.
