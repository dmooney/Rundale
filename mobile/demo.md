# Phase 1 demonstration

This walkthrough demonstrates the native fixture prototype. It is not embedded
Limerick gameplay and does not use a model or a network connection. Follow the
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

The iPhone SE (3rd generation) simulator recording runs the embedded Limerick
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

## Phase 4 reliability demonstration

Use the canonical tiny world in the updated native build. Keep the
[physical acceptance matrix](../docs/test-plans/phase-4-test-cases.md) open as the
record of device work still required.

1. Travel to the Letter Office, type an unsent draft, switch apps, then relaunch.
   Show the restored destination and exact draft.
2. Start dialogue, type a different draft while it streams, and background the
   app. Return to the explicit interruption, use Retry, and show one completed
   response with the newer draft retained.
3. Repeat with connectivity lost before a response and during streaming.
   Restore connectivity and recover without restarting the game or repeating a
   committed action.
4. Read far back in a long saved transcript. Keep the passage in place during
   new output, relaunch at the same reading position, then return to newest.
5. Show the native composer and travel at accessibility text size in dark mode.
   Perform the separate VoiceOver and physical keyboard/dictation checks on device.

The automated simulator version uses `--phase3 --ui-tests --phase3-mock` and
injects network faults only at the Endpoint transport. Its screenshots and
xcresults prove that native path with deterministic responses. They do not
establish cellular connectivity, App Attest, or the two physical 20-minute sessions.

## Phase 5 living-world demonstration

Use an internal build with diagnostics enabled. Diagnostic reads are typed,
read-only Rust projections rendered as ephemeral transcript rows; they do not
enter inference, persistence, or the semantic event journal. `/setup …` is a
separately labelled internal mutation path. `/setup reset CONFIRM` is the only
reset form and every setup change records `diagnostic_override` provenance.

1. Ask Peig to remember “I grew up in Athleague.” Inspect `/debug memory Peig`,
   then Mícheál and Róisín; relaunch and inspect again.
2. Reset, inspect Róisín’s knowledge, run `/wait 1`, then inspect Róisín and
   Peig to prove the one intended gossip propagation and negative case.
3. At the Letter Office accept Peig’s sealed-letter task, explicitly take the
   letter, relaunch, travel to Connolly Cottage, and give it to present Róisín.
   Inspect `/debug tasks` after every transition and ask Peig about it later.
4. Set 09:59/Clear, wait one minute, and inspect `/debug world`; repeat from
   reset with Heavy Rain. Mícheál’s location decision must name `schedule` or
   `weather_override`, respectively.
5. From 08:58 run `/wait 2`, inspect `/people` and `/debug world`, relaunch at
   the Letter Office, and speak to Peig there.

Generated prose is supporting evidence only. Each proof requires the matching
typed diagnostic state. The deferred Phase 4 device matrix remains open; Phase
5 additionally requires these five cases on one physical iPhone.

## Phase 5 simulator video preview — 2026-09-15

Eight short recordings were captured from the actual native app on the
`Goblin2-QA-Expo-20260912-171339` iPhone 17 Pro simulator running iOS 26.5. The
app used `--ui-tests --phase3 --phase3-mock --no-auto-focus` so Endpoint output
was deterministic; Rust state, SQLite persistence, Swift/C integration, and the
rendered SwiftUI path were production code. The gameplay implementation is
commit `490795f8f`; build receipt commit `5a6caeaef` records TestFlight 0.1.0
build 8.

| Recording | Demonstration and authoritative evidence |
| --- | --- |
| `01-player-memory.mov` | Peig accepts the Athleague player claim; `/debug memory Peig` shows a typed `player_claim`, the claim survives relaunch, and Mícheál remains at `recordCount: 0`. |
| `02-gossip-propagation.mov` | Róisín starts without the authored stock fact, `/wait 1` triggers her morning contact with Mícheál, and her knowledge projection gains the sourced `heard_from_npc` record while Peig remains uninformed. |
| `03-sealed-letter-task.mov` | Peig's offer becomes `assigned`, taking the letter makes it `in_progress`, and after relaunch delivery to present Róisín makes it `completed`; Peig later acknowledges delivery. |
| `04-weather-behavior.mov` | The same 09:59 transition places Mícheál in Kilteevan under Clear weather with cause `schedule`, then keeps him at Connolly Cottage under Heavy Rain with cause `weather_override`. |
| `05-scheduled-movement.mov` | `/wait 2`, `/people`, and `/debug world` agree that Peig moved to the Letter Office; after relaunch she remains available for conversation there. |
| `06-wait-command.mov` | `/wait 15` advances the world through the shared time path, while `/wait 0` is rejected with the supported 1–1440 minute range and does not advance it. |
| `07-diagnostics-setup.mov` | The four `/debug` projections retain one state revision, setup changes are labelled `INTERNAL SETUP`, and reset is rejected until the retained draft is completed as `/setup reset CONFIRM`. |
| `08-cancellation-atomicity.mov` | A deliberately slowed memory response is stopped before its final candidate; Peig's memory remains at `recordCount: 0` both immediately and after relaunch. |

Final delivery copies are H.264 portrait `.mov` files attached inline to the
Phase 5 pull request. Local source and compressed captures remain under the
ignored `mobile/.build/phase5-demo/` directory rather than entering Git history.
These videos are simulator implementation evidence, not live-provider or
physical-iPhone acceptance. The five signed-device proofs, including force-quit
and cancellation, remain open until a physical iPhone is available.
