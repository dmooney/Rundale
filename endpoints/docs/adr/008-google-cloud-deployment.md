# ADR 008: Google Cloud deployment topology

- Status: Accepted
- Date: 2026-08-25
- Supersedes: ADR 007

## Decision

Deploy Parish Endpoints into the existing Cottage Google Cloud project as isolated resources: separate public Cloud Run services for `parish-server` and `parish-web`, a dedicated Cloud SQL for PostgreSQL instance, a dedicated least-privilege service identity, Secret Manager values, and Artifact Registry images built by Cloud Build. Run schema migrations as an explicit Cloud Run job before application rollout rather than during every server start.

The Google adapter supports either Gemini API-key authentication or Vertex AI Application Default Credentials. The Google Cloud deployment uses Vertex AI with the Cloud Run service identity, project, and global location; provider authentication remains an adapter concern and does not alter public Endpoint contracts.

## Consequences

Cottage and Parish share a billing and administrative project but not services, databases, service identities, or application data. The server receives Cloud SQL Client, Vertex AI User, Firebase Authentication Viewer, and narrowly scoped Secret Manager access. OpenAI and database credentials remain server-side secrets. The web image contains only public Firebase and API configuration. Behavior rollback remains a Deployment Alias operation, while Cloud Run revisions provide application rollback.
