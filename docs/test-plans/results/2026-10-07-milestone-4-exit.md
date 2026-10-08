# Spec Milestone 4 exit session

> Issue: #2193 (Stability). Recorded 2026-10-07 on the shared-engine line,
> following [the automated re-acceptance](2026-10-07-milestone-4-reacceptance.md)
> and its defect fixes (#2214, #2216). This record covers the checks that
> re-acceptance left manual, and the phase-end demonstration.

**No physical iPhone was used.** The owner waived physical-iPhone gates on
2026-10-04 (product spec §16). Every result below is a simulator result.
VoiceOver, real dictation, airplane mode, lock-screen data protection, App
Attest and Instruments budgets cannot be exercised on the simulator and are
recorded as not run.

## Environment

| Item         | Value                                                                                                                                                                                                               |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Primary size | iPhone 17 Pro simulator, iOS 26.5 ("Rundale Phase 1 Large iPhone" and a fresh clone)                                                                                                                                |
| Small size   | iPhone SE (3rd generation) simulator, iOS 26.5 ("Rundale Phase 1 Small iPhone" and a fresh clone)                                                                                                                   |
| App          | Debug build from `main` at `9e3a74e34`, with the test changes on this branch; the demonstration and the Phase 4 gates ran with PR #2217                                                                             |
| World        | Canonical `mods/rundale` on the embedded engine (`LimerickMobileFFI`)                                                                                                                                               |
| Remote calls | Scripted Endpoint transport (`--phase3-mock`), except the live network check: `limerick-prod` Endpoints (`limerick-demo`, Gemini Flash Lite) with an App Check debug token created for the session and then deleted |
| Appearance   | Light and dark; default text size and Accessibility M to XXXL                                                                                                                                                       |
| Tester       | Claude Code session, driving the simulator through XCUITest                                                                                                                                                         |

## Results

| Check                                        | Result                  | Evidence                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 20-minute play session, both sizes           | Pass (simulator)        | `RundaleSoakUITests` with `RUNDALE_SOAK_UI_TESTS=1`, default 20 minutes. Large: 1214 s, 50 travel cycles, 50 dialogue cycles, 17 background/foreground recoveries. Small: 1205 s, 50, 50, 17. No failure, no Retry left over, every reply committed                                                                                                                                                                                                                       |
| Backgrounding and foregrounding              | Pass (simulator)        | The soak's 17 recoveries per size, with the transcript scrolled into history first; P4-01, P4-02 and P4-06 in the Phase 4 gate on both sizes                                                                                                                                                                                                                                                                                                                              |
| Force-quit before acceptance                 | Pass (simulator)        | Killed the moment Send is tapped: the command is kept once, Retry is offered, no knot or Stop survives, and Retry commits one reply. Now `testForceQuitAtSendKeepsOneCommandAndOffersRetry`                                                                                                                                                                                                                                                                               |
| Force-quit during streaming                  | Pass (simulator)        | P4-03 in the Phase 4 gate                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Force-quit around final completion           | Pass (simulator)        | Killed at the last streamed chunk, and again just after the reply committed: one command and one committed reply, no Retry. Now `testForceQuitAfterTheReplyCommitsKeepsItWithoutRetry`. On the simulator the last chunk and the commit land together, so the kill at the last chunk also found the reply already committed                                                                                                                                                |
| Connection loss before inference             | Pass (simulator, live)  | A local TLS proxy in front of `limerick-prod` dropped every connection. The intent and dialogue requests both failed at the socket. The app showed "The road out of the parish is washed away..." with Retry and committed nothing. After the proxy came back online, Retry committed one live reply in the same process                                                                                                                                                  |
| Connection loss during inference             | Defect fixed (PR #2217) | The proxy cut the live dialogue stream after its first frames. Recovery was correct: Retry committed one reply, and after relaunch only that reply remained. The unapplied reply, however, looked exactly like a committed one. PR #2217 dims it and adds "Not applied". The note is hidden from VoiceOver, which already reads "Interrupted; not applied", so no UI test can see it; `UnappliedReplyTests` covers the rule and the PR #2217 recording shows it on screen |
| Keyboard                                     | Pass (simulator)        | Every check above typed through the on-screen keyboard; the keyboard suites in the Phase 1 and Phase 4 gates passed on both sizes                                                                                                                                                                                                                                                                                                                                         |
| Rotation                                     | Pass (simulator)        | Portrait only since #2216; `testTurningThePhoneKeepsThePortraitComposerAndDraft` in the Phase 4 gate on both sizes                                                                                                                                                                                                                                                                                                                                                        |
| Dictation                                    | Not run                 | The simulator cannot dictate into the app; a physical iPhone is needed (waived)                                                                                                                                                                                                                                                                                                                                                                                           |
| Dynamic Type at accessibility sizes          | Pass (simulator)        | P4-07 at Accessibility M to XXXL in dark mode, on both sizes                                                                                                                                                                                                                                                                                                                                                                                                              |
| Reduce Motion                                | Pass (simulator)        | Two captures of the waiting knot 1.1 s apart: different with Reduce Motion off, identical with it on (set through `com.apple.Accessibility ReduceMotionEnabled`). Closes the P4-14 gap                                                                                                                                                                                                                                                                                    |
| Accessibility audit                          | Findings recorded       | Xcode's `performAccessibilityAudit` after travel and one reply. Default size, light: one contrast failure and two clipped-text warnings. XXXL, dark: one contrast failure on the large size, none on the small. See [Findings](#findings)                                                                                                                                                                                                                                 |
| VoiceOver labels and focus order             | Not run                 | VoiceOver does not run on the simulator; a physical iPhone is needed (waived). Labels are covered by the existing label tests and the audit                                                                                                                                                                                                                                                                                                                               |
| Lock/unlock, App Attest, Instruments budgets | Not run                 | Not available on the simulator; physical iPhone waived                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Phase 4 gate after the fix                   | Pass (simulator)        | `just mobile-verify --phase 4` on PR #2217, large and small: 28 passed, 0 failed, with 1 skipped (simulator already booted), 1 unavailable (opt-in live Endpoint suite) and 11 not automatable (physical iPhone) on each                                                                                                                                                                                                                                                  |

The live network check is the nearest the simulator comes to losing the
network. The sockets really closed, but the operating system never reported
"no Internet connection", as airplane mode would.

## Findings

| Finding                                                                                                                                                                                                      | Blocks Milestone 5                         | Disposition                                                                           |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------ | ------------------------------------------------------------------------------------- |
| A reply cut off by a lost connection looked exactly like a committed reply. Only its VoiceOver label said "not applied", so after Retry the screen showed two different answers to one question              | Yes (misrepresents an action)              | Fixed by PR #2217                                                                     |
| At the cottage, before any introduction, "Róisín, what news...?" and "ask Róisín about..." are answered by Mícheál, without asking. A first name resolves only after an introduction; "Roisin" never matches | No (nothing lost, duplicated or corrupted) | Added to #2143, which owns recipient resolution                                       |
| The 20-minute soak had gone stale: compass travel (#2147), Peig named before her introduction, and asking her during the hours she waits on the village road                                                 | No (test only)                             | Fixed on this branch                                                                  |
| The audit reports a contrast failure on a SwiftUI node it cannot name. The theme's secondary ink measures about 6.5:1 (light) and 7.6:1 (dark) against the canvas, so the failure lies elsewhere             | No                                         | Unattributed; needs Accessibility Inspector or a VoiceOver pass                       |
| The audit's two clipped-text warnings come from transcript rows and disappear when the transcript scrolls                                                                                                    | No                                         | Recorded; P4-07 shows the composer and header stay usable at every accessibility size |
| XCUITest lists the header's clock and weather icons as images labelled "Clock" and "Brightness Higher". It also lists the "·" that the code hides, so this does not show what VoiceOver actually reads       | Unknown                                    | Check on a device during a VoiceOver pass                                             |

## Spec Milestone 4 requirements left manual by the re-acceptance

| #     | Requirement                                                 | Verdict now                                                                                                    |
| ----- | ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| 6     | Connectivity loss before a request is recoverable           | Met (simulator, live Endpoints through a dropping proxy)                                                       |
| 12    | Force-quit at representative points cannot corrupt the save | Met (simulator): before acceptance, during streaming, at the last chunk and after commit; #2210 fixed by #2214 |
| 13    | Long transcripts responsive over a realistic session        | Met (simulator) for the 20-minute session; device performance budgets waived                                   |
| 16    | Newest-content affordance understandable and reliable       | Reliable (simulator, including 17 history reads in each soak); understandability needs a player                |
| 17    | Keyboard, rotation, dictation, text entry keep the composer | Met (simulator) apart from dictation, not run                                                                  |
| 19    | Usable at accessibility Dynamic Type sizes                  | Met (simulator); audit findings above                                                                          |
| 20    | Core loop usable with VoiceOver                             | Not run (no VoiceOver on the simulator; physical iPhone waived)                                                |
| 21    | Sensible accessibility labels and focus order               | Partly met; VoiceOver focus order not run                                                                      |
| 22    | Errors say what happened and what to do next                | Met (simulator) for lost connections; the "Not applied" note (PR #2217) says what the error left behind        |
| 23    | 20-minute session without a secondary screen                | Met (simulator)                                                                                                |
| P4-14 | Knot stationary with Reduce Motion; no stale activity       | Met (simulator)                                                                                                |

**Exit criterion** (the tiny game behaves like a dependable iPhone app;
defects that can lose, duplicate, corrupt, or substantially confuse player
actions block the next milestone): **Met on the simulator.** No check found a lost, duplicated, or corrupted action. The one
misrepresentation found is fixed by PR #2217 (merged). VoiceOver, dictation, and the
device-only checks were not run, under the physical-iPhone waiver.

## Phase-end demonstration

The recording is on the evidence page of the pull request that adds this
record, linked from #2193. It is a 7-minute **simulator** demonstration on the
iPhone 17 Pro simulator (iOS 26.5), built from `main` at `219971a54` with
this branch's test changes. Each part is an XCUITest run recorded with
`TEST_RUNNER_RUNDALE_DEMO_HOLD=3`, with a title card before it:

1. A draft and a completed trip survive switching apps and a relaunch (P4-01).
2. Backgrounding during a reply interrupts it; Retry completes it once and
   keeps the newer draft (P4-02).
3. A force-quit at Send leaves one command and a Retry (new test).
4. Real connection loss against live `limerick-prod` Endpoints, before the
   response and mid-stream, with the cut reply marked "Not applied" (scratch
   probe, not committed).
5. 75 seconds of the soak: travel, dialogue, reading history, returning to
   the newest text, and backgrounding.
6. Accessibility M to XXXL in dark mode (P4-07).

Parts 1 to 3, 5 and 6 use the scripted Endpoint transport. This is not live
gameplay beyond part 4, and not a physical-iPhone session.
