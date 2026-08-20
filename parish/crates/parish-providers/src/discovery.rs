use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, Utc};
use parish_config::{
    AuthAdapter, CATALOG_CACHE_SCHEMA_VERSION, CatalogCacheDocument, CatalogCacheIdentity,
    CatalogConflictKind, CatalogConflictObservation, ConfigDiagnostic, DiagnosticSeverity,
    DiscoveredModel, DiscoveryAdapter, DiscoveryStatus, EndpointDefinition,
    OutputContractCapabilities, ReasoningEffortV2, SecretString,
};
use reqwest::{Client, StatusCode, Url};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_PAGES: usize = 100;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_MODELS: usize = 5_000;

/// Fetch and normalize a live model catalog. Failures are explicit cacheable
/// states, never an authoritative empty model list.
pub async fn fetch_catalog_endpoint(
    identity: CatalogCacheIdentity,
    endpoint: &EndpointDefinition,
    credential: Option<&SecretString>,
    ttl: Duration,
    prior: Option<&CatalogCacheDocument>,
) -> CatalogCacheDocument {
    let fetched_at = Utc::now();
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("static redirect-disabled discovery client configuration must build");
    let deadline = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        fetch_pages(
            &client,
            endpoint,
            credential,
            prior.and_then(|cache| cache.etag.as_deref()),
            prior.and_then(|cache| cache.last_modified.as_deref()),
        ),
    )
    .await;
    let (
        status,
        routes,
        diagnostics,
        etag,
        last_modified,
        payload_hash,
        payload_fetched_at,
        retry_after_at,
        consecutive_failures,
    ) = match deadline {
        Ok(Ok(page)) if page.not_modified => {
            let old = prior.expect("304 is only possible when a prior ETag was supplied");
            (
                DiscoveryStatus::Success,
                old.routes.clone(),
                Vec::new(),
                old.etag.clone(),
                old.last_modified.clone(),
                old.payload_hash.clone(),
                old.fetched_at,
                None,
                0,
            )
        }
        Ok(Ok(page)) => (
            DiscoveryStatus::Success,
            page.routes,
            Vec::new(),
            page.etag,
            page.last_modified,
            Some(page.payload_hash),
            fetched_at,
            None,
            0,
        ),
        Ok(Err(failure))
            if failure.status == DiscoveryStatus::Unsupported
                && prior.is_some_and(|cache| !cache.routes.is_empty()) =>
        {
            return retain_unsupported(identity, fetched_at, prior.unwrap(), failure);
        }
        Ok(Err(failure)) if prior.is_some_and(|cache| !cache.routes.is_empty()) => {
            return retain_last_good(identity, fetched_at, prior.unwrap(), failure);
        }
        Ok(Err(failure)) => (
            failure.status,
            BTreeMap::new(),
            vec![ConfigDiagnostic {
                code: failure.code.into(),
                severity: DiagnosticSeverity::Warning,
                message: failure.message,
                rule_id: None,
            }],
            prior.and_then(|cache| cache.etag.clone()),
            prior.and_then(|cache| cache.last_modified.clone()),
            prior.and_then(|cache| cache.payload_hash.clone()),
            prior.map_or(fetched_at, |cache| cache.fetched_at),
            failure.retry_after_at,
            prior.map_or(1, |cache| cache.consecutive_failures.saturating_add(1)),
        ),
        Err(_) if prior.is_some_and(|cache| !cache.routes.is_empty()) => {
            return retain_last_good(
                identity,
                fetched_at,
                prior.unwrap(),
                failure(
                    DiscoveryStatus::Unavailable,
                    "discovery-deadline",
                    "model discovery exceeded the 30 second aggregate deadline",
                ),
            );
        }
        Err(_) => (
            DiscoveryStatus::Unavailable,
            BTreeMap::new(),
            vec![ConfigDiagnostic {
                code: "discovery-deadline".into(),
                severity: DiagnosticSeverity::Warning,
                message: "model discovery exceeded the 30 second aggregate deadline".into(),
                rule_id: None,
            }],
            prior.and_then(|cache| cache.etag.clone()),
            prior.and_then(|cache| cache.last_modified.clone()),
            prior.and_then(|cache| cache.payload_hash.clone()),
            prior.map_or(fetched_at, |cache| cache.fetched_at),
            None,
            prior.map_or(1, |cache| cache.consecutive_failures.saturating_add(1)),
        ),
    };
    let base_backoff =
        (60_i64 * 2_i64.pow(consecutive_failures.saturating_sub(1).min(6))).min(3_600);
    let jitter = fetched_at.timestamp_subsec_millis() as i64 % (base_backoff / 5 + 1);
    let backoff_seconds = (base_backoff + jitter).min(3_600);
    let expires_at = retry_after_at.unwrap_or_else(|| {
        if status == DiscoveryStatus::Success {
            fetched_at + ttl
        } else {
            fetched_at + Duration::seconds(backoff_seconds)
        }
    });
    let conflicting_observations = if status == DiscoveryStatus::Success {
        collect_conflicts(prior, &routes, payload_hash.as_deref(), fetched_at)
    } else {
        prior
            .map(|document| document.conflicting_observations.clone())
            .unwrap_or_default()
    };
    CatalogCacheDocument {
        schema_version: CATALOG_CACHE_SCHEMA_VERSION,
        identity,
        fetched_at: payload_fetched_at,
        last_refresh_attempt_at: Some(fetched_at),
        expires_at,
        status,
        complete_listing: status == DiscoveryStatus::Success,
        etag,
        last_modified,
        payload_hash,
        retry_after_at,
        consecutive_failures,
        routes,
        diagnostics,
        conflicting_observations,
    }
}

