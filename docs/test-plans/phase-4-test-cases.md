# Phase 4 — Mobile reliability

This local plan implements the [Phase 4 requirements](../product-specs/product-technical-spec.md)
and supplements the earlier phase gates. It is not an execution record. Record
results in [mobile acceptance](../../mobile/acceptance.md), including failures and
unavailable devices. Gameplay breadth remains the canonical three-place world.

## Automated regression matrix

Run `./verify --phase 4 --simulator <UDID>`. This includes the Phase 1–3 regression
suites, portable runtime and persistence tests, Endpoint and bridge contracts,
device packaging, native controller tests, and the Phase 4 native recovery suite.
`./verify --phase all` includes the same gates and lists Phases 5–6 as future work.
Deterministic network cases inject faults only at the Endpoint transport boundary.
They do not establish live Firebase, App Attest, or cellular network behavior.

| Case  | Exercise                                                                                           | Required outcome                                                                                          |
| ----- | -------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| P4-01 | Travel, type an unsent draft, switch apps, terminate, relaunch                                     | Location, exact draft, and one committed travel command survive                                           |
| P4-02 | Background while dialogue streams with a newer draft                                               | Uncommitted reply is interrupted; newer draft survives; retry completes once                              |
| P4-03 | Terminate during streaming, relaunch, retry, relaunch again                                        | One logical command, one completed reply, no spontaneous duplicate request                                |
| P4-04 | Lose connection before any response; retry after recovery                                          | Recoverable failure, no committed dialogue before retry, normal continuation                              |
| P4-05 | Lose connection after partial output; retry                                                        | Partial text remains visibly uncommitted; only the validated retry commits                                |
| P4-06 | Complete dialogue and switch apps repeatedly                                                       | Completed action stays completed and offers no misleading retry                                           |
| P4-07 | Use the native world at accessibility text size in dark mode                                       | Header, draft and Send remain available; travel succeeds                                                  |
| P4-08 | Earlier phase long-history, resize and anchor cases                                                | Visible historical row keeps its offset during streaming and relaunch                                     |
| P4-09 | Storage transaction failure, incompatible schema and interrupted bootstrap                         | Committed save remains intact; failure cannot masquerade as a successful action                           |
| P4-10 | Replayed, late and duplicate events/candidates                                                     | Existing identity, sequence and commit guards reject duplication                                          |
| P4-11 | At Peig's Letter Office, say Michael directed you there; try explicit absent addressing separately | Ordinary speech reaches Peig; only explicit unavailable targets fail presence checks                      |
| P4-12 | Touch/bounce at the tail, append and grow rows, resize the keyboard                                | Automatic following remains active; deliberate history reading remains anchored                           |
| P4-13 | Tap People and Commands using the alphabetic keyboard, including large text                        | Browsing retains the draft; selection is editable before Send; controls remain accessible                 |
| P4-14 | Start, stop and complete a response, including Reduce Motion                                       | Native Celtic knot indicates activity; stationary with Reduce Motion; no stale activity after termination |

### Follow recovery during conversation

Repeat short and long dialogue turns with the alphabetic keyboard visible.
Include a touch at the transcript tail while text arrives, a held touch during
composer/keyboard resizing, and activity appearing/disappearing. These layout
changes must keep the newest response visible without exposing **New text**.
Then deliberately scroll up during a reply: history must stay anchored and
**New text** must appear. Sending a new accepted message must rejoin its latest
exchange; an empty or ignored Send must preserve the reading position.

## Physical sessions and phase-end demo

Run at least 20 minutes on both the primary iPhone and a supported small-screen
iPhone. Record model, iOS version, app version/build, revision, date, tester,
appearance, text size, duration, defects and evidence paths. Select the minimum
supported device/OS using actual results; the iOS 17 deployment target alone is
not compatibility evidence.

1. Read, type, select/edit text, dictate, dismiss/reopen the keyboard, send,
   recall a command, and use Stop/Retry. Keep an unsent draft while switching apps.
2. Visit all three places, observe scheduled presence, resolve Connolly
   ambiguity, and resume the same state after relaunch.
3. Start real dialogue. Background at acceptance and during streaming. Return,
   inspect the outcome, and retry only an interrupted request. Repeat with
   airplane mode before sending and during streaming; restore connectivity.
4. Force-quit before acceptance, during streaming and around final completion.
   Confirm no completed action is lost or duplicated and the save still opens.
5. Read earlier history during new output, return to newest, resize the composer,
   and relaunch while reading an older passage. Include a long saved session.
6. Repeat the core loop in light/dark mode and accessibility Dynamic Type.
   With VoiceOver, verify labels, focus order, announcements, interruption status,
   clarification and recovery without relying on sight.
7. Lock/unlock the phone while saving/streaming; verify data protection does not
   strand the save. Check real App Attest and expired-credential recovery.
8. Use Instruments to record launch/resume, local-action and save latency,
   scrolling responsiveness and peak/resident memory with growing histories.
   Agree device-specific budgets before sign-off and retain measured results.

The phase-end demo follows these interactions using the actual running build.
Simulator demonstrations must be labelled as such. Any defect that loses,
duplicates, corrupts or materially misrepresents an action blocks Phase 5.
No automated pass closes the human accessibility or device-performance gates.
