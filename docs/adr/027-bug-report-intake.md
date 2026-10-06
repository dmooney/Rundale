# ADR-027: A private bug-report intake service beside Limerick Endpoints

- Status: Accepted
- Date: 2026-10-05
- Issue: #2022
- Plan: [mobile-bug-report.md](../plans/mobile-bug-report.md)

## Context

iPhone beta testers need to report bugs in one step. Apple gives an app no way
to submit TestFlight feedback, and GitHub's API cannot attach images to an
issue. The phone must not hold a GitHub credential. Limerick Endpoints is a
generic runtime for typed AI calls and must carry no game-specific or non-AI
behavior (`endpoints/AGENTS.md`).

## Decision

- Add `limerick-bug-report`, a separate Cloud Run service in limerick-prod
  built from `bug-report/`. It is a sibling deployable to Limerick Endpoints,
  not part of it.
- It authenticates the phone exactly as Endpoints does: a Firebase ID token
  plus a mandatory App Check token for an allowed app.
- It stores each report and screenshot in a private Cloud Storage bucket
  (`gs://limerick-prod-bug-reports`, public access prevention enforced). Its
  runtime account can reach that bucket only. It holds no GitHub credential.
- An agent triages the inbox with the `bug-triage` skill using the operator's
  own credentials. It pulls each report locally, deletes it from the bucket,
  and files a GitHub issue only when one is warranted. Screenshots stay
  private and are described in words.

## Consequences

- One more deployable, with its own deploy script
  (`bug-report/deploy/limerick-prod.sh`) and CI job.
- Reports wait until someone runs triage; nothing reaches GitHub automatically.
- The hourly per-player limit is held in memory, so the service runs with one
  instance.
- After triage, the only copy of a report is under
  `~/.cache/limerick/bug-reports/` on the machine that pulled it.
