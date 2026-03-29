# Bolt's Journal - Critical Learnings Only

## 2026-03-29 - HTTP Client Reuse in reqwest
**Learning:** `reqwest::Client` maintains an internal connection pool with keep-alive. Creating a new client per request wastes that pool entirely — each new client starts fresh TCP+TLS negotiations. Both `DEFAULT_TIMEOUT_SECS` and `STREAMING_TIMEOUT_SECS` were identical (30s), making the separate streaming client completely redundant.
**Action:** Always check if reqwest clients are being reused across the struct. A single client handles different timeout needs via per-request `.timeout()` if truly needed.
