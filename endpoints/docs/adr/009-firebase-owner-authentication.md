# ADR 009: Firebase owner authentication

- Status: Accepted
- Date: 2026-08-25
- Supersedes: creator-authentication portion of ADR 005

## Decision

Use Firebase Authentication in the existing Cottage Firebase project for creator identity, with Google as the MVP sign-in provider. The web console obtains a Firebase ID token and sends it as a Bearer token to control-plane routes. The server verifies the token and its revocation status with the Firebase Admin SDK using Cloud Run Application Default Credentials, then authorizes only the exact configured owner UID. The database maps that UID to an organization-scoped owner record.

Firebase client configuration is public build-time configuration. No Firebase service-account key is stored or shipped. Consumer invocation API keys remain separate from creator identity and cannot authorize management routes.

## Consequences

The Parish runtime service identity needs read-only Firebase Authentication access for revocation checks. Each deployed web hostname must be registered as a Firebase authorized domain. The initial console offers Google sign-in only, while local development can continue using the explicit synthetic identity mode.
