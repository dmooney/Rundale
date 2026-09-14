# Parish mobile FFI

This crate owns the callback-free C boundary used by the native mobile client.
Each call copies bounded UTF-8 JSON into Rust and returns one owned JSON buffer;
the caller releases that buffer exactly once with
`parish_mobile_owned_bytes_free`. Rust keeps only an opaque session handle in a
registry, and one mutex per session serializes access to the authoritative
`limerick_core::mobile::MobileSession`.

The production build enables the `engine-api` feature:

```sh
rustup run 1.98.0 cargo build \
  --manifest-path limerick/Cargo.toml \
  -p limerick-mobile-ffi --no-default-features --features engine-api
```

The JSON operations are `submit`, `retry`, `stop`, `fail`, `receive_failure`,
`receive_frame`, `receive_candidate`, `read_events`, `read_event_page`,
`read_event_page_before`, `snapshot`, and
`pending_endpoint`. The `receive_failure` operation accepts the native
camel-case fields `attemptID`, `baseRevision`, `errorKind`, and `message`;
the bridge also accepts snake-case aliases for compatibility.
Successful responses use `{ "ok": true, "value": ... }`; failures use a
bounded `{ "ok": false, "error": ... }` envelope. The core DTOs define the
payload fields and serialization names, so the bridge does not mirror engine
objects in the C header.

Every successful envelope fits the 256 KiB response limit. Snapshots project a
recent event/request tail and preserve active, newest retryable, and newest
unresolved clarification metadata. Attempt projections retain the original and
current attempts. These bounds do not prune the authoritative save or alter
idempotency. Clients restore pending clarification from the request's durable
`pendingClarification` field; its original event may precede the retained tail.

History pages may contain fewer events than the requested limit when constrained
by bytes. Forward pages retain a prefix and advance from the last returned
sequence; backward pages retain a suffix and advance from the first. Use the
returned `nextCursor` and `hasMore` rather than subtracting page sizes or treating
a short page as exhaustion.
