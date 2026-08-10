//! Live "is this provider config usable?" probe for BYOK onboarding.
//!
//! Sends a tiny request to the configured endpoint and maps the response to a
//! structured `ValidationOutcome` so the UI can render auth / network /
//! not-found / rate-limit errors with specific copy. Costs ~$0 on Anthropic
//! (1-token completion) and zero on every other provider (`/v1/models` GET).

use std::time::Duration;

use parish_config::Provider;
use serde::{Deserialize, Serialize};

/// Default timeout for a single validation probe. Generous enough for cold
/// cloud endpoints, tight enough that users don't sit waiting.
const VALIDATE_TIMEOUT: Duration = Duration::from_secs(8);

/// Anthropic API version header value. Matches `anthropic_client.rs`.
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidationOutcome {
    Ok,
    /// 401/403 — key missing, wrong, or revoked.
    AuthFailed {
        status: u16,
        body_excerpt: String,
    },
    /// 404 — `/v1/models` (or equivalent) doesn't exist at this base URL.
    /// Usually means the user pasted a wrong base URL.
    NotFound {
        status: u16,
        body_excerpt: String,
    },
    /// 429 — too many calls during validation. Debounce in the UI.
    RateLimited {
        status: u16,
        retry_after_secs: Option<u64>,
    },
    /// Connection refused, DNS failure, TLS error, timeout.
    Network {
        message: String,
    },
    /// Any other non-2xx that doesn't fit the buckets above.
    Unexpected {
        status: u16,
        body_excerpt: String,
    },
}

impl ValidationOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, ValidationOutcome::Ok)
    }
}

/// Probes the provider with a minimal request and returns a structured outcome.
///
/// `base_url` should be the user-supplied base (without a trailing slash);
/// per-provider path suffixes are appended internally.
pub async fn validate(
    provider: &Provider,
    base_url: &str,
    api_key: Option<&str>,
) -> ValidationOutcome {
    let client = match reqwest::Client::builder().timeout(VALIDATE_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            return ValidationOutcome::Network {
                message: format!("build http client: {e}"),
            };
        }
    };

    use parish_config::ProviderKind;
    match provider.kind() {
        ProviderKind::Simulator => ValidationOutcome::Ok,
        ProviderKind::Local if provider.id() == "ollama" => {
            probe_get(
                &client,
                &format!("{}/api/tags", base_url.trim_end_matches('/')),
                None,
            )
            .await
        }
        ProviderKind::Anthropic => probe_anthropic(&client, base_url, api_key).await,
        ProviderKind::Google => probe_google(&client, base_url, api_key).await,
        // GitHub Models uses /chat/completions (no /v1 prefix) without a /v1 base path.
        _ if provider.id() == "github_models" => {
            probe_github_models(&client, base_url, api_key).await
        }
        // Every other provider speaks OpenAI-compatible /v1.
        _ => probe_openai_compat(&client, base_url, api_key).await,
    }
}

async fn probe_google(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> ValidationOutcome {
    let trimmed = base_url.trim_end_matches('/');
    let models_url = if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    };
    let mut request = client.get(models_url);
    if let Some(key) = api_key {
        request = request.header("x-goog-api-key", key);
    }
    classify_response(request.send().await).await
}

async fn probe_openai_compat(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> ValidationOutcome {
    let trimmed = base_url.trim_end_matches('/');
    // Normalize trailing /v1 so callers can pass either form; mirrors ClientBase::new.
    let trimmed = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
    let models_url = format!("{trimmed}/v1/models");
    let outcome = probe_get(client, &models_url, api_key).await;
    // Some self-hosted OpenAI-compat servers (older vLLM, certain TGI builds)
    // don't expose /v1/models. Fall back to a 1-token chat completion so we
    // can still distinguish auth from URL errors.
    if matches!(outcome, ValidationOutcome::NotFound { .. }) {
        return probe_openai_chat_min(client, base_url, api_key).await;
    }
    outcome
}

async fn probe_get(
    client: &reqwest::Client,
    url: &str,
    api_key: Option<&str>,
) -> ValidationOutcome {
    let mut req = client.get(url);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }
    classify_response(req.send().await).await
}

