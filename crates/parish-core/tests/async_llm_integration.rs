//! Async LLM path integration tests for `game_session::resolve_reaction_texts`.
//!
//! Closes the "async LLM path is unexercised" regression gap identified
//! in the engine audit (Tier A). These tests spin up a `wiremock` server
//! standing in for an OpenAI-compatible endpoint, build an `OpenAiClient`
//! pointed at it, and drive `resolve_reaction_texts` through its key
//! branches:
//!
//!   1. Successful LLM response → cleaned text is returned.
//!   2. Response containing `---` separator → only the prefix is returned.
//!   3. Empty LLM response → canned text fallback.
//!   4. HTTP error from upstream → canned text fallback (no panic).
//!   5. Timeout elapses → canned text fallback (covers the `tokio::time::timeout` arm).
//!   6. `use_llm = false` → canned text is returned without contacting the server.
//!   7. `client: None` → canned text is returned without contacting the server.
//!
//! Bypassing these via the live network would leave the entire async
//! fallback ladder untested — that's the exact gap the audit flagged.

use std::collections::HashSet;

use parish_core::game_session::resolve_reaction_texts;
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::npc::Npc;
use parish_core::npc::reactions::{NpcReaction, ReactionKind};
use parish_types::{LocationId, NpcId, TimeOfDay};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build an NPC at a known location with a canned personality so the
/// reaction prompt renders cleanly.
fn test_npc() -> Npc {
    let mut npc = Npc::new_test_npc();
    npc.id = NpcId(42);
    npc.name = "Padraig Darcy".to_string();
    npc.location = LocationId(2);
    npc.workplace = Some(LocationId(2));
    npc
}

/// Build a reaction that asks the resolver to call the LLM.
fn llm_reaction(canned: &str) -> NpcReaction {
    NpcReaction {
        npc_id: NpcId(42),
        npc_display_name: "Padraig Darcy".to_string(),
        kind: ReactionKind::Welcome,
        canned_text: canned.to_string(),
        introduces: false,
        use_llm: true,
    }
}

/// Mount a single POST `/v1/chat/completions` response on the given server.
async fn mount_openai_response(server: &MockServer, content: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": content}}]
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn resolve_reaction_texts_returns_llm_response_on_success() {
    let server = MockServer::start().await;
    mount_openai_response(&server, "Well, good day to ye").await;

    let client = OpenAiClient::new(&server.uri(), None);
    let npc = test_npc();
    let reactions = [llm_reaction("(canned greeting)")];

    let texts = resolve_reaction_texts(
        &reactions,
        std::slice::from_ref(&npc),
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        Some(&client),
        "gpt-test",
        None,
    )
    .await;

    assert_eq!(texts.len(), 1);
    assert_eq!(
        texts[0], "Well, good day to ye",
        "LLM success path must surface the LLM content verbatim (after trim)"
    );
}

#[tokio::test]
async fn resolve_reaction_texts_strips_separator_from_llm_response() {
    // The resolver is documented to split on "---" and keep only the prefix,
    // which is how Tier 1 reactions carry optional structured metadata after
    // the spoken line.
    let server = MockServer::start().await;
    mount_openai_response(&server, "Aye, welcome in.\n---\nmood: content").await;

    let client = OpenAiClient::new(&server.uri(), None);
    let npc = test_npc();
    let reactions = [llm_reaction("(canned)")];

    let texts = resolve_reaction_texts(
        &reactions,
        std::slice::from_ref(&npc),
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        Some(&client),
        "gpt-test",
        None,
    )
    .await;

    assert_eq!(texts[0], "Aye, welcome in.", "suffix must be stripped");
}

#[tokio::test]
async fn resolve_reaction_texts_falls_back_to_canned_on_empty_response() {
    let server = MockServer::start().await;
    mount_openai_response(&server, "   ").await; // whitespace only → empty after trim

    let client = OpenAiClient::new(&server.uri(), None);
    let npc = test_npc();
    let reactions = [llm_reaction("canned welcome line")];

    let texts = resolve_reaction_texts(
        &reactions,
        std::slice::from_ref(&npc),
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        Some(&client),
        "gpt-test",
        None,
    )
    .await;

    assert_eq!(
        texts[0], "canned welcome line",
        "empty LLM response must fall back to canned_text"
    );
}

#[tokio::test]
async fn resolve_reaction_texts_falls_back_to_canned_on_http_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let npc = test_npc();
    let reactions = [llm_reaction("canned fallback")];

    let texts = resolve_reaction_texts(
        &reactions,
        std::slice::from_ref(&npc),
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        Some(&client),
        "gpt-test",
        None,
    )
    .await;

    assert_eq!(
        texts[0], "canned fallback",
        "HTTP errors must surface as the canned fallback, never panic"
    );
}

#[tokio::test]
async fn resolve_reaction_texts_falls_back_to_canned_on_timeout() {
    // Mount a response that takes longer than the ReactionConfig default
    // timeout of 5s. We set the delay to 6s so the `tokio::time::timeout`
    // wrapper in the resolver fires first.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(6))
                .set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "too late"}}]
                })),
        )
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let npc = test_npc();
    let reactions = [llm_reaction("timed-out canned")];

    let texts = resolve_reaction_texts(
        &reactions,
        std::slice::from_ref(&npc),
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        Some(&client),
        "gpt-test",
        None,
    )
    .await;

    assert_eq!(
        texts[0], "timed-out canned",
        "hitting the 5s reaction timeout must surface canned_text, not the delayed response"
    );
}

#[tokio::test]
async fn resolve_reaction_texts_honors_use_llm_false() {
    // No server at all — the resolver must not attempt a network call
    // when `use_llm = false`, so the test would hang if it did.
    let npc = test_npc();
    let reactions = [NpcReaction {
        npc_id: NpcId(42),
        npc_display_name: "Padraig".to_string(),
        kind: ReactionKind::Gesture,
        canned_text: "nods silently".to_string(),
        introduces: false,
        use_llm: false,
    }];

    let bogus_client = OpenAiClient::new("http://127.0.0.1:1", None);

    let texts = resolve_reaction_texts(
        &reactions,
        std::slice::from_ref(&npc),
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        Some(&bogus_client),
        "gpt-test",
        None,
    )
    .await;

    assert_eq!(texts[0], "nods silently");
}

#[tokio::test]
async fn resolve_reaction_texts_handles_none_client() {
    // Same guarantee: if no client is passed, the LLM path is skipped.
    let npc = test_npc();
    let reactions = [llm_reaction("canned when no client")];

    let texts = resolve_reaction_texts(
        &reactions,
        std::slice::from_ref(&npc),
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        None,
        "gpt-test",
        None,
    )
    .await;

    assert_eq!(texts[0], "canned when no client");
}

#[tokio::test]
async fn resolve_reaction_texts_handles_empty_reaction_list() {
    let texts = resolve_reaction_texts(
        &[],
        &[],
        LocationId(2),
        "Darcy's Pub",
        TimeOfDay::Morning,
        "Clear",
        &HashSet::new(),
        None,
        "gpt-test",
        None,
    )
    .await;

    assert!(texts.is_empty());
}
