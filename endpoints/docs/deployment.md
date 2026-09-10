# Google Cloud deployment and verification runbook

## External prerequisites

- Owner access to Google Cloud project `cottage-d6dc9` with billing enabled
- Firebase Authentication enabled in `cottage-d6dc9`, with Google sign-in enabled
- the exact Firebase UID authorized as the Parish owner: `JvyQWyqV5MelZdF5RIPE8dPPYbm1` for `dmooney@gmail.com`
- a server-side OpenAI credential for the current live runtime configuration; OpenAI live verification is deferred by owner decision and is not required for the Gemini-only acceptance gate
- DNS access is optional and outside MVP completion; use the generated Cloud Run hostnames for initial verification

## Topology

Create Parish-only resources in `us-east1`: `parish-server` and `parish-web` Cloud Run services, `parish-postgres` Cloud SQL PostgreSQL, `parish-runtime` service identity, and `parish-migrate` Cloud Run job. Cottage services, identities, secrets, and data are not modified. The generated Cloud Run server and web hostnames are the MVP URLs; custom domains remain optional follow-up work.

Cloud Run connects to Cloud SQL through its managed Unix socket. Store the complete socket-based `DATABASE_URL` and any configured provider credential in Secret Manager. The Gemini-only verification path uses Vertex AI Application Default Credentials; it does not make an OpenAI request. Configure the non-secret Firebase project ID and owner UID as server environment variables. Grant the runtime identity only Cloud SQL Client, Vertex AI User, Firebase Authentication Viewer, and access to the exact secrets it consumes. Do not grant `roles/iam.serviceAccountTokenCreator`; the server uses Application Default Credentials for Firebase verification and Vertex AI calls without signing tokens.

Use `AUTH_MODE=firebase`, `FIREBASE_PROJECT_ID=cottage-d6dc9`, `PROVIDER_MODE=live`, and `GOOGLE_PROVIDER_AUTH=vertex-ai`. Set `GOOGLE_CLOUD_PROJECT=cottage-d6dc9` and `GOOGLE_CLOUD_LOCATION=global`. Keep the Google model allowlist narrow for the Gemini-only live gate. The OpenAI adapter remains available and its allowlist/configuration may remain present for the current runtime, but its live smoke is deferred by owner decision. `MODEL_PRICES_JSON` must contain non-negative per-million input and output token prices for every allowed model; configuration fails closed when a price is missing.

The web image needs `NEXT_PUBLIC_API_URL` plus the Firebase API key, auth domain, project ID, and app ID at build time. Firebase web configuration identifies the project and is public configuration; server credentials must never be Docker build arguments or repository files.

## Build and release

The checked-in Dockerfiles exclude local dotenv files, including `.env.local` and
other `.env.*` variants; only the synthetic `.env.example` may enter the build
context. Both images build and run as the unprivileged `node` user. The server
image keeps the workspace development dependencies because the migration job
continues to use the checked-in `pnpm db:migrate` command, which invokes `tsx`.

1. Submit `deploy/google/cloudbuild.server.yaml` with `_IMAGE` set to a versioned Artifact Registry path.
2. Configure the `parish-migrate` job from the same immutable image, Cloud SQL attachment, runtime identity, and database secret. Execute it and require success before rollout.
3. Deploy `parish-server` from the image digest with database, Firebase owner identity, provider, limits, allowlists, prices, and CORS origin configured. Keep maximum instances bounded for the MVP.
4. Submit `deploy/google/cloudbuild.web.yaml` with `_IMAGE`, `_NEXT_PUBLIC_API_URL`, and the four `_NEXT_PUBLIC_FIREBASE_*` substitutions.
5. Deploy `parish-web` from the image digest with `parish-server` as its API origin.
6. Keep the previous healthy Cloud Run revisions available until release verification completes.

## Release verification

1. Confirm `GET /health/live` and `GET /health/ready` return 200 on the generated server hostname.
2. Register the generated web hostname as a Firebase authorized domain. Sign in with the Google owner account and confirm unauthenticated control access receives 401. Non-owner, expired, and revoked-token rejection remain deterministic authentication-test evidence unless an additional live identity is deliberately available.
3. Create a generic image Endpoint from the current Cottage-derived SeedPacket contract and save its Draft. Derive the contract from the current Cottage code at verification time; do not add Cottage-specific branches to the Parish runtime.
4. Upload a supported image in the playground and confirm the test is recorded as a draft test with valid output.
5. Publish v1 and promote it to `production`.
6. Create an Endpoint-scoped key and copy it once. Keep the key in the operator's secure runtime environment; never place it in the repository, image, logs, or an evidence document.
7. Run the Node CLI with the Endpoint output schema and confirm schema-valid JSON.
8. Change the draft, publish v2, promote it, and verify production resolves v2 while `/versions/1` still resolves v1.
9. Move `production` back to v1 and verify the next unversioned invocation resolves v1.
10. Confirm invocation history and usage totals include the tests and API calls without raw content.
11. Run one bounded Vertex AI Google smoke request with tight output and cost limits. The deployed Gemini draft tests provide the required live provider proof for this acceptance scope. The OpenAI Responses adapter remains implemented and covered by deterministic contract tests; its live smoke is deferred by explicit owner decision and is not a release gate. The Google API-key Interactions path is also covered by deterministic adapter tests and is not required in the Vertex deployment.
12. Disable and re-enable a model with the operator CLI; verify inference fails closed while disabled.

## Optional custom domains

Custom DNS is outside the MVP completion gate. If it is added later, verify both generated Cloud Run hostnames first. Prefer a global external Application Load Balancer for production custom domains; Cloud Run domain mapping is acceptable for follow-up validation in `us-east1`. Add only the ownership and DNS records Google provides, wait for managed certificates, then rebuild the web image and update server CORS for the final hostnames.

## Rollback

Behavior rollback moves the Endpoint's `production` Deployment Alias and never modifies an immutable Endpoint Version. Application rollback moves Cloud Run traffic to a previous healthy revision. Database migrations must remain forward-compatible with overlapping revisions.