async fn probe_openai_chat_min(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> ValidationOutcome {
    let trimmed = base_url.trim_end_matches('/');
    let trimmed = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
    let url = format!("{trimmed}/v1/chat/completions");
    let body = serde_json::json!({
        "model": "validation-probe",
        "messages": [{ "role": "user", "content": "hi" }],
        "max_tokens": 1,
        "stream": false,
    });
    let mut req = client.post(&url).json(&body);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }
    classify_response(req.send().await).await
}

async fn probe_github_models(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> ValidationOutcome {
    // Prefer the models listing endpoint — works regardless of org-level
    // model allowlists and doesn't require picking a specific model name.
    let trimmed = base_url.trim_end_matches('/');
    let models_url = format!("{trimmed}/models");
    let outcome = probe_get(client, &models_url, api_key).await;
    if !matches!(outcome, ValidationOutcome::NotFound { .. }) {
        return outcome;
    }
    // Fall back to a minimal chat completion if /models returns 404.
    let chat_url = format!("{trimmed}/chat/completions");
    let body = serde_json::json!({
        "model": "microsoft/Phi-4",
        "messages": [{ "role": "user", "content": "hi" }],
        "max_tokens": 1,
        "stream": false,
    });
    let mut req = client.post(&chat_url).json(&body);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }
    classify_response(req.send().await).await
}