fn retain_unsupported(
    identity: CatalogCacheIdentity,
    attempted_at: chrono::DateTime<Utc>,
    prior: &CatalogCacheDocument,
    failure: Failure,
) -> CatalogCacheDocument {
    let mut retained = prior.clone();
    retained.identity = identity;
    retained.last_refresh_attempt_at = Some(attempted_at);
    retained.expires_at = attempted_at;
    retained.status = DiscoveryStatus::Unsupported;
    retained.complete_listing = false;
    retained.retry_after_at = None;
    retained.consecutive_failures = 0;
    retained.diagnostics.push(ConfigDiagnostic {
        code: failure.code.into(),
        severity: DiagnosticSeverity::Warning,
        message: failure.message,
        rule_id: None,
    });
    retained
}

fn retain_last_good(
    identity: CatalogCacheIdentity,
    attempted_at: chrono::DateTime<Utc>,
    prior: &CatalogCacheDocument,
    failure: Failure,
) -> CatalogCacheDocument {
    let failures = prior.consecutive_failures.saturating_add(1);
    let base = (60_i64 * 2_i64.pow(failures.saturating_sub(1).min(6))).min(3_600);
    let jitter = attempted_at.timestamp_subsec_millis() as i64 % (base / 5 + 1);
    let mut retained = prior.clone();
    retained.identity = identity;
    retained.last_refresh_attempt_at = Some(attempted_at);
    retained.retry_after_at = failure
        .retry_after_at
        .or_else(|| Some(attempted_at + Duration::seconds((base + jitter).min(3_600))));
    retained.consecutive_failures = failures;
    retained.diagnostics.push(ConfigDiagnostic {
        code: failure.code.into(),
        severity: DiagnosticSeverity::Warning,
        message: failure.message,
        rule_id: None,
    });
    retained
}

