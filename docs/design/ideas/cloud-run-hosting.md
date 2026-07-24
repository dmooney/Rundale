# Hosting Rundale on Google Cloud Run

**Status:** Feasibility analysis (not yet implemented)
**Verdict:** Viable on Cloud Run **Gen 2**, single-instance, with Gemini for
inference and a GCS FUSE mount for persistence.

## Context

Rundale currently deploys via a Railway-targeted Docker image
(`deploy/Dockerfile`) running a single Axum server (`parish-server`) that:

- binds `0.0.0.0:$PORT` via `parish-server --port ${PORT:-3001}` with
  explicit packaged mod and frontend paths;
- serves the pre-built Svelte frontend out of `apps/ui/dist`;
- writes SQLite databases under a `saves/` directory — one global
  `sessions.db` plus per-session `saves/<sid>/parish_NNN.db`
  (`crates/parish-persistence/src/picker.rs:15`,
  `crates/parish-server/src/session.rs:284`);
- keeps live `WorldState`/`NpcManager` in an in-memory `DashMap` session
  registry with per-session background tick tasks
  (`crates/parish-server/src/session.rs:79`,
  `crates/parish-server/src/lib.rs:276`);
- defaults to local Ollama on `localhost:11434` but has a full provider
  abstraction (`crates/parish-config/src/provider.rs:28`) that already
  supports Google Gemini via its OpenAI-compatible endpoint.

A pre-existing `railway.toml` already defines a `GET /api/health` health check
(public, unauthenticated), which Cloud Run can reuse directly.

## Verdict

**Yes — Cloud Run can host Rundale.** The intended configuration (persistent
multi-user production, Gemini/Vertex AI for the LLM, GCS FUSE mount for saves)
runs on Cloud Run **Gen 2** subject to three structural constraints:

1. **Single instance only** (`--max-instances=1`, `--min-instances=1`). The
   session registry and per-session tick tasks live in process memory, and
   SQLite over GCS FUSE is single-writer. Horizontal scaling would split
   sessions across instances and corrupt DB files.
2. **CPU always allocated** (`--cpu-throttling=false`). Tick tasks must keep
   advancing the simulation between HTTP requests; the default "CPU only
   during requests" mode would freeze the living world while idle.
3. **WebSocket sessions capped at 60 min** by Cloud Run's max request timeout.
   The frontend already reconnects on drop, but this should be verified.

These constraints mean Cloud Run hosts Rundale as a _single always-on
container_ rather than an elastically-scaled service — appropriate given the
in-memory, stateful nature of the game world.

## Recommended approach

### 1. Dockerfile tweaks — `deploy/Dockerfile`

The existing image is ~90% ready. Two changes:

- Remove the `cloudflared` download (Dockerfile lines 29–40). Cloud Run
  terminates TLS itself; the tunnel binary is Railway-specific. Keep it only
  if fronting Cloud Run with Cloudflare Access (see Authentication below).
- Otherwise unchanged. The `${PORT:-3001}` passthrough already honours Cloud
  Run's injected `$PORT`, and the non-root `app` user is fine.

### 2. Knative service descriptor — `deploy/cloud-run.yaml` (new)

A declarative `gcloud run services replace` descriptor that captures the full
config so deploys are reproducible:

```yaml
spec:
  template:
    metadata:
      annotations:
        run.googleapis.com/execution-environment: gen2
        run.googleapis.com/cpu-throttling: 'false'
        autoscaling.knative.dev/minScale: '1'
        autoscaling.knative.dev/maxScale: '1'
    spec:
      timeoutSeconds: 3600 # 60-min max; required for long WS sessions
      containers:
        - image: REGION-docker.pkg.dev/PROJECT/parish/parish:TAG
          volumeMounts:
            - name: saves
              mountPath: /app/saves
      volumes:
        - name: saves
          csi:
            driver: gcsfuse.run.googleapis.com
            volumeAttributes:
              bucketName: PROJECT-parish-saves
```

### 3. Environment and secrets

Set on the service (`--set-env-vars` / `--set-secrets`):

