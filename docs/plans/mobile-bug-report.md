# Plan: mobile bug reporting through TestFlight feedback

> Status: Accepted · Created: 2026-10-05 · Issue: #2022 · Milestone: Mobile Phase 7

Testers of the iPhone beta report bugs through TestFlight's own feedback, with
the game's context pasted into the comment. Nothing leaves the phone except
what the tester sends, and no credential ships in the app.

## Decisions (owner, 2026-10-05)

| Question                      | Decision                                                                                     |
| ----------------------------- | -------------------------------------------------------------------------------------------- |
| How a report leaves the phone | TestFlight feedback (screenshot plus comment). No Endpoints route, no GitHub token.          |
| How a report starts           | `/bug` with an optional description, or shaking the phone.                                   |
| What a report carries         | Screenshot (TestFlight's), world state, recent transcript, recent Endpoint exchanges.        |
| Who can report                | Beta builds only: debug builds and TestFlight installs. Not offered in an App Store build.   |
| Where reports go              | They stay in App Store Connect. No sync to GitHub.                                           |
| Large payloads                | The app composes a bounded text report and copies it; the tester pastes it into the comment. |

The issue's earlier proposal (a `BugReportSink` trait with an Endpoints sink
holding a GitHub token) is superseded by these decisions.

## Constraints

- The app cannot open or fill TestFlight's feedback sheet. The tester opens it
  by taking a screenshot. It carries the screenshot, the tester's comment, and
  device, OS, and build details.
- A TestFlight feedback comment holds up to 4,000 characters. The report
  targets 90% of that (3,600 characters, counted as Unicode scalar values so
  Irish text is measured as typed), per
  [external API payload caps](../agent/test-tooling-rules.md#external-api-payload-caps).
- Reporting is diagnostics, not gameplay: `/bug` never becomes a logical
  request, never reaches an Endpoint, and leaves the save unchanged.

## Flow

1. The tester types `/bug the miller ignored me`, or shakes the phone.
2. The app asks the engine for the report (`bug_report` operation), copies it
   to the clipboard, and shows: "Bug report copied. Take a screenshot, send it
   as TestFlight feedback, and paste the report into the comment."
3. The tester screenshots, opens TestFlight feedback, and pastes.
4. The owner reads feedback in App Store Connect.

`/bug` clears the draft only when it was typed; shaking leaves the draft alone.

## Report

Composed by `limerick-diagnostics::feedback_report` (portable, pure):

- the description (whole, up to 1,000 characters);
- app build and engine contract version;
- scene, time of day, weather, and who is present;
- the open request and what it waits on, if any;
- the newest transcript lines from the journal (survive relaunch);
- the newest Endpoint exchanges since launch: Endpoint slug and version, how
  long it took, an excerpt of its input, and its reply or failure.

Sections are filled newest first until the budget runs out, then rendered in
time order. Each line has its own cap so one long reply cannot crowd out the
rest.

## Changes

1. **`limerick-diagnostics`.** Add `feedback_report` (report types, the
   composer, the budget). Put the GitHub bug-report module behind a default
   `github` feature, so a portable build carries none of the token lookup,
   `gh` subprocess, or GitHub HTTP code. Desktop is unchanged.
2. **`limerick-mobile-ffi`.** Keep the last eight Endpoint exchanges in memory
   (recorded when `resolve` or `fail` answers the awaited call). Add the
   `bug_report` operation (`description`, optional `build`) that returns the
   report text. It reads only; tests prove the journal and revision stay
   unchanged. Building it found that a stale `resolve` or `fail` (which the
   engine ignores) cleared the session's awaited call, so `pending_endpoint`
   answered `null` while the engine still waited; `settle` now keeps the call
   on an ignored step.
3. **iPhone app.** `RundaleBridge` gains `bugReport(description:build:)`. The
   presentation model handles `/bug` before submit (even while a reply
   streams) and a shake from a `UIWindow` motion override, in beta builds
   only; shake-to-undo is turned off there so a shake does not also offer to
   undo typing. Completions offer `/bug` as the player types.
   Beta builds are marked by the `RUNDALE_BETA_FEEDBACK` build setting (`YES`
   in `project.yml`; `testflight-update` sets it and checks the archive). A
   runtime TestFlight check was rejected: StoreKit's `AppTransaction` can
   throw on TestFlight builds, and the receipt-file check is deprecated, which
   fails a build that treats warnings as errors.
4. **Docs.** Tester steps in [mobile/testflight.md](../../mobile/testflight.md),
   the operation in the FFI README, and this plan.

## Verification

- `cargo test -p limerick-diagnostics` (with and without default features) and
  `cargo test -p limerick-mobile-ffi`.
- `just check`, and `just mobile-verify --phase all` (builds the iOS device and
  simulator libraries and runs the UI suites).
- A UI test: `/bug` copies a report that names the scene and the description,
  shows the notice, clears the draft, and creates no request.
- A simulator recording of `/bug` and the Simulator's Shake command. The
  simulator is not a physical iPhone; the TestFlight sheet itself is checked
  on a TestFlight build.