fn collect_conflicts(
    prior: Option<&CatalogCacheDocument>,
    routes: &BTreeMap<String, DiscoveredModel>,
    payload_hash: Option<&str>,
    observed_at: chrono::DateTime<Utc>,
) -> Vec<CatalogConflictObservation> {
    const MAX_CONFLICTS: usize = 32;
    let Some(prior) = prior else {
        return Vec::new();
    };
    let mut conflicts = prior.conflicting_observations.clone();
    let (Some(previous_hash), Some(new_hash)) = (prior.payload_hash.as_deref(), payload_hash)
    else {
        return conflicts;
    };
    let model_ids = prior
        .routes
        .keys()
        .chain(routes.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for model_id in model_ids {
        let previous_model = prior.routes.get(&model_id);
        let current_model = routes.get(&model_id);
        let (Some(previous), Some(current)) = (previous_model, current_model) else {
            let change_kind = if current_model.is_some() {
                CatalogConflictKind::Added
            } else {
                CatalogConflictKind::Removed
            };
            conflicts.push(CatalogConflictObservation {
                model_id,
                field: "model-membership".into(),
                change_kind,
                previous_value: if previous_model.is_some() {
                    "listed"
                } else {
                    "absent"
                }
                .into(),
                observed_value: if current_model.is_some() {
                    "listed"
                } else {
                    "absent"
                }
                .into(),
                previous_payload_hash: previous_hash.to_string(),
                payload_hash: new_hash.to_string(),
                previous_observed_at: prior.fetched_at,
                observed_at,
            });
            continue;
        };
        let previous = serde_json::to_value(previous).ok();
        let current = serde_json::to_value(current).ok();
        let (Some(Value::Object(previous)), Some(Value::Object(current))) = (previous, current)
        else {
            continue;
        };
        let fields = previous
            .keys()
            .chain(current.keys())
            .filter(|field| field.as_str() != "model_id")
            .cloned()
            .collect::<BTreeSet<_>>();
        for field in fields {
            if field == "model_id" {
                continue;
            }
            let previous_value = previous.get(&field).cloned().unwrap_or(Value::Null);
            let observed_value = current.get(&field).cloned().unwrap_or(Value::Null);
            if previous_value == observed_value {
                continue;
            }
            let change_kind = match (&previous_value, &observed_value) {
                (Value::Null, _) => CatalogConflictKind::Added,
                (_, Value::Null) => CatalogConflictKind::Removed,
                _ => CatalogConflictKind::Changed,
            };
            conflicts.push(CatalogConflictObservation {
                model_id: model_id.clone(),
                field,
                change_kind,
                previous_value: previous_value.to_string(),
                observed_value: observed_value.to_string(),
                previous_payload_hash: previous_hash.to_string(),
                payload_hash: new_hash.to_string(),
                previous_observed_at: prior.fetched_at,
                observed_at,
            });
        }
    }
    if conflicts.len() > MAX_CONFLICTS {
        conflicts.drain(..conflicts.len() - MAX_CONFLICTS);
    }
    conflicts
}

struct Failure {
    status: DiscoveryStatus,
    code: &'static str,
    message: String,
    retry_after_at: Option<chrono::DateTime<Utc>>,
}

struct PageResult {
    routes: BTreeMap<String, DiscoveredModel>,
    etag: Option<String>,
    last_modified: Option<String>,
    payload_hash: String,
    not_modified: bool,
}

async fn fetch_pages(
    client: &Client,
    endpoint: &EndpointDefinition,
    credential: Option<&SecretString>,
    prior_etag: Option<&str>,
    prior_last_modified: Option<&str>,
) -> Result<PageResult, Failure> {
    if endpoint.discovery_adapter == DiscoveryAdapter::None {
        return Err(failure(
            DiscoveryStatus::Unsupported,
            "discovery-unsupported",
            "endpoint has no discovery adapter",
        ));
    }
    let base = endpoint.discovery_base_url.as_deref().ok_or_else(|| {
        failure(
            DiscoveryStatus::Malformed,
            "discovery-url-missing",
            "endpoint has no discovery base URL",
        )
    })?;
    let relative = if endpoint.discovery_adapter == DiscoveryAdapter::OllamaTagsV1 {
        "api/tags"
    } else {
        "models"
    };
    let mut url = join_prefix(base, relative)?;
    if endpoint.discovery_adapter == DiscoveryAdapter::GoogleModelsV1Beta {
        url.query_pairs_mut().append_pair("pageSize", "1000");
    }
    let mut routes = BTreeMap::new();
    let mut aggregate_bytes = 0usize;
    let mut payload_hasher = Sha256::new();
    let mut root_etag = None;
    let mut root_last_modified = None;
    for page in 0..MAX_PAGES {
        let mut request = apply_auth(client.get(url.clone()), endpoint.auth_adapter, credential)?;
        if page == 0
            && let Some(etag) = prior_etag
        {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if page == 0
            && let Some(last_modified) = prior_last_modified
        {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
        }
        let response = request.send().await.map_err(|error| {
            failure(
                DiscoveryStatus::Unavailable,
                "discovery-network",
                error.to_string(),
            )
        })?;
        let status = response.status();
        if status == StatusCode::NOT_MODIFIED {
            if page != 0 {
                return Err(failure(
                    DiscoveryStatus::Malformed,
                    "discovery-pagination-validator",
                    "received an unsolicited 304 after the first discovery page",
                ));
            }
            return Ok(PageResult {
                routes: BTreeMap::new(),
                etag: prior_etag.map(str::to_string),
                last_modified: prior_last_modified.map(str::to_string),
                payload_hash: String::new(),
                not_modified: true,
            });
        }
        let response_etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let response_last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if page == 0 {
            root_etag = response_etag.clone();
            root_last_modified = response_last_modified.clone();
        }
        let retry_after_at = retry_after_time(response.headers().get(reqwest::header::RETRY_AFTER));
        if response
            .content_length()
            .is_some_and(|length| length > MAX_BODY_BYTES as u64)
        {
            return Err(failure(
                DiscoveryStatus::Malformed,
                "discovery-body-limit",
                format!("model discovery response exceeds {MAX_BODY_BYTES} bytes"),
            ));
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            failure(
                DiscoveryStatus::Unavailable,
                "discovery-body",
                error.to_string(),
            )
        })? {
            if aggregate_bytes.saturating_add(chunk.len()) > MAX_BODY_BYTES {
                return Err(failure(
                    DiscoveryStatus::Malformed,
                    "discovery-body-limit",
                    format!("model discovery response exceeds {MAX_BODY_BYTES} bytes"),
                ));
            }
            aggregate_bytes = aggregate_bytes.saturating_add(chunk.len());
            body.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let mut failure = http_failure(status, &body);
            failure.retry_after_at = retry_after_at;
            return Err(failure);
        }
        update_payload_hash(&mut payload_hasher, &body);
        let value: Value = serde_json::from_slice(&body).map_err(|error| {
            failure(
                DiscoveryStatus::Malformed,
                "discovery-json",
                error.to_string(),
            )
        })?;
        parse_models(endpoint.discovery_adapter, &value, &mut routes)?;
        if routes.len() > MAX_MODELS {
            return Err(failure(
                DiscoveryStatus::Malformed,
                "discovery-model-limit",
                format!("model discovery exceeds {MAX_MODELS} models"),
            ));
        }
        let Some(token) = next_page(endpoint.discovery_adapter, &value)? else {
            return Ok(PageResult {
                routes,
                etag: root_etag,
                last_modified: root_last_modified,
                payload_hash: finalize_payload_hash(payload_hasher),
                not_modified: false,
            });
        };
        if page + 1 == MAX_PAGES {
            return Err(failure(
                DiscoveryStatus::Malformed,
                "discovery-pagination-limit",
                format!("model discovery exceeded {MAX_PAGES} pages"),
            ));
        }
        let field = match endpoint.discovery_adapter {
            DiscoveryAdapter::AnthropicModelsV1 => "after_id",
            DiscoveryAdapter::GoogleModelsV1Beta => "pageToken",
            _ => {
                return Ok(PageResult {
                    routes,
                    etag: root_etag,
                    last_modified: root_last_modified,
                    payload_hash: finalize_payload_hash(payload_hasher),
                    not_modified: false,
                });
            }
        };
        {
            let mut pairs = url.query_pairs_mut();
            pairs.clear();
            if endpoint.discovery_adapter == DiscoveryAdapter::GoogleModelsV1Beta {
                pairs.append_pair("pageSize", "1000");
            }
            pairs.append_pair(field, &token);
        }
    }
    Err(failure(
        DiscoveryStatus::Malformed,
        "discovery-pagination-limit",
        format!("model discovery exceeded {MAX_PAGES} pages"),
    ))
}

fn apply_auth(
    request: reqwest::RequestBuilder,
    auth: AuthAdapter,
    credential: Option<&SecretString>,
) -> Result<reqwest::RequestBuilder, Failure> {
    let key = credential.map(SecretString::expose);
    let missing = || {
        failure(
            DiscoveryStatus::AuthenticationFailed,
            "discovery-credential-missing",
            "model discovery requires a credential",
        )
    };
    match auth {
        AuthAdapter::None => Ok(request),
        AuthAdapter::Bearer => key.map(|key| request.bearer_auth(key)).ok_or_else(missing),
        AuthAdapter::AnthropicKey => key
            .map(|key| {
                request
                    .header("x-api-key", key)
                    .header("anthropic-version", "2023-06-01")
            })
            .ok_or_else(missing),
        AuthAdapter::GoogleKey => key
            .map(|key| request.header("x-goog-api-key", key))
            .ok_or_else(missing),
    }
}

fn finalize_payload_hash(hasher: Sha256) -> String {
    use std::fmt::Write as _;
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        })
}

