# Spec Milestone 4 re-acceptance on the shared-engine line

> Issue: #2192 (Stability). Recorded 2026-10-07 at `main` revision
> `146948033`. The manual Milestone 4 checks and the phase-end demonstration
> belong to #2193 and are not recorded here.

This gives a current verdict for every spec Milestone 4 (mobile reliability)
requirement (product spec §17) on the iPhone app built from `main`, where the
app runs the shared Limerick engine through `limerick-mobile-ffi`. The Phase 4
suites and [test cases](../phase-4-test-cases.md) came over from `ios-port`;
the case map below checks that each one now drives the current app and the
shared turn engine.

**No physical-iPhone testing happened in this pass.** The owner's
physical-iPhone waiver of 2026-10-04 (product spec §16) still applies. Every
result below is a simulator or engine result, never a device result.

## Verdicts

| Verdict             | Meaning                                                                               |
| ------------------- | ------------------------------------------------------------------------------------- |
| **Met (simulator)** | An automated test in `just mobile-verify --phase 4` shows it on the iPhone simulator. |
| **Met (engine)**    | Rust tests through the shared code the phone links (`mobile` feature, FFI) show it.   |
| **Partly met**      | Automated evidence covers part of the requirement; the remainder is named.            |
| **Manual (#2193)**  | Needs a human check, which #2193 runs. Simulator sessions count under the waiver.     |
| **Defect (#N)**     | A filed Stability issue; see [Defects](#defects).                                     |

A row can carry more than one verdict: the automated part is established and
the remainder is manual or a defect.

## Evidence runs

Both runs passed with no failed or skipped required gate. Test names below
are in `mobile/RundaleUITests/`, `mobile/RundaleTests/`, and
`limerick/crates/limerick-mobile-ffi/src/tests.rs`.

| Run     | Simulator (iOS 26.5)                                       | Passed | Unavailable              | Not automatable |
| ------- | ---------------------------------------------------------- | ------ | ------------------------ | --------------- |
| Phase 4 | Rundale Phase 1 Large iPhone (iPhone 17 Pro), `--no-cache` | 30     | 1 (opt-in live Endpoint) | 11              |
| Phase 4 | Rundale Phase 1 Small iPhone (iPhone SE (3rd generation))  | 30     | 1 (opt-in live Endpoint) | 11              |

Phase 4 runs the Phase 1–3 regression suites as well. On each simulator the
UI and controller suites ran fresh: 58 Phase 1 UI tests, 39 Phase 2, 8
Phase 3, 8 Phase 4 UI, and 55 controller tests (`RundaleTests`), all passed.
The small-iPhone run reused eight device-independent suites (Rust tests, Swift
packages, the unsigned device build) from the large run with identical inputs.

Two scratch probes ran outside the gate and were then removed: two FFI tests
that interrupt new-game creation (#2210), and a UI test that rotates the
small iPhone to landscape with a draft and the keyboard up (#2211).

## Phase 4 test cases on the current app

Every Phase 4 UI test launches with `--phase3 --phase3-mock`. That route
opens the embedded engine (`LimerickRuntime.openResume`, the
`LimerickMobileFFI` xcframework) on the canonical `mods/rundale` world.
`LaunchConfiguration` swaps in a scripted Endpoint transport, the only
substitution. The retired mobile runtime
(`limerick-core/src/mobile`, `limerick-persistence/src/mobile`) no longer
exists on this line.

| Case  | Current coverage                                                                                                                                                                                                                                                                                                                 | Status                 |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| P4-01 | `testDraftAndCommittedTravelSurviveBackgroundAndTermination`                                                                                                                                                                                                                                                                     | Exercised              |
| P4-02 | `testBackgroundInterruptsStreamAndPreservesNewDraftForRetry`; controller `testBackgroundingStopsActiveRequestBeforeLifecyclePersistence`, `testRapidBackgroundForegroundStopsOldRequestBeforeAllowingNewSubmission`                                                                                                              | Exercised              |
| P4-03 | `testTerminationDuringStreamingRecoversOneRequestAndRetriesOnce`; FFI `a_relaunch_interrupts_the_open_request_without_rerunning_it`                                                                                                                                                                                              | Exercised              |
| P4-04 | `testConnectionLossBeforeResponseCanRetryWithoutRestart`                                                                                                                                                                                                                                                                         | Exercised              |
| P4-05 | `testConnectionLossDuringStreamCanRetryWithoutDuplicatingDialogue`                                                                                                                                                                                                                                                               | Exercised              |
| P4-06 | `testCompletedActionStaysCompletedThroughRepeatedAppSwitching`                                                                                                                                                                                                                                                                   | Exercised              |
| P4-07 | `testNativeWorldCoreLoopAtEveryAccessibilitySizeAndDarkAppearance` (AccessibilityM through AccessibilityXXXL)                                                                                                                                                                                                                    | Exercised              |
| P4-08 | `testReadingAnchorRestoresTheVisibleHistoricalRow`, `testPagedHistoryRestoresOlderAnchorAndReturnsToNewestCompletedReply`; controller `testRelaunchRestoresFarBackAnchorFromDurableHistory`, `testDurableHistoryPagesWithoutEvictingReadingAnchorAndReturnsToLiveTail`                                                           | Exercised              |
| P4-09 | Transaction failure: `sqlite_failure_rolls_back_generation_state_request_and_event_together` (`limerick-core`), `a_failing_statement_rolls_back_state_tasks_request_and_events_together`. Incompatible save: FFI `an_unreadable_save_is_kept_aside_and_a_new_game_starts`. Interrupted bootstrap: no test; the probe found #2210 | Defect (#2210)         |
| P4-10 | Controller `testCompletionCommitsAndLateOldAttemptCannotWin`, `testLateCallbacksFromCancelledAttemptCannotCommit`, `testRetryCreatesCurrentAttemptBeforeRejectingOldAttemptEvents`; FFI `stop_wins_and_a_late_result_cannot_commit_until_a_retry_runs_a_new_attempt`, `a_second_submission_while_a_request_is_open_is_rejected`  | Exercised              |
| P4-11 | `testTalkingAboutAbsentMichaelStillGetsPeigsReplyAtTheLetterOffice`, `testAskingForSomeoneElsewhereSaysTheyAreNotHere`; FFI `a_person_named_who_is_elsewhere_is_reported_absent_without_an_endpoint_call`                                                                                                                        | Exercised              |
| P4-12 | Controller `testTouchWithoutLeavingBottomDoesNotDisableFollowing`, `testStationaryTouchSurvivesStreamingAndKeyboardGeometryChanges`, `testGrowingHostedRowsAndKeyboardResizeKeepTheActualTailVisible`                                                                                                                            | Exercised              |
| P4-13 | `testPeopleAndCommandsButtonsAvoidSymbolKeyboardAndPreserveDraftUntilSelection` (fixture route), `testAccessibilitySizeKeepsCompletionAndClarificationControlsAboveKeyboard`, `testPhase2CompletionsUseRustNearbyPeople`                                                                                                         | Exercised              |
| P4-14 | `testWaitingKnotAppearsDuringRequestAndDisappearsOnStopAndCompletion` (fixture route). `WaitingAnimation` holds still under Reduce Motion, but no test checks that, or that no activity is left after termination                                                                                                                | Partly; manual (#2193) |

## Spec Milestone 4: mobile reliability

| #   | Requirement                                                 | Verdict                                    | Evidence                                                                                                                                                                                                                                                                                                   |
| --- | ----------------------------------------------------------- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Survives backgrounding without losing committed state       | Met (simulator)                            | P4-01, P4-06                                                                                                                                                                                                                                                                                               |
| 2   | Unsent draft survives backgrounding                         | Met (simulator)                            | P4-01, P4-02; `testNonTestFixtureRegionsAndBackgroundDraft`                                                                                                                                                                                                                                                |
| 3   | Unsent draft survives termination and relaunch              | Met (simulator)                            | P4-01; `testOrdinaryLaunchRestoresHistoryAndDraftAcrossRelaunch`                                                                                                                                                                                                                                           |
| 4   | Backgrounding during inference is defined, never duplicates | Met (simulator)                            | P4-02. The defined behaviour: backgrounding stops the attempt, marks it "Interrupted; not applied", and offers Retry. Controller `testBackgroundingBlocksAQueuedSubmitUntilForeground`, `testBackgroundDuringRetryStopsTheLatestFailedRequest`                                                             |
| 5   | Transcript and world agree after background inference       | Met (simulator)                            | P4-02 (interrupted), P4-06 (completed), both checked again after relaunch                                                                                                                                                                                                                                  |
| 6   | Connectivity loss before a request is recoverable           | Met (simulator)                            | P4-04. Faults are injected at the Endpoint transport only; real network loss is manual (#2193)                                                                                                                                                                                                             |
| 7   | Connectivity loss during a request is recoverable           | Met (simulator)                            | P4-05; `testALostConnectionHasItsOwnLineAndRetryRecovers`                                                                                                                                                                                                                                                  |
| 8   | Restored connectivity continues without a restart           | Met (simulator)                            | P4-04 and P4-05 retry in the same process; `testLocalCommandsAndTravelWorkWhenEveryEndpointRequestFails`                                                                                                                                                                                                   |
| 9   | Retry tells an uncommitted request from a completed action  | Met (simulator)                            | Retry appears only for interrupted or failed attempts (P4-02 to P4-05) and never after completion (P4-01, P4-06)                                                                                                                                                                                           |
| 10  | No duplicate transcript events after reconnect or restore   | Met (simulator)                            | `assertSingleCommand` and the single completed reply across relaunch in P4-03 to P4-06; FFI `a_new_game_opens_on_the_journaled_opening_scene_and_resumes_without_repeating_it`                                                                                                                             |
| 11  | No duplicate authoritative actions                          | Met (simulator); Met (engine)              | P4-01 (one travel), P4-10; `a_failed_commit_writes_nothing_and_an_unchanged_retry_commits_once` (`limerick-core` journal contract)                                                                                                                                                                         |
| 12  | Force-quit at representative points cannot corrupt the save | Partly met; Defect (#2210)                 | During streaming (P4-03) and after completion (P4-01); turn commits are single transactions (P4-09). A new game killed while it is being created leaves a save that never opens again (#2210). Force-quit before acceptance and around final completion is manual (#2193)                                  |
| 13  | Long transcripts responsive over a realistic session        | Partly met; Manual (#2193)                 | `testThousandRowsRenderAndReachBothEndsWithinBudget`, `testLongFixtureTraversesOldestAndNewestWithinInteractionBudget`, FFI `transcript_pages_are_bounded_in_both_directions`. A realistic session and device performance budgets are manual; physical budgets waived                                      |
| 14  | Streaming is scroll-stable in long transcripts              | Met (simulator)                            | P4-08, P4-12                                                                                                                                                                                                                                                                                               |
| 15  | Earlier history readable while output streams               | Met (simulator)                            | `testReadingHistoryExposesNewTextAndReturnsToNewest`, `testAutomaticOutputPreservesHistoryAndNewTextReturnsToFinalReply`                                                                                                                                                                                   |
| 16  | Newest-content affordance understandable and reliable       | Met (simulator); Manual (#2193)            | `testAcceptedMessagesFollowLatestAcrossRepeatedTurnsAndHistoryReading`, P4-12, P4-11 (no stray **New text**). Whether a player finds it understandable is manual                                                                                                                                           |
| 17  | Keyboard, rotation, dictation, text entry keep the composer | Partly met; Defect (#2211); Manual (#2193) | Keyboard show, dismiss and resize: `testMultilineKeyboardDismissesAndReopensWithoutCoveringComposer`, `testComposerTracksNativeEmojiKeyboardHeightChange`, P4-12. The app rotates to landscape without declaring it, and on a small iPhone the transcript shrinks to one line (#2211). Dictation is manual |
| 18  | App switching keeps submitted and unsent text               | Met (simulator)                            | P4-01, P4-02; controller `testAcceptedReceiptRacingBackgroundStillClearsTheOriginalDraft`, `testRetypingTheSameTextDuringAcceptanceKeepsTheNewDraft`                                                                                                                                                       |
| 19  | Usable at accessibility Dynamic Type sizes                  | Met (simulator); Manual (#2193)            | P4-07 on both simulators; Phase 1 accessibility-size tests. Human judgement is manual                                                                                                                                                                                                                      |
| 20  | Core loop usable with VoiceOver                             | Manual (#2193)                             | Labels are checked (row 21); the VoiceOver loop itself needs a person                                                                                                                                                                                                                                      |
| 21  | Sensible accessibility labels and focus order               | Partly met; Manual (#2193)                 | `testCoreAccessibilityLabelsRemainMeaningful`, P4-07 label checks, `testTranscriptKindsAndFocusLoopUseProductionControls` (focus returns to the composer). VoiceOver focus order is manual                                                                                                                 |
| 22  | Errors say what happened and what to do next                | Partly met; Defect (#2210)                 | `testAnUnavailableStorytellerHasItsOwnLine`, `testALostConnectionHasItsOwnLineAndRetryRecovers`, `testPhase2EndpointErrorNamesTheFailureCategory`. The half-created save shows a "try again" message that cannot succeed (#2210). The wider error contract is #1825                                        |
| 23  | 20-minute session without a secondary screen                | Manual (#2193)                             | The opt-in `RundaleSoakUITests` 20-minute soak exists but was not run in this pass. Physical device waived                                                                                                                                                                                                 |
| 24  | Covers a small-screen iPhone and the primary test iPhone    | Met (simulator)                            | Both evidence runs above. Physical devices waived                                                                                                                                                                                                                                                          |
| 25  | No new map, art, inventory, quest, or save UI               | Met (simulator)                            | `testLaunchShowsOnlyPrimaryPlayRegions`. The beta bug report added since (#2181, #2182) is a `/bug` command and a shake, with no new screen                                                                                                                                                                |

**Exit criterion** (the tiny game behaves like a dependable iPhone app;
defects that can lose, duplicate, corrupt, or substantially confuse player
actions block the next milestone): **Not met.** #2210 can leave a save that
never opens again, which blocks. The manual checks in #2193 also remain.

## Defects

| Issue | Defect                                                                                                                | Blocks Milestone 5         |
| ----- | --------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| #2210 | A new game interrupted while it is being created cannot be opened again, and the error says to try again              | Yes (save left unopenable) |
| #2211 | The app rotates to landscape, a layout nobody designed or tests; on a small iPhone the transcript shrinks to one line | No                         |

## Gate wording

`just mobile-verify --phase all` does not describe Phase 4 as ported or
unaccepted: it runs Phases 1–4 as implemented gates and lists later phases as
future work. Because spec §17 now runs to Milestone 7, the tool's phase range
moved from 1–6 to 1–7 in its own PR (#2209).