async fn probe_anthropic(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> ValidationOutcome {
    let Some(key) = api_key else {
        return ValidationOutcome::AuthFailed {
            status: 401,
            body_excerpt: "missing api key".to_string(),
        };
    };
    let trimmed = base_url.trim_end_matches('/');
    let url = format!("{trimmed}/v1/messages");
    let body = serde_json::json!({
        "model": "claude-haiku-4-5",
        "max_tokens": 1,
        "messages": [{ "role": "user", "content": "hi" }],
    });
    let resp = client
        .post(&url)
        .header("x-api-key", key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .await;
    classify_response(resp).await
}

async fn classify_response(result: Result<reqwest::Response, reqwest::Error>) -> ValidationOutcome {
    let resp = match result {
        Ok(r) => r,
        Err(e) => {
            return ValidationOutcome::Network {
                message: e.to_string(),
            };
        }
    };
    let status = resp.status();
    if status.is_success() {
        return ValidationOutcome::Ok;
    }
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let code = status.as_u16();
    let body_excerpt = body_excerpt(resp).await;
    match code {
        401 | 403 => ValidationOutcome::AuthFailed {
            status: code,
            body_excerpt,
        },
        404 => ValidationOutcome::NotFound {
            status: code,
            body_excerpt,
        },
        429 => ValidationOutcome::RateLimited {
            status: code,
            retry_after_secs: retry_after,
        },
        _ => ValidationOutcome::Unexpected {
            status: code,
            body_excerpt,
        },
    }
}

async fn body_excerpt(resp: reqwest::Response) -> String {
    const MAX_LEN: usize = 240;
    let body = resp.text().await.unwrap_or_default();
    let trimmed = body.trim();
    if trimmed.len() <= MAX_LEN {
        trimmed.to_string()
    } else {
        let cut = trimmed
            .char_indices()
            .nth(MAX_LEN)
            .map(|(i, _)| i)
            .unwrap_or(MAX_LEN);
        format!("{}…", &trimmed[..cut])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_config::Provider;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn simulator_short_circuits_ok() {
        let outcome = validate(&Provider::simulator(), "http://nowhere.invalid", None).await;
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn openai_validate_returns_ok_on_200_models() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer sk-good"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"data\":[]}"))
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("openai").expect("openai provider mod must be loaded"),
            &server.uri(),
            Some("sk-good"),
        )
        .await;
        assert_eq!(outcome, ValidationOutcome::Ok);
    }

    #[tokio::test]
    async fn openai_validate_maps_401_to_auth_failed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid_api_key"))
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("openai").expect("openai provider mod must be loaded"),
            &server.uri(),
            Some("sk-bad"),
        )
        .await;
        match outcome {
            ValidationOutcome::AuthFailed {
                status,
                body_excerpt,
            } => {
                assert_eq!(status, 401);
                assert!(body_excerpt.contains("invalid_api_key"));
            }
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn openai_validate_maps_429_to_rate_limited_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "30")
                    .set_body_string("rate limited"),
            )
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("openai").expect("openai provider mod must be loaded"),
            &server.uri(),
            Some("sk"),
        )
        .await;
        assert_eq!(
            outcome,
            ValidationOutcome::RateLimited {
                status: 429,
                retry_after_secs: Some(30),
            }
        );
    }

    #[tokio::test]
    async fn openai_validate_404_falls_back_to_chat_completions_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let outcome = validate(&Provider::vllmmlx(), &server.uri(), None).await;
        assert_eq!(outcome, ValidationOutcome::Ok);
    }

    #[tokio::test]
    async fn openai_validate_404_fallback_propagates_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let outcome = validate(&Provider::custom(), &server.uri(), Some("sk-bad")).await;
        match outcome {
            ValidationOutcome::AuthFailed { status, .. } => assert_eq!(status, 401),
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn github_models_validate_probes_models_endpoint_not_v1() {
        let server = MockServer::start().await;
        // Primary probe: GET /models (no specific model required)
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("authorization", "Bearer ghp_token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("github_models").expect("github_models provider mod must be loaded"),
            &server.uri(),
            Some("ghp_token"),
        )
        .await;
        assert_eq!(outcome, ValidationOutcome::Ok);
    }

    #[tokio::test]
    async fn github_models_validate_falls_back_to_chat_when_models_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer ghp_token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("github_models").expect("github_models provider mod must be loaded"),
            &server.uri(),
            Some("ghp_token"),
        )
        .await;
        assert_eq!(outcome, ValidationOutcome::Ok);
    }

    #[tokio::test]
    async fn github_models_validate_maps_401_to_auth_failed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("github_models").expect("github_models provider mod must be loaded"),
            &server.uri(),
            Some("ghp_bad"),
        )
        .await;
        match outcome {
            ValidationOutcome::AuthFailed { status, .. } => assert_eq!(status, 401),
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn anthropic_validate_returns_ok_on_200_messages() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-ant-good"))
            .and(header("anthropic-version", ANTHROPIC_VERSION))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("{\"content\":[{\"text\":\"hi\"}]}"),
            )
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("anthropic").expect("anthropic provider mod must be loaded"),
            &server.uri(),
            Some("sk-ant-good"),
        )
        .await;
        assert_eq!(outcome, ValidationOutcome::Ok);
    }

    #[tokio::test]
    async fn anthropic_validate_maps_401_to_auth_failed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid"))
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("anthropic").expect("anthropic provider mod must be loaded"),
            &server.uri(),
            Some("sk-ant-bad"),
        )
        .await;
        match outcome {
            ValidationOutcome::AuthFailed { status, .. } => assert_eq!(status, 401),
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn anthropic_validate_no_key_short_circuits_auth_failed() {
        let outcome = validate(
            &Provider::from_id("anthropic").expect("anthropic provider mod must be loaded"),
            "https://nowhere.invalid",
            None,
        )
        .await;
        match outcome {
            ValidationOutcome::AuthFailed { .. } => {}
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ollama_validate_returns_ok_on_200_tags() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"models\":[]}"))
            .mount(&server)
            .await;

        let outcome = validate(&Provider::ollama(), &server.uri(), None).await;
        assert_eq!(outcome, ValidationOutcome::Ok);
    }

    #[tokio::test]
    async fn validate_network_error_when_unreachable() {
        // Port 1 is reserved + unbindable; reqwest will fail to connect.
        let outcome = validate(
            &Provider::from_id("openai").expect("openai provider mod must be loaded"),
            "http://127.0.0.1:1",
            Some("sk"),
        )
        .await;
        match outcome {
            ValidationOutcome::Network { .. } => {}
            other => panic!("expected Network, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn validate_unexpected_500_is_unexpected_variant() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(500).set_body_string("oops"))
            .mount(&server)
            .await;

        let outcome = validate(
            &Provider::from_id("groq").expect("groq provider mod must be loaded"),
            &server.uri(),
            Some("gsk"),
        )
        .await;
        match outcome {
            ValidationOutcome::Unexpected { status, .. } => assert_eq!(status, 500),
            other => panic!("expected Unexpected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_normalizes_trailing_v1_in_base_url() {
        // A base URL that already ends in /v1 (common when users paste docs examples)
        // must not produce /v1/v1/models — the probe should strip and re-add /v1.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"data\":[]}"))
            .mount(&server)
            .await;

        let base_with_v1 = format!("{}/v1", server.uri());
        let outcome = validate(
            &Provider::from_id("openai").expect("openai provider mod must be loaded"),
            &base_with_v1,
            Some("sk-test"),
        )
        .await;
        assert_eq!(outcome, ValidationOutcome::Ok);
    }
}
