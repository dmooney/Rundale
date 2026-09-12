# Rundale Phase 2 Test Cases

> Imported on: 2026-09-07
> Source modified (UTC): 2026-09-05 15:55:27.928 UTC
> Google source: [Google Doc](https://docs.google.com/document/d/1Fsw8hSp13XZT_F9uMOdwAWyFTslBV_G03cmX2W6-G7A/edit)
>
> Note: This test plan is source-derived planning material and is not execution evidence.

These cases validate Phase 2: the embedded Rust vertical slice. They focus on proving that Limerick runs locally on iPhone, that the SwiftUI client communicates through a narrow presentation-oriented boundary, that one real NPC conversation works through remote inference, and that state, errors, cancellation, retry, and local persistence behave correctly.

## 1. Create a new local game

- Launch the Phase 2 build with no existing save.
- Start a new game.
- Verify the game initializes locally on the device.
- Verify the player enters the single Phase 2 location with the single Phase 2 NPC available.

## 2. Resume an existing local game

- Create a game and perform at least one completed action.
- Terminate and relaunch the app.
- Resume the game.
- Verify the same authoritative game state is restored without requiring a remote Limerick server.

## 3. Limerick runtime is on-device

- Disable access to any remote Limerick game server while leaving the app otherwise functional.
- Launch or resume the game.
- Run a deterministic action such as /look.
- Verify it succeeds locally.

## 4. No desktop-only runtime dependency

- Build and run the iOS gameplay target.
- Verify normal gameplay does not require Tauri, an Axum game server, desktop process spawning, local-model launching, desktop diagnostics, or browser-only infrastructure.

## 5. /look is authoritative and local

- Disconnect the device from the network.
- Run /look.
- Verify the command succeeds.
- Verify the returned location information comes through the same transcript event path used by the player UI.

## 6. Basic free-text NPC conversation

- Restore network connectivity.
- Enter a natural-language message directed at the single NPC.
- Verify the request is accepted and produces a real NPC response through remote inference.
- Verify the response appears in the transcript rather than a secondary UI.

## 7. Interpretation receipt

- Submit a free-text request whose meaning is clear enough to interpret.
- Verify the game displays an understandable interpretation receipt when useful.
- Verify the receipt corresponds to the action Limerick actually executes.

## 8. Real streaming response

- Submit a request that produces a nontrivial NPC response.
- Verify output appears incrementally as it is received.
- Verify the transcript remains usable while the response streams.
- Verify completion is clearly distinguishable from partial output.

## 9. Stop active inference

- Start a long inference-backed NPC response.
- Stop it before completion.
- Verify output stops promptly.
- Verify the app returns to a usable input state.
- Verify the game remains coherent and resumable.

## 10. Stop does not partially commit world state

- Trigger an inference-backed request that would cause a state change only on successful completion.
- Stop the request before completion.
- Inspect or exercise the resulting game state.
- Verify no partial authoritative state change was committed.

## 11. Inference failure handling

- Force the inference request to fail, for example through a controlled mock failure or unavailable endpoint.
- Verify the player sees a comprehensible error.
- Verify the transcript remains intact.
- Verify the current game session remains usable.
- Verify no authoritative state corruption occurs.

## 12. Retry failed inference

- Cause an inference-backed request to fail before completion.
- Use Retry.
- Verify the original player request is preserved.
- Verify the retry can complete successfully.
- Verify only one successful gameplay action is ultimately committed.

## 13. Retry cannot duplicate a completed action

- Complete a state-changing request successfully.
- Attempt the failure/retry path or equivalent duplicate-delivery condition for the same request identity.
- Verify the already committed action is not applied a second time.
- Verify duplicate transcript completion is also prevented.

## 14. Offline deterministic gameplay

- Disconnect the device from the network.
- Exercise every Phase 2 action that does not inherently require inference.
- Verify each works normally.
- Attempt an NPC interaction that does require inference.
- Verify only the inference-dependent portion fails or waits for connectivity rather than making the entire game unusable.

## 15. No long-lived provider secret in the app

- Inspect the built app configuration and normal runtime setup used for Phase 2 inference.
- Verify no long-lived OpenAI, Google, or other provider credential is embedded in the shipped application bundle or exposed through ordinary client configuration.

## 16. Completed action persists locally

- Perform a state-changing action and allow it to complete.
- Force-quit the app immediately afterward.
- Relaunch and resume.
- Verify the completed state change is present.
- Verify the corresponding transcript events are restored consistently.

## 17. Interrupted request is not restored as completed

- Start a state-changing inference-backed request.
- Interrupt the app before the request completes or commits.
- Relaunch and resume.
- Verify the world does not reflect a partially completed action.
- Verify the transcript clearly represents the interrupted state rather than falsely showing successful completion.

## 18. Event identity prevents duplicate restoration

- Create a session with several commands and responses.
- Terminate and relaunch repeatedly.
- Verify previously persisted transcript events appear exactly once.
- Verify restoring the session does not append duplicates of commands, responses, errors, or completion events.

## 19. Request correlation remains correct

- Submit multiple requests sequentially, including at least one streamed response and one failed or stopped response.
- Verify each command, interpretation receipt, streamed output, error/stop state, and completion state remains associated with the correct request.
- Verify content from one request never appears attached to another.

## 20. Phase 2 scope regression

- Play through the entire Phase 2 vertical slice.
- Verify the experience remains limited to one location and one NPC.
- Verify no map, portrait system, provider-configuration screen, debug surface, save DAG, legacy sidebar, or other nonessential player UI has been introduced.
- Verify the result already feels like a small playable text game rather than a technology demo.