fn update_payload_hash(hasher: &mut Sha256, body: &[u8]) {
    hasher.update((body.len() as u64).to_be_bytes());
    hasher.update(body);
}

fn join_prefix(base: &str, relative: &str) -> Result<Url, Failure> {
    Url::parse(&format!("{}/", base.trim_end_matches('/')))
        .and_then(|url| url.join(relative))
        .map_err(|error| {
            failure(
                DiscoveryStatus::Malformed,
                "discovery-url-invalid",
                error.to_string(),
            )
        })
}

fn parse_models(
    adapter: DiscoveryAdapter,
    value: &Value,
    output: &mut BTreeMap<String, DiscoveredModel>,
) -> Result<(), Failure> {
    let array_field = match adapter {
        DiscoveryAdapter::GoogleModelsV1Beta | DiscoveryAdapter::OllamaTagsV1 => "models",
        _ => "data",
    };
    let models = value
        .get(array_field)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            failure(
                DiscoveryStatus::Malformed,
                "discovery-shape",
                "model discovery response omitted its model array",
            )
        })?;
    for value in models {
        let id_field = if matches!(
            adapter,
            DiscoveryAdapter::OllamaTagsV1 | DiscoveryAdapter::GoogleModelsV1Beta
        ) {
            "name"
        } else {
            "id"
        };
        let raw_id = value.get(id_field).and_then(Value::as_str).ok_or_else(|| {
            failure(
                DiscoveryStatus::Malformed,
                "discovery-model-id",
                "model discovery item omitted its string id",
            )
        })?;
        let model_id = raw_id.strip_prefix("models/").unwrap_or(raw_id).to_string();
        let mut discovered = DiscoveredModel {
            model_id,
            ..Default::default()
        };
        match adapter {
            DiscoveryAdapter::OpenrouterModelsV1 => {
                discovered.display_name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                discovered.max_input_tokens = value.get("context_length").and_then(Value::as_u64);
                discovered.max_output_tokens = value
                    .pointer("/top_provider/max_completion_tokens")
                    .and_then(Value::as_u64);
            }
            DiscoveryAdapter::AnthropicModelsV1 => {
                discovered.display_name = value
                    .get("display_name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                discovered.max_input_tokens = value.get("max_input_tokens").and_then(Value::as_u64);
                discovered.max_output_tokens = value.get("max_tokens").and_then(Value::as_u64);
            }
            DiscoveryAdapter::GoogleModelsV1Beta => {
                discovered.display_name = value
                    .get("displayName")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                discovered.max_input_tokens = value.get("inputTokenLimit").and_then(Value::as_u64);
                discovered.max_output_tokens =
                    value.get("outputTokenLimit").and_then(Value::as_u64);
            }
            // The standard OpenAI Models object and Ollama tags listing are
            // identity/availability authorities only. Extension fields from
            // a gateway must never acquire capability authority by spelling.
            DiscoveryAdapter::OpenaiModelsV1
            | DiscoveryAdapter::OllamaTagsV1
            | DiscoveryAdapter::None => {}
        }
        parse_capabilities(adapter, value, &mut discovered);
        output.insert(discovered.model_id.clone(), discovered);
    }
    Ok(())
}

