# Notebook Person Art Candidates

The provider-generation command writes local, non-shipping candidate runs here.
Each job has a content-addressed object directory containing the exact prompt
and input record. Every provider call writes an immutable attempt directory with
its raw PNG and either a transparent candidate or failure receipt. A successful
job-level receipt points to the accepted attempt and starts with review status
`pending`; retries never overwrite earlier paid responses.

Candidate runs are intentionally ignored by Git. A later explicit approval step
promotes selected objects into reviewed source assets and records the receipt
hash; merely generating a candidate cannot alter the runtime asset pack.
