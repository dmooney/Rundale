# Plan: mobile bug reporting

> Status: Accepted · Created: 2026-10-05 · Revised: 2026-10-05 · Issue: #2022 · Decision: [ADR-027](../adr/027-bug-report-intake.md)

A tester reports a bug from the iPhone app in one step: type `/bug` with an
optional description, or shake the phone. The app sends a report and a
screenshot to `limerick-bug-report`, which keeps them in a private inbox. An
agent triages the inbox with the [`bug-triage`](../../.agents/skills/bug-triage/SKILL.md)
skill and files a GitHub issue only when one is needed.

## Decisions (owner, 2026-10-05)

| Question            | Decision                                                                                            |
| ------------------- | --------------------------------------------------------------------------------------------------- |
| How a report starts | `/bug` with an optional description, or a shake. Nothing else for the tester to do.                 |
| Who can report      | Beta builds only (`RUNDALE_BETA_FEEDBACK=YES`). Not offered in an App Store build.                  |
| Where it goes       | `limerick-bug-report`, its own Cloud Run service in limerick-prod, never Limerick Endpoints.        |
| Storage             | A private Cloud Storage inbox. Nothing is public; the service holds no GitHub credential.           |
| Triage              | The `bug-triage` skill, run on demand. It pulls each report locally and deletes it from the bucket. |
| Screenshots         | Stay private. An issue describes the screenshot in words.                                           |

### Rejected

- **TestFlight feedback with a pasted report** (shipped in #2181, then
  replaced). Apple offers no way for an app to open or fill TestFlight's
  feedback sheet: the tester had to screenshot, open the sheet, and paste.
- **The iOS share sheet.** Still several taps and a choice of app.
- **Filing GitHub issues directly from the service.** GitHub's API cannot
  attach images, so screenshots would need a public home. The owner chose to
  keep them private and to file only after triage.

## Flow

1. The tester types `/bug the miller ignored me`, or shakes the phone.
2. The app takes a screenshot of what the player sees, asks the engine for
   the report (`bug_report` operation), and saves both to disk. It then sends
   them with the same Firebase ID and App Check tokens it sends to Endpoints.
   The notice reads "Sending the bug report…", then "Bug report sent. Thank
   you."
3. Offline, or if the service is down: "No connection. The bug report will be
   sent when you're back online." The report is sent at the next launch or
   foreground.
4. The service checks both tokens and the app ID, applies a per-player hourly
   limit, and writes `inbox/<id>/screenshot.png` and then
   `inbox/<id>/report.json` to `gs://limerick-prod-bug-reports`. Both writes
   are create-only, so a resent report is stored once.
5. The `bug-triage` skill pulls and deletes each report, investigates, and
   files an issue, comments on an existing one, or sets the report aside.

`/bug` never becomes a game turn and leaves the save unchanged. It works while
a reply streams. A typed `/bug` clears the draft; a shake keeps it.

## Report

Composed by `limerick-diagnostics::mobile_report` (portable, pure), at most
50,000 characters:

- the description;
- app build and engine contract version;
- scene, time of day, weather, and who is present;
- the open request and what it waits on, if any;
- the newest transcript lines from the journal (these survive a relaunch);
- the last eight Endpoint calls answered since launch: Endpoint, duration,
  what was asked, and the reply or failure.

Sections are filled newest first and rendered in time order; each line has its
own cap.

## Pieces

| Piece                                                    | Role                                                                       |
| -------------------------------------------------------- | -------------------------------------------------------------------------- |
| `limerick-diagnostics::mobile_report`                    | Composes the report. The GitHub reporter sits behind the `github` feature. |
| `limerick-mobile-ffi` `bug_report`                       | Returns the report; keeps the Endpoint exchange log. Read-only.            |
| `mobile/Rundale/BugReportFiler.swift`                    | Outbox on disk, HTTP transport, screenshot capture.                        |
| `RundalePresentationModel`                               | `/bug`, shake, notices, sending queued reports at launch and foreground.   |
| [`bug-report/`](../../bug-report/README.md)              | The `limerick-bug-report` service and its deploy and inbox script.         |
| [`bug-triage`](../../.agents/skills/bug-triage/SKILL.md) | Turns the inbox into GitHub issues.                                        |

## Verification

- `pnpm check` in `bug-report/`: format, lint, typecheck, tests, build. The
  phone's encoding and the service's parsing are both tested against
  `bug-report/test/fixtures/phone-report.json`.
- `cargo test -p limerick-diagnostics` (with and without default features) and
  `cargo test -p limerick-mobile-ffi`.
- `just mobile-verify --phase all`, including `RundaleBugReportTests` and
  `RundaleCommandsUITests.testBugCommandSendsAReportWithoutATurn`.
- Live: the simulator app, with an App Check debug token, sent a shake report
  to the deployed service. `limerick-prod.sh pull` retrieved the report and
  screenshot and emptied the inbox.
