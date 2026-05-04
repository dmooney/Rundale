# Observability — tracing and OpenTelemetry (#621)

## Overview

The Parish web server ships structured tracing via the [`tracing`] crate with an optional
OpenTelemetry (OTel) OTLP export path. Everything is gated so local development produces zero
network I/O by default.

## Feature flag

`otel-tracing` (default-on). Disable explicitly to skip the `request_id_layer` middleware:

```json
{ "flags": { "disabled": ["otel-tracing"] } }
```

## Environment variables

| Variable                | Effect                                                                         |
|------------------------|--------------------------------------------------------------------------------|
| `PARISH_OTEL_ENDPOINT` | Base URL of an OTLP/HTTP collector (e.g. `http://localhost:4318`). When unset, the OTel exporter pipeline is skipped entirely — no background threads, no network I/O. |

## Standard span fields

Every `http.request` span opened by `request_id_layer` carries these fields. Fields marked
"deferred" start empty and are recorded later by the named middleware/handler.

| Field        | Type  | Source                                                               |
|-------------|-------|----------------------------------------------------------------------|
| `request_id` | str   | `request_id_layer` — UUID v4, minted once per inbound request        |
| `route`      | str   | `request_id_layer` — `req.uri().path()`                              |
| `method`     | str   | `request_id_layer` — HTTP verb                                       |
| `status`     | u16   | `request_id_layer` — recorded after the inner service returns        |
| `latency_ms` | u64   | `request_id_layer` — wall-clock milliseconds for the full request    |
| `session_id` | str   | `session_middleware` / `session_middleware_tower` (deferred)         |
| `account_id` | str   | `cf_access_guard` — Cloudflare Access email (deferred)               |
| `model`      | str   | inference pipeline — model tag used for this request (deferred)      |

The `request_id` is echoed in the `X-Request-Id` response header so clients can correlate logs.

## Per-request metrics event

`request_id_layer` also emits a structured `tracing` event at `INFO` level on completion:

```
target: "parish_server::metrics"
event:  "http.request.complete"
fields: request_id, route, method, status, latency_ms
```

Log-based metric tools (Datadog, Loki, CloudWatch Logs Insights) can aggregate these without
a separate Prometheus endpoint.

## Per-session tick metric

The world-tick background task emits a `DEBUG`-level event each tick:

```
target: "parish_server::metrics"
event:  "session.tick"
fields: session_id, tick (generation counter)
```

## Code locations

| Concern                   | File                                                         |
|--------------------------|--------------------------------------------------------------|
| OTel provider setup       | `parish/crates/parish-server/src/tracing_setup.rs`          |
| Request-ID middleware      | `parish/crates/parish-server/src/middleware.rs`             |
| Subscriber composition    | `parish/crates/parish-cli/src/main.rs`                      |
| Session field recording   | `parish/crates/parish-server/src/middleware.rs`             |
| Account-id recording      | `parish/crates/parish-server/src/lib.rs` (`cf_access_guard`)|
| Per-session tick metric   | `parish/crates/parish-server/src/session.rs`                |

## Dep version matrix (locked)

| Crate                   | Version |
|------------------------|---------|
| `opentelemetry`         | 0.29.x  |
| `opentelemetry_sdk`     | 0.29.x  |
| `opentelemetry-otlp`    | 0.29.x  |
| `tracing-opentelemetry` | 0.30.x  |

`tracing-opentelemetry 0.30.x` requires `opentelemetry ^0.29`. All four must move together;
see the [compatibility table](https://crates.io/crates/tracing-opentelemetry) before upgrading.