/// Normalize only documented, explicit metadata. Missing fields remain
/// unknown; names and provider-wide assumptions never grant capabilities.
fn parse_capabilities(adapter: DiscoveryAdapter, value: &Value, model: &mut DiscoveredModel) {
    match adapter {
        DiscoveryAdapter::OpenrouterModelsV1 => {
            model.input_modalities = string_set(value.pointer("/architecture/input_modalities"));
            model.output_modalities = string_set(value.pointer("/architecture/output_modalities"));
            if let Some(parameters) = string_set(value.get("supported_parameters")) {
                model.supports_temperature = Some(parameters.contains("temperature"));
                model.supports_frequency_penalty = Some(parameters.contains("frequency_penalty"));
                model.output_contracts = Some(OutputContractCapabilities {
                    prompt_validated_json: true,
                    native_json_object: parameters.contains("response_format"),
                    native_json_schema: parameters.contains("structured_outputs")
                        || parameters.contains("structured_output"),
                });
            }
            // `reasoning` in supported_parameters proves only the presence of
            // a control, not its exact effort levels, budget, or off encoding.
        }
        DiscoveryAdapter::GoogleModelsV1Beta
            if value.get("thinking").and_then(Value::as_bool) == Some(false) =>
        {
            // The v1beta Model schema exposes only a coarse `thinking` flag.
            // `false` can narrow authored reasoning; `true` cannot prove any
            // exact Interactions thinking level or off encoding.
            model.reasoning_effort_levels = Some(BTreeSet::new());
            model.supports_reasoning_budget = Some(false);
        }
        DiscoveryAdapter::GoogleModelsV1Beta => {}
        DiscoveryAdapter::AnthropicModelsV1 => {
            let levels = [
                ("low", ReasoningEffortV2::Low),
                ("medium", ReasoningEffortV2::Medium),
                ("high", ReasoningEffortV2::High),
                ("xhigh", ReasoningEffortV2::Xhigh),
                ("max", ReasoningEffortV2::Max),
            ];
            let thinking_supported = value
                .pointer("/capabilities/thinking/supported")
                .and_then(Value::as_bool);
            let adaptive_supported = value
                .pointer("/capabilities/thinking/types/adaptive/supported")
                .and_then(Value::as_bool);
            let effort_supported = value
                .pointer("/capabilities/effort/supported")
                .and_then(Value::as_bool);
            if thinking_supported == Some(false)
                || adaptive_supported == Some(false)
                || effort_supported == Some(false)
            {
                model.reasoning_effort_levels = Some(BTreeSet::new());
            } else if thinking_supported == Some(true)
                && adaptive_supported == Some(true)
                && effort_supported == Some(true)
            {
                model.reasoning_effort_levels = Some(
                    levels
                        .into_iter()
                        .filter(|(name, _)| {
                            value
                                .pointer(&format!("/capabilities/effort/{name}/supported"))
                                .and_then(Value::as_bool)
                                == Some(true)
                        })
                        .map(|(_, level)| level)
                        .collect(),
                );
            }
            let enabled_supported = value
                .pointer("/capabilities/thinking/types/enabled/supported")
                .and_then(Value::as_bool);
            model.supports_reasoning_budget =
                if thinking_supported == Some(false) || enabled_supported == Some(false) {
                    Some(false)
                } else if thinking_supported == Some(true) && enabled_supported == Some(true) {
                    Some(true)
                } else {
                    None
                };
            if let Some(native) = value
                .pointer("/capabilities/structured_outputs/supported")
                .and_then(Value::as_bool)
            {
                model.output_contracts = Some(OutputContractCapabilities {
                    prompt_validated_json: true,
                    native_json_object: native,
                    native_json_schema: native,
                });
            }
        }
        _ => {}
    }
}

fn string_set(value: Option<&Value>) -> Option<BTreeSet<String>> {
    let values = value?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(Value::as_str)
            .map(|item| item.to_ascii_lowercase())
            .collect(),
    )
}

fn next_page(adapter: DiscoveryAdapter, value: &Value) -> Result<Option<String>, Failure> {
    match adapter {
        DiscoveryAdapter::AnthropicModelsV1
            if value.get("has_more").and_then(Value::as_bool) == Some(true) =>
        {
            value
                .get("last_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .map(Some)
                .ok_or_else(|| {
                    failure(
                        DiscoveryStatus::Malformed,
                        "discovery-pagination-evidence",
                        "Anthropic discovery reported has_more=true without a non-empty last_id",
                    )
                })
        }
        DiscoveryAdapter::GoogleModelsV1Beta => Ok(value
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string)),
        _ => Ok(None),
    }
}

fn http_failure(status: StatusCode, body: &[u8]) -> Failure {
    let kind = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => DiscoveryStatus::AuthenticationFailed,
        StatusCode::TOO_MANY_REQUESTS => DiscoveryStatus::RateLimited,
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => DiscoveryStatus::Unsupported,
        _ => DiscoveryStatus::Unavailable,
    };
    failure(
        kind,
        "discovery-http",
        format!(
            "HTTP {status}: {}",
            String::from_utf8_lossy(body)
                .chars()
                .take(512)
                .collect::<String>()
        ),
    )
}

