//! Wiremock-based integration tests for `run_tier2_for_group`.
//!
//! Closes the "async LLM path for Tier 2 inference is unexercised" gap
//! identified in the engine audit (Tier A.2). The inline unit tests only
//! cover the solo-NPC template path (no HTTP) and the empty-group path.
//! These tests spin up a wiremock server, back an InferenceQueue worker
//! with it, and drive the multi-NPC path through success, HTTP error,
//! and malformed-JSON branches.

use parish_inference::openai_client::OpenAiClient;
use parish_inference::{
    AnyClient, InferenceQueue, InferenceRequest, new_inference_log, spawn_inference_worker,
};
use parish_npc::ticks::{NpcSnapshot, Tier2Group, run_tier2_for_group};
use parish_types::{LocationId, NpcId};
use tokio::sync::mpsc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn two_npc_group() -> Tier2Group {
    Tier2Group {
        location: LocationId(2),
        location_name: "Darcy's Pub".to_string(),
        npcs: vec![
            NpcSnapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                personality: "Warm and welcoming".to_string(),
                intelligence_tag: "INT[V3 A3 E4 P4 W5 C4]".to_string(),
                mood: "content".to_string(),
                relationship_context: String::new(),
            },
            NpcSnapshot {
                id: NpcId(2),
                name: "Tommy".to_string(),
                occupation: "Farmer".to_string(),
                personality: "Gruff but kind".to_string(),
                intelligence_tag: "INT[V2 A4 E3 P3 W4 C2]".to_string(),
                mood: "tired".to_string(),
                relationship_context: String::new(),
            },
        ],
    }
}

/// Build an InferenceQueue backed by a wiremock server and spawn a worker.
fn spawn_mock_worker(server_uri: &str) -> InferenceQueue {
    let client = OpenAiClient::new(server_uri, None);
    let any_client = AnyClient::open_ai(client);
    let log = new_inference_log();

    let (itx, irx) = mpsc::channel::<InferenceRequest>(16);
    let (btx, brx) = mpsc::channel::<InferenceRequest>(32);
    let (batx, batrx) = mpsc::channel::<InferenceRequest>(64);
    let queue = InferenceQueue::new(itx, btx, batx);

    spawn_inference_worker(
        any_client,
        irx,
        brx,
        batrx,
        log,
        parish_inference::InferenceConfig::default(),
    );

    queue
}

/// Mount a `/v1/chat/completions` response whose `content` field is a JSON
/// string that will be deserialized into `Tier2Response`.
async fn mount_tier2_response(server: &MockServer, content: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": content}}]
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn tier2_multi_npc_success_returns_event() {
    let server = MockServer::start().await;
    mount_tier2_response(
        &server,
        r#"{"summary":"Padraig pours Tommy a pint and they chat about the harvest.","mood_changes":[],"relationship_changes":[]}"#,
    )
    .await;

    let queue = spawn_mock_worker(&server.uri());
    let group = two_npc_group();
    let event = run_tier2_for_group(&queue, "test-model", &group, "Afternoon", "Clear").await;

    let event = event.expect("multi-NPC group should return Some on successful LLM response");
    assert_eq!(event.location, LocationId(2));
    assert_eq!(event.participants, vec![NpcId(1), NpcId(2)]);
    assert!(event.summary.contains("Padraig"));
    assert!(event.mood_changes.is_empty());
    assert!(event.relationship_changes.is_empty());
}

#[tokio::test]
async fn tier2_multi_npc_with_mood_and_relationship_changes() {
    let server = MockServer::start().await;
    mount_tier2_response(
        &server,
        r#"{"summary":"Tommy complains about the rent. Padraig sympathises.","mood_changes":[{"npc_id":2,"new_mood":"frustrated"}],"relationship_changes":[{"from":1,"to":2,"delta":0.1}]}"#,
    )
    .await;

    let queue = spawn_mock_worker(&server.uri());
    let group = two_npc_group();
    let event = run_tier2_for_group(&queue, "test-model", &group, "Morning", "Clear").await;

    let event = event.unwrap();
    assert_eq!(event.mood_changes.len(), 1);
    assert_eq!(event.relationship_changes.len(), 1);
    assert!((event.relationship_changes[0].delta - 0.1).abs() < f64::EPSILON);
}

#[tokio::test]
async fn tier2_http_error_returns_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;

    let queue = spawn_mock_worker(&server.uri());
    let group = two_npc_group();
    let event = run_tier2_for_group(&queue, "test-model", &group, "Morning", "Clear").await;

    assert!(event.is_none(), "HTTP error must return None, not panic");
}

#[tokio::test]
async fn tier2_malformed_json_returns_none() {
    let server = MockServer::start().await;
    mount_tier2_response(&server, "this is not json at all").await;

    let queue = spawn_mock_worker(&server.uri());
    let group = two_npc_group();
    let event = run_tier2_for_group(&queue, "test-model", &group, "Morning", "Clear").await;

    assert!(
        event.is_none(),
        "malformed JSON content must return None, not panic"
    );
}

#[tokio::test]
async fn tier2_empty_choices_returns_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"choices": []})))
        .mount(&server)
        .await;

    let queue = spawn_mock_worker(&server.uri());
    let group = two_npc_group();
    let event = run_tier2_for_group(&queue, "test-model", &group, "Morning", "Clear").await;

    assert!(
        event.is_none(),
        "empty choices array must return None, not panic"
    );
}

#[tokio::test]
async fn tier2_missing_optional_fields_defaults_to_empty() {
    let server = MockServer::start().await;
    mount_tier2_response(&server, r#"{"summary":"They nod at each other."}"#).await;

    let queue = spawn_mock_worker(&server.uri());
    let group = two_npc_group();
    let event = run_tier2_for_group(&queue, "test-model", &group, "Morning", "Clear").await;

    let event = event.expect("missing optional fields should still parse via serde defaults");
    assert_eq!(event.summary, "They nod at each other.");
    assert!(event.mood_changes.is_empty());
    assert!(event.relationship_changes.is_empty());
}
