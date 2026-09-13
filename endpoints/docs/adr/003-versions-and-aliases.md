# ADR 003: Immutable versions and mutable deployment aliases

- Status: Accepted
- Date: 2026-08-25

## Decision

Endpoint Drafts are mutable with optimistic revisions. Publishing copies a validated draft into an immutable, content-hashed Endpoint Version. The `production` Deployment Alias is the only unversioned pointer and is updated with optimistic alias revisions.

## Consequences

Promotion and rollback are atomic pointer moves. Pinned version URLs remain stable, concurrent edits fail with conflicts, and no published behavior is changed in place.