fn failure(status: DiscoveryStatus, code: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        status,
        code,
        message: message.into(),
        retry_after_at: None,
    }
}

fn retry_after_time(value: Option<&reqwest::header::HeaderValue>) -> Option<chrono::DateTime<Utc>> {
    let value = value?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<i64>() {
        return Some(Utc::now() + Duration::seconds(seconds.max(0)));
    }
    chrono::DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_config::{BackendKind, InferenceAdapter, ManagementAdapter, ReasoningDialect};

    fn endpoint(base: String) -> EndpointDefinition {
        EndpointDefinition {
            inference_base_url: base.clone(),
            discovery_base_url: Some(base),
            inference_adapter: InferenceAdapter::OpenaiChatV1,
            discovery_adapter: DiscoveryAdapter::OpenaiModelsV1,
            backend_kind: BackendKind::Remote,
            management_adapter: ManagementAdapter::None,
            auth_adapter: AuthAdapter::None,
            credential_slot: None,
            default_reasoning_dialect: ReasoningDialect::None,
            default_openai_generation_wire: None,
        }
    }

    fn identity(base: &str) -> CatalogCacheIdentity {
        CatalogCacheIdentity {
            provider_id: "test".into(),
            endpoint_id: "default".into(),
            discovery_base_url: base.into(),
            inference_base_url: base.into(),
            inference_adapter_version: "openai-chat-v1@1".into(),
            discovery_adapter_version: "openai-models-v1@1".into(),
            credential_fingerprint: "none".into(),
        }
    }

    #[tokio::test]
    async fn openai_list_is_normalized() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":[{"id":"model-a"}]})),
            )
            .mount(&server)
            .await;
        let base = format!("{}/v1", server.uri());
        let result = fetch_catalog_endpoint(
            identity(&base),
            &endpoint(base.clone()),
            None,
            Duration::minutes(5),
            None,
        )
        .await;
        assert_eq!(result.status, DiscoveryStatus::Success);
        assert!(result.routes.contains_key("model-a"));
    }

    #[tokio::test]
    async fn malformed_is_not_empty_success() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"unexpected":[]})),
            )
            .mount(&server)
            .await;
        let base = format!("{}/v1", server.uri());
        let result = fetch_catalog_endpoint(
            identity(&base),
            &endpoint(base),
            None,
            Duration::minutes(5),
            None,
        )
        .await;
        assert_eq!(result.status, DiscoveryStatus::Malformed);
    }

    #[tokio::test]
    async fn last_modified_revalidates_and_unsupported_retains_stale_routes() {
        use wiremock::matchers::path;
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT")
                    .set_body_json(serde_json::json!({"data":[{"id":"model-a"}]})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let base = format!("{}/v1", server.uri());
        let first = fetch_catalog_endpoint(
            identity(&base),
            &endpoint(base.clone()),
            None,
            Duration::minutes(5),
            None,
        )
        .await;
        assert_eq!(
            first.last_modified.as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );

        server.reset().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(wiremock::ResponseTemplate::new(304))
            .expect(1)
            .mount(&server)
            .await;
        let revalidated = fetch_catalog_endpoint(
            identity(&base),
            &endpoint(base.clone()),
            None,
            Duration::minutes(5),
            Some(&first),
        )
        .await;
        assert_eq!(revalidated.status, DiscoveryStatus::Success);
        assert!(revalidated.routes.contains_key("model-a"));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(
            requests[0]
                .headers
                .get("if-modified-since")
                .and_then(|value| value.to_str().ok()),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );

        server.reset().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        let unsupported = fetch_catalog_endpoint(
            identity(&base),
            &endpoint(base.clone()),
            None,
            Duration::minutes(5),
            Some(&revalidated),
        )
        .await;
        assert_eq!(unsupported.status, DiscoveryStatus::Unsupported);
        assert!(!unsupported.complete_listing);
        assert!(unsupported.routes.contains_key("model-a"));
        assert!(unsupported.retry_after_at.is_none());

        server.reset().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        let unsupported_again = fetch_catalog_endpoint(
            identity(&base),
            &endpoint(base.clone()),
            None,
            Duration::minutes(5),
            Some(&unsupported),
        )
        .await;
        assert_eq!(unsupported_again.status, DiscoveryStatus::Unsupported);
        assert!(unsupported_again.routes.contains_key("model-a"));
        assert_eq!(unsupported_again.fetched_at, first.fetched_at);

        server.reset().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(wiremock::ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;
        let transient = fetch_catalog_endpoint(
            identity(&base),
            &endpoint(base),
            None,
            Duration::minutes(5),
            Some(&unsupported_again),
        )
        .await;
        assert!(transient.routes.contains_key("model-a"));
        assert_eq!(transient.fetched_at, first.fetched_at);
    }

    #[tokio::test]
    async fn pagination_keeps_root_validators_and_hashes_ordered_pages() {
        use wiremock::matchers::{path, query_param, query_param_is_missing};
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(path("/v1/models"))
            .and(query_param_is_missing("after_id"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("etag", "root-tag")
                    .insert_header("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT")
                    .set_body_json(serde_json::json!({
                        "data":[{"id":"model-a"}],"has_more":true,"last_id":"cursor"
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(path("/v1/models"))
            .and(query_param("after_id", "cursor"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("etag", "tail-tag")
                    .insert_header("last-modified", "Thu, 22 Oct 2015 07:28:00 GMT")
                    .set_body_json(serde_json::json!({
                        "data":[{"id":"model-b"}],"has_more":false
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let base = format!("{}/v1", server.uri());
        let mut endpoint = endpoint(base.clone());
        endpoint.discovery_adapter = DiscoveryAdapter::AnthropicModelsV1;
        let result =
            fetch_catalog_endpoint(identity(&base), &endpoint, None, Duration::minutes(5), None)
                .await;
        assert_eq!(result.etag.as_deref(), Some("root-tag"));
        assert_eq!(
            result.last_modified.as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );
        assert_eq!(result.routes.len(), 2);
        assert_eq!(result.payload_hash.as_deref().map(str::len), Some(64));

        let mut forward = Sha256::new();
        update_payload_hash(&mut forward, b"first");
        update_payload_hash(&mut forward, b"second");
        let mut reverse = Sha256::new();
        update_payload_hash(&mut reverse, b"second");
        update_payload_hash(&mut reverse, b"first");
        assert_ne!(
            finalize_payload_hash(forward),
            finalize_payload_hash(reverse)
        );
    }

    #[tokio::test]
    async fn changed_remote_fields_retain_bounded_payload_conflict_evidence() {
        use wiremock::matchers::path;
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"data":[{"id":"model-a","context_length":100}]}),
                ),
            )
            .mount(&server)
            .await;
        let base = format!("{}/v1", server.uri());
        let mut endpoint = endpoint(base.clone());
        endpoint.discovery_adapter = DiscoveryAdapter::OpenrouterModelsV1;
        let first =
            fetch_catalog_endpoint(identity(&base), &endpoint, None, Duration::minutes(5), None)
                .await;
        server.reset().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"data":[{"id":"model-a","context_length":200}]}),
                ),
            )
            .mount(&server)
            .await;
        let second = fetch_catalog_endpoint(
            identity(&base),
            &endpoint,
            None,
            Duration::minutes(5),
            Some(&first),
        )
        .await;
        assert_ne!(first.payload_hash, second.payload_hash);
        assert!(second.conflicting_observations.iter().any(|observation| {
            observation.model_id == "model-a"
                && observation.field == "max_input_tokens"
                && Some(observation.previous_payload_hash.as_str()) == first.payload_hash.as_deref()
                && Some(observation.payload_hash.as_str()) == second.payload_hash.as_deref()
                && observation.previous_observed_at == first.fetched_at
                && observation.change_kind == CatalogConflictKind::Changed
        }));

        server.reset().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":[{"id":"model-a"},{"id":"model-b"}]})),
            )
            .mount(&server)
            .await;
        let third = fetch_catalog_endpoint(
            identity(&base),
            &endpoint,
            None,
            Duration::minutes(5),
            Some(&second),
        )
        .await;
        assert!(third.conflicting_observations.iter().any(|observation| {
            observation.model_id == "model-a"
                && observation.field == "max_input_tokens"
                && observation.change_kind == CatalogConflictKind::Removed
        }));
        assert!(third.conflicting_observations.iter().any(|observation| {
            observation.model_id == "model-b"
                && observation.field == "model-membership"
                && observation.change_kind == CatalogConflictKind::Added
        }));

        server.reset().await;
        wiremock::Mock::given(path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":[{"id":"model-a"}]})),
            )
            .mount(&server)
            .await;
        let fourth = fetch_catalog_endpoint(
            identity(&base),
            &endpoint,
            None,
            Duration::minutes(5),
            Some(&third),
        )
        .await;
        assert!(fourth.conflicting_observations.iter().any(|observation| {
            observation.model_id == "model-b"
                && observation.field == "model-membership"
                && observation.change_kind == CatalogConflictKind::Removed
        }));
    }

    #[test]
    fn anthropic_has_more_requires_pagination_evidence() {
        let error = next_page(
            DiscoveryAdapter::AnthropicModelsV1,
            &serde_json::json!({"data": [], "has_more": true}),
        )
        .unwrap_err();
        assert_eq!(error.status, DiscoveryStatus::Malformed);
        assert_eq!(error.code, "discovery-pagination-evidence");
    }

    #[test]
    fn openrouter_explicit_metadata_is_normalized_without_name_inference() {
        let mut output = BTreeMap::new();
        assert!(parse_models(
            DiscoveryAdapter::OpenrouterModelsV1,
            &serde_json::json!({"data":[{
                "id":"vendor/opaque-model",
                "supported_parameters":["temperature","response_format","structured_outputs"],
                "architecture":{"input_modalities":["text","image"],"output_modalities":["text"]}
            }]}),
            &mut output,
        )
        .is_ok());
        let model = &output["vendor/opaque-model"];
        assert_eq!(model.supports_temperature, Some(true));
        assert_eq!(model.supports_frequency_penalty, Some(false));
        assert_eq!(model.streaming, None);
        assert!(model.input_modalities.as_ref().unwrap().contains("image"));
        let outputs = model.output_contracts.as_ref().unwrap();
        assert!(outputs.prompt_validated_json);
        assert!(outputs.native_json_object && outputs.native_json_schema);
        assert!(model.reasoning_effort_levels.is_none());
    }

    #[test]
    fn google_and_anthropic_explicit_constraints_are_normalized() {
        let mut google = BTreeMap::new();
        assert!(
            parse_models(
                DiscoveryAdapter::GoogleModelsV1Beta,
                &serde_json::json!({"models":[{
                "name":"models/opaque",
                "displayName":"Opaque",
                "inputTokenLimit":1234,
                "outputTokenLimit":567,
                "supportedGenerationMethods":["generateContent"],
                "thinking":false
                }]}),
                &mut google,
            )
            .is_ok()
        );
        let google = &google["opaque"];
        assert_eq!(google.supports_reasoning_budget, Some(false));
        assert!(google.reasoning_effort_levels.as_ref().unwrap().is_empty());
        assert_eq!(google.max_output_tokens, Some(567));
        assert!(google.service_tiers.is_none());
        assert!(google.output_contracts.is_none());

        let mut anthropic = BTreeMap::new();
        assert!(parse_models(
            DiscoveryAdapter::AnthropicModelsV1,
            &serde_json::json!({"data":[{
                "id":"claude-opaque",
                "display_name":"Claude Opaque",
                "max_input_tokens":200000,
                "max_tokens":64000,
                "capabilities":{
                    "thinking":{"supported":true,"types":{"adaptive":{"supported":true},"enabled":{"supported":false}}},
                    "effort":{"supported":true,"low":{"supported":true},"medium":{"supported":true},"high":{"supported":false},"xhigh":{"supported":false},"max":{"supported":false}},
                    "structured_outputs":{"supported":false}
                }
            }]}),
            &mut anthropic,
        )
        .is_ok());
        let anthropic = &anthropic["claude-opaque"];
        assert_eq!(anthropic.supports_reasoning_budget, Some(false));
        assert!(anthropic.supports_reasoning_off.is_none());
        assert_eq!(anthropic.max_output_tokens, Some(64_000));
        assert_eq!(
            anthropic.reasoning_effort_levels.as_ref().unwrap(),
            &BTreeSet::from([ReasoningEffortV2::Low, ReasoningEffortV2::Medium])
        );
        assert!(
            anthropic
                .output_contracts
                .as_ref()
                .unwrap()
                .prompt_validated_json
        );

        let mut false_parent = BTreeMap::new();
        assert!(parse_models(
            DiscoveryAdapter::AnthropicModelsV1,
            &serde_json::json!({"data":[{
                "id":"claude-parent-false",
                "capabilities":{
                    "thinking":{"supported":false,"types":{"adaptive":{"supported":true},"enabled":{"supported":true}}},
                    "effort":{"supported":true,"low":{"supported":true}},
                    "structured_outputs":{"supported":true}
                }
            }]}),
            &mut false_parent,
        )
        .is_ok());
        let false_parent = &false_parent["claude-parent-false"];
        assert!(
            false_parent
                .reasoning_effort_levels
                .as_ref()
                .unwrap()
                .is_empty()
        );
        assert_eq!(false_parent.supports_reasoning_budget, Some(false));

        let mut adaptive_false = BTreeMap::new();
        assert!(parse_models(
            DiscoveryAdapter::AnthropicModelsV1,
            &serde_json::json!({"data":[{
                "id":"claude-adaptive-false",
                "capabilities":{
                    "thinking":{"supported":true,"types":{"adaptive":{"supported":false},"enabled":{"supported":true}}},
                    "effort":{"supported":true,"low":{"supported":true}}
                }
            }]}),
            &mut adaptive_false,
        )
        .is_ok());
        assert!(
            adaptive_false["claude-adaptive-false"]
                .reasoning_effort_levels
                .as_ref()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn generic_openai_and_ollama_extension_fields_have_no_capability_authority() {
        for (adapter, root, id_field) in [
            (DiscoveryAdapter::OpenaiModelsV1, "data", "id"),
            (DiscoveryAdapter::OllamaTagsV1, "models", "name"),
        ] {
            let mut output = BTreeMap::new();
            let mut item = serde_json::json!({
                "display_name":"hostile",
                "context_length":1,
                "max_output_tokens":1,
                "inputModalities":["image"],
                "supports_streaming":false,
                "supported_parameters":[]
            });
            item[id_field] = serde_json::json!("opaque");
            let mut document = serde_json::Map::new();
            document.insert(root.into(), serde_json::json!([item]));
            assert!(parse_models(adapter, &Value::Object(document), &mut output).is_ok());
            let model = &output["opaque"];
            assert!(model.display_name.is_none());
            assert!(model.max_input_tokens.is_none());
            assert!(model.max_output_tokens.is_none());
            assert!(model.input_modalities.is_none());
            assert!(model.streaming.is_none());
            assert!(model.output_contracts.is_none());
        }
    }
}
