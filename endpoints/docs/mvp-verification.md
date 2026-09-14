# MVP completion audit

Status: complete for the accepted Gemini-only MVP scope

Scope: owner-operated Parish Endpoints MVP

Last reviewed: 2026-09-08

The owner explicitly deferred live OpenAI verification. The OpenAI adapter remains implemented and covered by deterministic contract tests; OpenAI quota and live smoke are outside this acceptance decision. Google Gemini through Vertex AI is the sole live provider path for this audit. No provider credential, consumer key, or invocation content is recorded here or committed to the repository.

This audit distinguishes deterministic repository evidence from live owner-environment evidence. The accepted live scope covers the deployed Gemini path, the real Cottage image and required output schema, Endpoint lifecycle, scoped consumer-key controls, public boundary behavior, metadata-only history, operator controls, and cleanup. A non-owner Firebase session was not run live, and Cloud Run application rollback was not run live; negative authentication is covered by deterministic tests, while application rollback remains documented in the deployment runbook and unexercised. Neither is an additional completion gate for this accepted scope.

## Deployment inputs

| Decision             | Value                                                             |
| -------------------- | ----------------------------------------------------------------- |
| Google Cloud project | `cottage-d6dc9`                                                   |
| Cloud Run region     | `us-east1`                                                        |
| Creator identity     | Firebase Google sign-in; `dmooney@gmail.com`                      |
| Runtime auth         | Vertex AI Application Default Credentials; no Token Creator grant |
| Web                  | `https://parish-web-24861210203.us-east1.run.app`                 |
| API                  | `https://parish-server-24861210203.us-east1.run.app`              |

## Live evidence accepted

- The owner Firebase console reloaded to Ready for `dmooney@gmail.com`; Google is enabled and the Structured Image Extractor is available. Health and readiness returned HTTP 200. Public boundary checks passed: consumer-key management returned 401, another Endpoint returned 403, another organization returned 404, fake JSON/image input returned 400, and the required control/public authentication and CORS checks passed.
- The real JPEG used throughout was 251,708 bytes at 750×1,023. The checked-in `@parish/node-cli-example` sent that same image and validated the response against the Cottage 22-required-field schema for every lifecycle stage:

  | Stage                        | Request                                    |  Latency | Result       |
  | ---------------------------- | ------------------------------------------ | -------: | ------------ |
  | Published v1                 | `req_795010b6-b4fb-4352-8f78-72e98fb4d367` | 3,054 ms | schema-valid |
  | Published/promoted v2        | `req_dbc59394-a185-45fd-9392-8477c121e21a` | 2,886 ms | schema-valid |
  | Pinned v1 while alias was v2 | `req_861ada0a-7c0f-4ce6-9da0-80c1228fe38c` | 2,347 ms | schema-valid |
  | Alias rolled back to v1      | `req_9f60d3e4-09e6-4e22-b984-2eec8aaeccec` | 2,903 ms | schema-valid |

  The final request completed at `2026-09-08T01:37:40Z`. v1 has hash prefix `a4f24`; v2 Draft revision 4 has hash prefix `e87a4` and adds evidence-ordering instructions. Google configuration was unchanged. These requests prove the unversioned alias, pinned-version resolution, and rollback behavior without mutating either published version.