| Var                            | Value                     | Notes                                                                      |
| ------------------------------ | ------------------------- | -------------------------------------------------------------------------- |
| `PARISH_PROVIDER`              | `google`                  | Matches the enum in `crates/parish-config/src/provider.rs:28`              |
| `PARISH_MODEL`                 | e.g. `gemini-1.5-flash`   | Pick per cost/quality target                                               |
| `PARISH_API_KEY`               | _Secret Manager ref_      | Gemini API key                                                             |
| `PARISH_WS_SIGNING_KEY`        | _Secret Manager ref_      | Required in release builds (WS token signing)                              |
| `CF_ACCESS_AUD`                | _empty or set_            | Release-build `cf_access_guard` fails closed if unset — see Authentication |
| `PARISH_PUBLIC_URL`            | `https://<cloud-run-url>` | Used by OAuth redirects                                                    |
| `GOOGLE_CLIENT_ID` / `_SECRET` | _optional_                | Only if using in-app Google OAuth sign-in                                  |
| `RUST_LOG`                     | `info`                    |                                                                            |

### 4. GCS bucket for saves

Create `gs://PROJECT-parish-saves` in the service's region. Grant the Cloud
Run service account `roles/storage.objectUser` on it. Gen 2 mounts it at
`/app/saves`, exactly where `crates/parish-persistence/src/picker.rs:15`
writes (`const SAVES_DIR = "saves"`) — no code change needed.

**Caveat:** SQLite WAL over GCS FUSE has known quirks. Confirm DBs are opened
in `journal_mode=DELETE` (not WAL) for FUSE compatibility; the change, if
needed, lives in `crates/parish-persistence/src/`. If FUSE proves unreliable,
the drop-in fallback is a Filestore (NFS) mount, which has stronger POSIX
semantics.

### 5. Authentication

Pick one before exposing the service:

- **Cloud IAM / IAP** (`--no-allow-unauthenticated`) — simplest within GCP.
- **In-app Google OAuth** — already wired in
  `crates/parish-server/src/auth.rs`; set `GOOGLE_CLIENT_ID`/`_SECRET` and
  `PARISH_PUBLIC_URL`.
- **Cloudflare Access in front** — keeps the current auth model; point a CF
  Access application at the Cloud Run URL and set `CF_ACCESS_AUD`.

Doing none of these leaves the world open, and in a release build the
`cf_access_guard` will reject all traffic when `CF_ACCESS_AUD` is empty.

## Files affected (when implemented)

- `deploy/Dockerfile` — drop `cloudflared` download; `$PORT` passthrough kept.
- `deploy/cloud-run.yaml` _(new)_ — Knative descriptor (Gen 2, no CPU
  throttling, min=max=1, GCS FUSE volume, env/secret refs).
- No Rust changes for the happy path. Only `crates/parish-persistence/src/` if
  SQLite-on-FUSE journaling needs adjusting.

## Verification plan

1. **Local image smoke test**

   ```sh
   docker build -f deploy/Dockerfile -t parish:local .
   docker run --rm -p 8080:8080 -e PORT=8080 \
     -e PARISH_PROVIDER=simulator parish:local
   curl -fsS http://localhost:8080/api/health
   ```

2. **Deploy**

   ```sh
   gcloud artifacts repositories create parish \
     --repository-format=docker --location=REGION
   gcloud builds submit --tag REGION-docker.pkg.dev/PROJECT/parish/parish:TAG
   gcloud run services replace deploy/cloud-run.yaml --region=REGION
   ```

3. **Health + startup** — `curl https://<url>/api/health` returns 200; logs
   show the Axum server bound and the session registry initialised.
4. **Persistence** — play to a checkpoint, deploy a new revision, reload, and
   confirm the session restores from `saves/` via the FUSE mount.
5. **Ticks run without requests** — idle the browser 5 min, return, confirm
   NPCs advanced (time, locations, moods). If not, `cpu-throttling=false`
   isn't applied.
6. **Gemini dialogue path** — talk to an NPC; confirm a streamed cloud reply
   (not simulator canned text) and a call to
   `generativelanguage.googleapis.com` in logs.
7. **WebSocket longevity** — hold a WS session open 65+ min to confirm the
   client reconnects cleanly after the 60-min cap (the 409-on-duplicate guard
   in `crates/parish-server/src/ws.rs:88` should be exercised and handled).
