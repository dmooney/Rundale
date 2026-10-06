# bug-report — agent scope

`limerick-bug-report`: the private intake for bug reports from the iPhone app.
Start with [README.md](README.md); the decision is
[ADR-027](../docs/adr/027-bug-report-intake.md).

## Commands

```sh
cd bug-report && pnpm check                       # what CI runs
bash bug-report/deploy/limerick-prod.sh deploy    # limerick-prod; pre-authorized like Endpoints
bash bug-report/deploy/limerick-prod.sh pull <dir>
```

## Traps

- **Not part of Limerick Endpoints.** Do not move this into `endpoints/` or
  import its packages; Endpoints must stay a generic AI runtime.
- **The wire shape has two sides.** The Swift `PendingBugReport` (in
  `mobile/Rundale/BugReportFiler.swift`) and `src/report.ts` both test against
  `test/fixtures/phone-report.json`. A key renamed on one side only answered
  400 live while each side's own tests passed (`reportID` against
  `reportId`). Change the fixture and both sides together.
- **Cloud Run reserves `/healthz`.** Google's front end answers it with its
  own 404 before the request reaches the service; use `/health`.
- **Build as `limerick-build`.** `gcloud run deploy --source` and a plain
  `gcloud builds submit` run as the default compute account, which cannot read
  the staged source (403 on `storage.objects.get`). The script submits
  `deploy/cloudbuild.yaml` with `--service-account` and Cloud-Logging-only
  logs.
- **Create-only writes make resends safe.** Both objects are written with
  `ifGenerationMatch: 0`; a 412 means the object is already there. Keep
  `report.json` last.
- **A pulled report has one copy.** `pull` deletes from the bucket after the
  download arrives; the local `~/.cache/limerick/bug-reports/<id>/` is then the
  only copy. Never commit it.
- **Never log request bodies.** They hold player text and screenshots.
