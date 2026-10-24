# Inference errors and recovery

Audit of issue 1825 (inference errors and recovery) against the current native
iPhone and Limerick Endpoints path, on the commit this document ships with. It
records what is implemented and proven, the error contract as built, the gaps,
and the decisions that belong to the owner. The governing requirements are
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
`runtime.fail`. For a dialogue call, or an intent call lost to the connection
(`offline`), the shared engine ends the attempt `Failed` (or `Interrupted`),
discards the candidate, and shows the mod's line for the reason. Any other
intent failure continues as an uncertain classification instead (gap 1 below).
The line is the one from `mods/rundale/loading.toml` `[failure_lines]`. The diagnostic
string (an Endpoint code or a fixed phrase) goes to the
`com.rundale.mobile:endpoint` log only; the player never sees it, a provider
body, a credential or reasoning. A failure with no reason, which includes every
desktop in-process failure, keeps the generic `DIALOGUE_RETRY_MESSAGE`.

| Condition                                                                                                         | Reported as                       | Player hears (key)         | Next action offered |
| ----------------------------------------------------------------------------------------------------------------- | --------------------------------- | -------------------------- | ------------------- |
| No network, lost or broken connection, DNS, TLS, stream ended before its terminal frame                           | `URLError`, `missingTerminal`     | `offline`                  | Retry               |
| Connection idle timeout (`URLError.timedOut`), reported as `offline` today (see gap 5)                            | `URLError.timedOut`               | `offline`                  | Retry               |
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
clear in a moment) the contract does not carry it yet; see the gaps below.

Optional background inference (arrival reactions, banter, tier simulation) does
not run on mobile. Travel encounters and reactions have no Endpoint role, so
their route is unavailable and no call is made. The per-role policy in
portable-turn-api.md §4.3 (canned text or silent finalisation, never a
player-visible error) is the intended handling when issue 2025 builds the host
seam, and that issue should adopt it rather than add an error surface. Nothing
here requires an unbuilt feature.

## 2. What is already implemented and proven

| Requirement                                                                | Evidence                                                                                                                                                                                                                                    | Kind                |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- |
| A failed call ends visibly, with its cause named, and input is released    | `RundaleFailureLinesUITests` (unavailable provider, lost connection), `RundalePhase2UITests.testPhase2EndpointErrorNamesTheFailureCategory`, `testLocalCommandsAndTravelWorkWhenEveryEndpointRequestFails` (real client, unresolvable host) | simulator           |
| Each reason has its own mod line, never the generic one                    | `limerick-mobile-ffi` test over `FailureReason::ALL`, `loading.toml` `[failure_lines]`                                                                                                                                                      | scripted            |
| Retry is a new attempt of the same logical request, no duplicate commit    | `turn_lifecycle.rs`: retry reuses the settled intent reply, failed commit installs nothing and the retry commits once; mobile-ffi `an_endpoint_failure_ends_the_attempt_failed_and_a_retry_can_commit`; `RundaleEngineLifecycleTests`       | scripted, simulator |
| Stop wins and a late result cannot commit                                  | `stop_discards_the_attempt_and_a_late_result_cannot_commit`, mobile-ffi `stop_wins_and_a_late_result_cannot_commit_until_a_retry_runs_a_new_attempt`, `testPhase2StopLeavesInterruptedAttemptAndRetryCompletes`                             | scripted, simulator |
| Stop cancels the server-side invocation                                    | Live `test02`; live `test09` records a `REQUEST_CANCELLED` invocation with `cancellation_requested` (pull request 2293, issue 2171)                                                                                                         | live (simulator)    |
| Relaunch interrupts the open request without re-running it                 | `restart_recovery_interrupts_the_open_request_without_rerunning_it`, mobile-ffi `a_relaunch_interrupts_the_open_request_without_rerunning_it`, `testTerminationDuringStreamingRecoversOneRequestAndRetriesOnce`, force-quit tests           | scripted, simulator |
| Connection loss before or during a stream recovers on Retry, no duplicates | `RundalePhase4NetworkUITests`                                                                                                                                                                                                               | simulator           |
| Background and foreground stop an active request and keep the draft        | `RundalePhase4Tests`, `testBackgroundInterruptsStreamAndPreservesNewDraftForRetry`                                                                                                                                                          | simulator           |
| Interrupted or failed requests leave no partial world state                | turn engine candidate discipline; `failed_provider_turn_discards_partial_and_returns_retry_outcome`, journal rollback tests                                                                                                                 | scripted            |

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
   the UI. `EndpointFailureClassificationTests` now pins every Endpoint code,
   HTTP status, transport error and the no-secret diagnostic (simulator). No
   shipped behaviour changes; the controller's classifier is only made visible
   to the tests.
2. **The intent failure policy had no regression test.** A lost connection
   during the intent call fails the request recoverably, and every other intent
   failure continues as an uncertain classification. Both halves are now pinned
   in `turn_lifecycle.rs` (scripted).

Open, each needing an owner decision (not built here):

1. **Intent failure other than a lost connection continues silently.** The
   design (dynamic-npc-context.md §4.4) makes any other failure uncertain: the
   recipient rules apply, with two or more people present the game asks, with
   one present that person answers. The line then runs as speech. If the
   dialogue call that follows succeeds, the player gets no signal that their
   free-form action (for example "head over to Darcy's", which the local parser
   does not take) was heard as talk. If it fails too, the dialogue failure line
   shows. Options: keep as designed; or fail the request recoverably, with the
   existing failure line and Retry, for every failure of the transport class
   (`busy`, `unavailable`, `timed_out`, `refused`) while leaving a malformed
   answer (`garbled`) uncertain. The second is a one-condition change at
   `game_loop/input.rs` and reuses existing copy, but it edits a design owned by
   the dynamic NPC context work, so it is not made here.
2. **The Endpoint cannot say why a provider call failed beyond five buckets.**
   Provider authentication, quota, model-not-found and safety refusals all
   arrive as `MODEL_ERROR`, which the player hears as `garbled` ("tangle, try
   again"). Those are operator faults or terminal refusals, where retrying does
   not help. Distinguishing them needs new codes in the Endpoint contract and
   new player copy.
3. **`Retry-After` and quota are not carried.** `busy` says to wait a moment.
   `QUOTA_EXCEEDED` will not clear in a moment, and a 429 may carry a delay the
   app ignores. Carrying it needs a field in the Endpoint error and, for the
   delay, copy that can state it.
4. **Authorization and session expiry is mapped but not proven live.** App Check
   token refresh after expiry, and the `refused` line on a real 401 or 403, are
   covered only by the mock. Inducing it live means revoking or expiring a
   debug token.
5. **A client timeout is reported as a lost connection.** Any `URLError`,
   including `URLError.timedOut`, maps to `offline`, so a call that ran out of
   time says the road is washed away rather than the answer wandered off.
   Mapping it to `timed_out` is trivial but is not made here, because it is
   coupled to decision 1: the engine fails an intent call recoverably only for
   `offline`, so the remap would let a timed-out intent call continue as
   uncertain and make a further inference call (a change in call volume).
   Decide it together with decision 1; the unit test pins today's mapping.

The unresolved items do not weaken the spec's lifecycle requirements: every
failure terminates, releases input, names its cause in game voice, and
leaves the request retryable without a duplicate commit.
