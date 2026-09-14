# Parish Endpoints

Parish Endpoints is an owner-operated runtime for versioned, typed AI API behavior. A creator defines a mutable Endpoint Draft, tests it, publishes immutable Endpoint Versions, and moves the `production` Deployment Alias to promote or roll back behavior. Consumers invoke a stable provider-neutral HTTP API.

## Local development

Requirements: Node.js 22+, pnpm 10.33.2, Docker, and Docker Compose.

```sh
cp .env.example .env
docker compose up -d
pnpm install --frozen-lockfile
node --env-file=.env --run db:migrate
node --env-file=.env --run db:seed
node --env-file=.env --run dev
```

The web console is at `http://localhost:3000`, the API is at `http://localhost:3001`, and readiness is at `http://localhost:3001/health/ready`. Development authentication sends the synthetic `user_synthetic_owner` identity; production refuses this mode and verifies the configured Firebase owner.

Set `SEED_CREATE_API_KEY=true` only when you need a local CLI key. The key is printed once and only its SHA-256 digest is stored.

## Quality gates

```sh
pnpm check
```

This runs formatting, lint, TypeScript checks, deterministic tests, and all builds. Set `DATABASE_URL_TEST` to include the real PostgreSQL workflow suite. Live provider tests are opt-in and require `LIVE_PROVIDER_TESTS=true` plus the relevant provider key.

From the Rundale repository root, the equivalent command is
`just endpoints-check`; `just endpoints-dev`, `just endpoints-test`, and
`just endpoints-db-migrate` retain the workspace's independent lifecycle.

## Node image client

```sh
export PARISH_API_KEY='the-one-time-key'
pnpm --filter @parish/node-cli invoke -- \
  --image ./packet.png \
  --endpoint https://api.example/v1/endpoints/parish-demo/generic-image-extractor \
  --schema ./output-schema.json
```

The CLI accepts PNG, JPEG, or WEBP, makes one multipart request, validates the JSON response when `--schema` is supplied, and never prints the API key.

## Operations

Inference can be stopped without redeploying:

```sh
DATABASE_URL=... pnpm operator disable global
DATABASE_URL=... pnpm operator disable provider openai
DATABASE_URL=... pnpm operator disable model openai/gpt-model
DATABASE_URL=... pnpm operator disable organization 00000000-0000-0000-0000-000000000000
DATABASE_URL=... pnpm operator disable endpoint 00000000-0000-0000-0000-000000000000
DATABASE_URL=... pnpm operator enable global
```

The runtime also enforces organization and Endpoint switches, daily organization quotas, per-process rate limits, request and decoded-image limits, provider timeouts, model allowlists, output-token limits, and independently validates every successful output.

## Google Cloud deployment

The production topology uses isolated resources in the existing Cottage Google Cloud project: `parish-server` and `parish-web` Cloud Run services, a dedicated Cloud SQL PostgreSQL instance, Secret Manager, a least-privilege runtime identity, and Artifact Registry images built by Cloud Build.

1. Provision the Parish Cloud SQL database and runtime service account in `us-east1`.
2. From the Rundale root, build the server with `endpoints/deploy/google/cloudbuild.server.yaml` and the web app with `endpoints/deploy/google/cloudbuild.web.yaml`; both Cloud Build steps use `endpoints/` as their build directory.
3. Store the database URL and OpenAI credential in Secret Manager; configure the Firebase project and exact owner UID as non-secret environment values.
4. Use Vertex AI Application Default Credentials for Google inference; no Google API key is required in Cloud Run.
5. Run migrations through the explicit `parish-migrate` Cloud Run job before server rollout.
6. Verify the generated `run.app` hostnames; custom domains such as `api.parish.dev` and `app.parish.dev` are optional follow-up work.

See [the deployment runbook](docs/deployment.md) for configuration and release verification.

## Security posture

Control routes live under `/api/control/v1` and require the configured creator owner. Invocation routes live under `/v1/endpoints/{organizationSlug}/{endpointSlug}`. Server callers use hashed, scoped consumer keys; mobile callers use a Firebase Auth ID token plus mandatory App Check, mapped by server configuration to one explicit organization and immutable Endpoint-version allowlist. Mobile principals cannot use alias routes. Neither invocation principal can authorize creator routes.

Completed JSON remains available on the base and pinned-version routes. Their
`/stream` counterparts use the versioned SSE contract described in
[ADR 012](docs/adr/012-mobile-invocation-streaming.md). Google supports
validated structured streaming; OpenAI retains completed Responses calls.
Provider credentials, creator instructions, raw images, raw inputs, and raw
outputs are excluded from persistent logs. Invocation records contain safe
metadata, validation, usage, latency, cancellation/failure status, and
estimated provider cost only.
