# Inference errors and recovery

Audit of issue 1825 (inference errors and recovery) against the current native
iPhone and Limerick Endpoints path, on the commit this document ships with. It
records what is implemented and proven, the error contract as built, the gaps,
and the owner's decisions on them (2026-10-24). The governing requirements are
[spec §6 Request Lifecycle and §7 Stop, Retry, and History](../product-specs/mobile-product-technical-spec.md#6-request-lifecycle)
and [§11 Inference Architecture](../product-specs/mobile-product-technical-spec.md#11-inference-architecture).
The attempt contract is in [portable-turn-api.md](portable-turn-api.md).

Evidence labels: **scripted** is the Rust engine with a scripted host or the
mock Endpoint transport; **simulator** is an XCUITest or XCTest run on the iOS
Simulator; **live** is the same app against `limerick-prod`. Physical-iPhone
acceptance is waived (2026-10-04); nothing here claims it.

## 1. The contract as built

One failure travels one path. The host (`RundaleEngineController`) classifies
each failed Endpoint call into a `LimerickFailureReason` and reports it with
`runtime.fail`. For a dialogue call, or an intent call that never produced an
answer (every reason but `garbled`), the shared engine ends the attempt
`Failed` (or `Interrupted`), discards the candidate, and shows the mod's line
for the reason. A garbled intent answer continues as an uncertain
classification (decision 1 below).
The line is the one from `mods/rundale/loading.toml` `[failure_lines]`. The diagnostic
string (an Endpoint code or a fixed phrase) goes to the
`com.rundale.mobile:endpoint` log only; the player never sees it, a provider
body, a credential or reasoning. A failure with no reason, which includes every
desktop in-process failure, keeps the generic `DIALOGUE_RETRY_MESSAGE`.

| Condition                                                                                                         | Reported as                       | Player hears (key)         | Next action offered |
| ----------------------------------------------------------------------------------------------------------------- | --------------------------------- | -------------------------- | ------------------- |
| No network, lost or broken connection, DNS, TLS, stream ended before its terminal frame                           | `URLError`, `missingTerminal`     | `offline`                  | Retry               |
| Connection idle timeout (`URLError.timedOut`)                                                                     | `URLError.timedOut`               | `timed_out`                | Retry               |
| Endpoint `REQUEST_TIMEOUT`                                                                                        | code in error frame or body       | `timed_out`                | Retry               |
| `RATE_LIMITED`, `PROVIDER_RATE_LIMITED`, `QUOTA_EXCEEDED`, HTTP 429                                               | code or status                    | `busy`                     | Retry               |
| `PROVIDER_UNAVAILABLE`, `INTERNAL_ERROR`, endpoint or version missing or disabled, HTTP 5xx                       | code or status                    | `unavailable`              | Retry               |
| `AUTHENTICATION_FAILED`, `AUTHORIZATION_FAILED`, HTTP 401 or 403, no App Check credential                         | code, status, `invalidCredential` | `refused`                  | Retry               |
| Endpoint misconfiguration in the app (bad URL, insecure URL, Endpoint version mismatch)                           | `LimerickEndpointError`           | `unavailable`              | Retry               |
| `MODEL_ERROR`, `OUTPUT_VALIDATION_FAILED`, `INVALID_INPUT`, unknown code, malformed or oversize frames, other 4xx | code, status, protocol error      | `garbled`                  | Retry               |
| `REQUEST_CANCELLED` not caused by Stop                                                                            | code                              | `cancelled`                | Retry               |
| Player Stop                                                                                                       | task cancel, `DELETE` on stream   | "Interrupted; not applied" | Retry               |

Recovery in every row is the same, Retry (or a new command), because the line
names the cause and the player cannot fix any of them. That is why the
distinctions that exist stay at seven keys. Where a distinction would change
the action (a configuration fault the operator must fix, a quota that will not
clear in a moment) the contract does not carry it yet; that is issue 2304.

Optional background inference (arrival reactions, banter, tier simulation) does
not run on mobile. Travel encounters and reactions have no Endpoint role, so
their route is unavailable and no call is made. The per-role policy in
portable-turn-api.md §4.3 (canned text or silent finalisation, never a
player-visible error) is the intended handling when issue 2025 builds the host
seam, and that issue should adopt it rather than add an error surface. Nothing
here requires an unbuilt feature.

## 2. What is already implemented and proven

| Requirement                                                                | Evidence                                                                                                                                                                                                                              | Kind                |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- |
| A failed call ends visibly, with its cause named, and input is released    | mobile-ffi `a_failed_call_shows_the_mods_line_for_its_reason`, `EngineScenarioTests.testEndpointErrorShowsTheModsLineForItsCategory`, `testLocalCommandsAndTravelWorkWhenEveryEndpointRequestFails` (real client, unresolvable host)  | scripted, simulator |
| Each reason has its own mod line, never the generic one                    | `limerick-mobile-ffi` test over `FailureReason::ALL`, `loading.toml` `[failure_lines]`                                                                                                                                                | scripted            |
| Retry is a new attempt of the same logical request, no duplicate commit    | `turn_lifecycle.rs`: retry reuses the settled intent reply, failed commit installs nothing and the retry commits once; mobile-ffi `an_endpoint_failure_ends_the_attempt_failed_and_a_retry_can_commit`; `RundaleEngineLifecycleTests` | scripted, simulator |
| Stop wins and a late result cannot commit                                  | `stop_discards_the_attempt_and_a_late_result_cannot_commit`, mobile-ffi `stop_wins_and_a_late_result_cannot_commit_until_a_retry_runs_a_new_attempt`, `testPhase2StopLeavesInterruptedAttemptAndRetryCompletes`                       | scripted, simulator |
| Stop cancels the server-side invocation                                    | Live `test02`; live `test09` records a `REQUEST_CANCELLED` invocation with `cancellation_requested` (pull request 2293, issue 2171)                                                                                                   | live (simulator)    |
| Relaunch interrupts the open request without re-running it                 | `restart_recovery_interrupts_the_open_request_without_rerunning_it`, mobile-ffi `a_relaunch_interrupts_the_open_request_without_rerunning_it`, `testTerminationDuringStreamingRecoversOneRequestAndRetriesOnce`, force-quit tests     | scripted, simulator |
| Connection loss before or during a stream recovers on Retry, no duplicates | `EngineScenarioTests` (`testConnectionLoss...`)                                                                                                                                                                                                       | simulator           |
| Background and foreground stop an active request and keep the draft        | `RundalePhase4Tests`, `testBackgroundInterruptsStreamAndPreservesNewDraftForRetry`                                                                                                                                                    | simulator           |
| Interrupted or failed requests leave no partial world state                | turn engine candidate discipline; `failed_provider_turn_discards_partial_and_returns_retry_outcome`, journal rollback tests                                                                                                           | scripted            |

The browser stall in issue 1896 (a dialogue left on Loading for five minutes,
then a reload that swallowed later commands) is not reproduced on the iPhone
path. A stalled stream is bounded three ways: the connection's 60 second idle
timeout ends a silent socket, the Endpoint enforces `PROVIDER_TIMEOUT_MS` and
answers `REQUEST_TIMEOUT`, and Stop is always offered while a request runs. After
relaunch the open request is interrupted and Retry is offered, and the
composer is not left disabled. No live test induces a stall, because the
deployed Endpoints cannot be made to stall on demand without production surface;
that stays unproven live.

## 3. Gaps

Closed in this change:

1. **The failure classification had no unit test**, only two mock cases through
   the UI. `EndpointFailureClassificationTests` now pins every Endpoint error code
   the server emits, the common HTTP statuses, the URL error codes, the
   missing-terminal, malformed-event and invalid-credential errors, and that
   the diagnostic carries no server message (RundaleSession package tests,
   Linux and macOS). The other
   `LimerickEndpointError` cases (size limits, version and correlation
   errors) are not individually pinned. The controller's classifier is made
   visible to the tests.
2. **The intent failure policy had no regression test.** Both halves of it,
   as decided below, are now pinned in `turn_lifecycle.rs` (scripted).
3. **A failed intent call that never produced an answer continued silently**
   as an uncertain classification, so a free-form move or action (for example
   "head over to Darcy's", which the local parser does not take) could run as
   speech with no signal to the player. Owner decision 1 (2026-10-24): every
   reason but `garbled` (`offline`, `busy`, `unavailable`, `timed_out`,
   `refused`, `cancelled`) now fails the request recoverably, with the mod's
   line and Retry, as a dialogue failure does. A garbled answer, or a failure
   a host reports without a reason (the desktop's in-process model), stays
   uncertain. `game_loop/input.rs`; pinned in `turn_lifecycle.rs` for each
   reason, including the retry (scripted). The design is updated in
   [dynamic-npc-context.md §4.4](dynamic-npc-context.md#44-failure).
4. **A client timeout was reported as a lost connection.** Owner decision (2026-10-24):
   `URLError.timedOut` now reports `timed_out` ("the answer wandered off")
   rather than `offline`. With decision 1 a timed-out intent call still fails
   the request, so call volume does not change. Pinned in
   `EndpointFailureClassificationTests` (RundaleSession package tests, Linux
   and macOS).

Tracked elsewhere, by owner decision:

1. **The Endpoint cannot say why a provider call failed beyond five buckets**,
   and **`Retry-After` and quota are not carried.** Provider authentication,
   quota, model-not-found and safety refusals all arrive as `MODEL_ERROR`,
   which the player hears as `garbled`; `QUOTA_EXCEEDED` maps to `busy`.
   Distinguishing them needs new Endpoint codes and new player copy: issue
   2304, outside Stability.
2. **Authorization and session expiry is proven only with the mock.** App
   Check token refresh after expiry, and the `refused` line on a real 401 or
   403, are covered by the mock Endpoint, not live. The owner accepts mock-only
   evidence for these; inducing them live would mean revoking a debug token
   mid-run.

With these decisions the issue's acceptance criteria hold on the current path:
failures terminate visibly and release input, the cause is named in game voice
with no credential, provider body or reasoning, a failed classification no
longer silently reinterprets an action, and Retry, Stop and relaunch follow the
attempt contract without a duplicate commit. Actionable copy for operator
faults and refusals moves to issue 2304.