- The UI history shows 7 valid calls, including 3 Draft tests and 4 published API invocations, 25,189 total tokens, and estimated cost `$0.016681`. Invocation records were metadata-only. The endpoint-scoped temporary key was authorized, its one-time secret was hidden after reload, and revocation was verified with HTTP 401 `AUTHENTICATION_FAILED` for `req_verify_99988516-f241-4f3e-b1c2-7cd58712e00d` at `2026-09-08T01:39:31Z`.
- The operator disable/restore gate passed and both ephemeral operator and provider diagnostic jobs were deleted. The endpoint key was revoked and is outside Git; no secret is recorded here.
- Cloud Build, deployment, IAM, migration, liveness/readiness, and service-identity checks remain verified. Server revision `parish-server-00003-bcq` used image digest `sha256:1f0e60842d0eb3eb3a4caeb45d0619bba4eac58aaaf66e0f1e6e3b36f7802156`; web revision `parish-web-00002-gkz` used `sha256:e7669e951ac9b09760d436f7493fe2cc911ecb85260e955dc92d03e75dfb0dc1`; migration job `parish-migrate-c74ns` succeeded. Both revisions received 100% traffic. No live non-owner Firebase session or Cloud Run application rollback was performed; the former has deterministic negative-auth coverage, while application rollback is documented in the deployment runbook only and remains unexercised.
- The actual Cottage contract was `packet-analysis-v8` with schema SHA-256 `f276d3013a474f9a58d5fd0e9ab78f220fb23fb50a0ec9ee1e521b4c22b9ee0b`; the deployed Draft test was `req_81b9509e-4190-4142-8d07-6902dad2215c`. The operator disable test returned HTTP 503 with unchanged usage/history for `req_7bbee4c6-3301-414a-8ced-fd2643f6d6c2`, and the restored successful real-image test was `req_58b2affa-4047-4521-b3b6-eeaa94e50787`.

## Requirement disposition

| Area                                                                                       | Disposition                                                                                                                                               |
| ------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Generic typed/versioned Endpoint scope and separation from Parish Engine                   | Verified by `docs/product-vision.md`, `docs/software-architecture.md`, `packages/domain`, `packages/runtime`, and two independent reviews                 |
| Owner Firebase authentication and control plane                                            | Live owner flow verified; `apps/server/src/auth/creator-auth.ts`, control routes, and deterministic negative auth tests retained                          |
| Real Gemini/Vertex execution and 22-required-field Cottage schema                          | Verified with the same real JPEG through Draft, v1, v2, pinned v1, and rollback; `packages/providers/src/google/adapter.ts` and runtime schema validation |
| Immutable versions, aliases, promotion, pinning, rollback                                  | Verified live by the four request records above; `packages/domain`, `apps/server/src/control/service.ts`, and lifecycle tests also passed                 |
| Consumer-key entropy, scoping, one-time display, management rejection, and revocation      | Verified live and by `packages/auth/src/index.ts`, control service, and deterministic auth/control tests                                                  |
| Public input limits, normalized errors, allowlists, timeouts, operator switch, and privacy | Verified by live boundary/operator checks plus `apps/server/src/invocation/service.ts`, operator code, and deterministic tests; history is metadata-only  |
| Provider-neutral runtime and OpenAI adapter                                                | Deterministically verified; OpenAI live smoke explicitly deferred by owner decision                                                                       |
| Deployment, IAM, migrations, health/readiness, builds, and reviews                         | Verified                                                                                                                                                  |
| Node CLI behavior and schema validation                                                    | Verified live across all four lifecycle stages; `examples/node-cli/src/cli.ts` and deterministic CLI tests also passed                                    |

## Repository and review evidence

- `pnpm check` passed formatting, lint, typecheck, deterministic tests, and builds. The established result was 89 passing tests with 2 credential-gated live tests skipped across 16 passing test files and 1 skipped live file; PostgreSQL integration was exercised with synthetic `DATABASE_URL_TEST` configuration. No repeat testing was required for this documentation update.
- Two independent final reviews cleared implementation correctness, security/privacy, lifecycle atomicity, provider boundaries, usage handling, and documentation consistency.

## Explicit boundaries

OpenAI live verification remains deferred and is not a blocker. Non-owner Firebase live sign-in and Cloud Run application rollback were not executed and are not represented as live evidence. Custom DNS, marketplace, billing, arbitrary code, RAG, tools, streaming, extra providers, and other post-MVP work remain outside this audit.

## Cleanup

The temporary Endpoint key was revoked and is outside Git; its private key file was removed, and an exact key scan found zero repository matches. Both ephemeral operator and provider diagnostic jobs were deleted. Google inference was restored. No provider credential, API key, raw image, raw output, prompt, or private invocation content is stored in this document.
