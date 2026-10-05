# limerick-bug-report

The private intake for bug reports from the iPhone app (#2022,
[ADR-027](../docs/adr/027-bug-report-intake.md)). A beta tester types `/bug` or
shakes the phone; the app posts a text report and a screenshot here. The
service checks the phone's Firebase credentials and stores the report in a
private Cloud Storage inbox. The [`bug-triage`](../.agents/skills/bug-triage/SKILL.md)
skill turns the inbox into GitHub issues where needed.

It is a sibling of Limerick Endpoints, not part of it: Endpoints stays a
generic AI runtime.

## API

`POST /v1/reports`, with the headers the app sends to Endpoints:
`Authorization: Bearer <Firebase ID token>` and
`X-Firebase-AppCheck: <App Check token>`.

```json
{
  "reportId": "8D6C1B0E-5A37-4C1E-9D55-0A1F2B3C4D5E",
  "description": "what the tester typed after /bug",
  "report": "the engine's report, at most 50,000 characters",
  "build": "0.1.0 (1284)",
  "device": "iPhone17,2 iOS Version 26.6 (Build 23G80)",
  "screenshot": "<base64 PNG, at most 6 MB decoded>"
}
```

| Answer | Meaning                                                                                                                       |
| ------ | ----------------------------------------------------------------------------------------------------------------------------- |
| 202    | Stored, or already stored under this `reportId`. The phone drops its copy.                                                    |
| 400    | Malformed; it will never be accepted. The phone drops it.                                                                     |
| 401    | Missing or invalid credentials, or another app. The phone keeps it.                                                           |
| 429    | Over the per-address limit (30 a minute, checked before credentials) or the per-player hourly limit (20). The phone keeps it. |
| 503    | Storage failed. The phone keeps it.                                                                                           |

`GET /health` answers `{"ok": true}`. Cloud Run reserves `/healthz`.

Each report becomes `inbox/<reportId>/report.json` (written last, so its
presence means the report is complete) and `inbox/<reportId>/screenshot.png`.
The wire shape is pinned on both sides by
[`test/fixtures/phone-report.json`](test/fixtures/phone-report.json).

## Develop

```sh
cd bug-report
pnpm install
pnpm check      # format, lint, typecheck, tests, build
```

## Operate (limerick-prod)

```sh
bash bug-report/deploy/limerick-prod.sh deploy        # bucket, runtime account, build, deploy
bash bug-report/deploy/limerick-prod.sh url
bash bug-report/deploy/limerick-prod.sh list          # waiting report IDs
bash bug-report/deploy/limerick-prod.sh pull <dir>    # download, then delete from the bucket
```

The service URL is `https://limerick-bug-report-877612517009.us-east1.run.app`;
TestFlight builds get it from `mobile/scripts/release.py`
(`RUNDALE_BUG_REPORT_URL`). The image builds as `limerick-build`, like
Endpoints'. The runtime account `limerick-bug-report-runtime` has object
access to the bucket only, plus `firebaseauth.viewer` for token revocation
checks.
