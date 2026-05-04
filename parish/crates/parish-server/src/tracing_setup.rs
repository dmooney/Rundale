//! OpenTelemetry tracing support for the Parish web server (#621).
//!
//! Gated by the `otel-tracing` feature flag (default-on per CLAUDE.md rule #6).
//! The OTLP exporter is further gated by the `PARISH_OTEL_ENDPOINT` environment
//! variable: when unset no network I/O occurs, so local development has zero
//! external side-effects.
//!
//! ## Usage
//!
//! Call [`try_build_otel_provider`] once at startup *before* initialising the
//! `tracing` subscriber.  It returns `None` when `PARISH_OTEL_ENDPOINT` is
//! unset (no-op path) or `Some(provider)` when the exporter is configured.
//! Pass `Some(provider.tracer("parish-server"))` to [`OpenTelemetryLayer::new`]
//! and include it in your subscriber stack.
//!
//! Shut the provider down gracefully on exit:
//!
//! ```ignore
//! if let Some(p) = otel_provider { p.shutdown(); }
//! ```
//!
//! ## Standard span fields
//!
//! Every request span records the following fields when available.  See
//! `docs/agent/tracing.md` for the canonical reference.
//!
//! | Field        | Type   | Set by                        |
//! |-------------|--------|-------------------------------|
//! | `request_id` | `str`  | `request_id_layer` middleware |
//! | `session_id` | `str`  | session middleware             |
//! | `account_id` | `str`  | CF-Access auth context        |
//! | `route`      | `str`  | `request_id_layer` middleware |
//! | `method`     | `str`  | `request_id_layer` middleware |
//! | `model`      | `str`  | inference pipeline            |
//! | `status`     | `u16`  | `request_id_layer` middleware |
//! | `latency_ms` | `u64`  | `request_id_layer` middleware |

use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// Environment variable name that enables the OTLP exporter.
///
/// When set to a non-empty URL (e.g. `http://localhost:4318`) the server ships
/// spans to an OpenTelemetry collector via OTLP/HTTP.  When unset (the default
/// for local development) the exporter pipeline is skipped entirely — no
/// network connections, no background threads for export.
pub const OTEL_ENDPOINT_ENV: &str = "PARISH_OTEL_ENDPOINT";

/// Attempts to build an OTLP span exporter from environment variables.
///
/// Returns `Some(SdkTracerProvider)` when `PARISH_OTEL_ENDPOINT` is set and
/// the exporter can be built.  Returns `None` otherwise — callers should
/// simply omit the OTel layer from their subscriber stack.
///
/// This function does **not** call `tracing_subscriber::...::init()`.  The
/// caller is responsible for composing the layer into their subscriber.
pub fn try_build_otel_provider(service_name: &str) -> Option<SdkTracerProvider> {
    let endpoint = std::env::var(OTEL_ENDPOINT_ENV)
        .ok()
        .filter(|s| !s.is_empty())?;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint.clone())
        .build()
        .map_err(|e| {
            // Emit to stderr (no tracing subscriber yet at this point).
            eprintln!(
                "[parish-server] WARNING: Failed to build OTLP exporter for {} — \
                 OTel tracing disabled: {}",
                endpoint, e
            );
        })
        .ok()?;

    let resource = Resource::builder()
        .with_service_name(service_name.to_string())
        .build();

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    Some(provider)
}

/// Gracefully flushes and shuts down an OTel provider if one was configured.
///
/// Call this during server shutdown so buffered spans are exported before the
/// process exits.
pub fn shutdown_otel(provider: Option<SdkTracerProvider>) {
    if let Some(p) = provider
        && let Err(e) = p.shutdown()
    {
        eprintln!(
            "[parish-server] WARNING: OTel provider shutdown error: {}",
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// The env var name must remain stable — operators configure their
    /// deployment pipelines against it.
    #[test]
    fn otel_endpoint_env_var_constant_is_correct() {
        assert_eq!(OTEL_ENDPOINT_ENV, "PARISH_OTEL_ENDPOINT");
    }

    #[test]
    #[serial(parish_env)]
    fn try_build_otel_provider_returns_none_when_endpoint_unset() {
        // SAFETY: serialised via `#[serial(parish_env)]` — no concurrent
        // threads touch this var while this test runs.
        unsafe { std::env::remove_var(OTEL_ENDPOINT_ENV) };
        let provider = try_build_otel_provider("test-service");
        assert!(
            provider.is_none(),
            "should return None when PARISH_OTEL_ENDPOINT is not set"
        );
    }

    #[test]
    #[serial(parish_env)]
    fn try_build_otel_provider_returns_none_for_empty_endpoint() {
        // SAFETY: serialised via `#[serial(parish_env)]`.
        unsafe { std::env::set_var(OTEL_ENDPOINT_ENV, "") };
        let provider = try_build_otel_provider("test-service");
        assert!(
            provider.is_none(),
            "empty PARISH_OTEL_ENDPOINT should be treated as unset"
        );
        unsafe { std::env::remove_var(OTEL_ENDPOINT_ENV) };
    }
}
