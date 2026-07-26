//! Wiremock-based integration tests for `run_tier2_for_group`.
//!
//! Closes the "async LLM path for Tier 2 inference is unexercised" gap
//! identified in the engine audit (Tier A.2). The inline unit tests only
//! cover the solo-NPC template path (no HTTP) and the empty-group path.
//! These tests spin up a wiremock server, back an InferenceQueue worker
//! with it, and drive the multi-NPC path through success, HTTP error,
//! and malformed-JSON branches.

use parish_inference::AnyClient;
use parish_inference::openai_client::OpenAiClient;
use parish_npc::LanguageSettings;
use parish_npc::ticks::{NpcSnapshot, Tier2Group, run_tier2_for_group};
use parish_types::{LocationId, NpcId};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn two_npc_group() -> Tier2Group {
    Tier2Group {
        location: LocationId(2),
        location_name: "Darcy's Pub".to_string(),
        other_location_names: vec!["The Mill".to_string(), "The Forge".to_string()],
        npcs: vec![
            NpcSnapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                personality: "Warm and welcoming".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: "Perceptive, wise, quick-witted.".to_string(),
                mood: "content".to_string(),
                relationship_summary: String::new(),
                current_activity: Some("tending bar".to_string()),
                grounding_revision: 1,
                activity_fingerprint: "interval-padraig".to_string(),
            },
            NpcSnapshot {
                id: NpcId(2),
                name: "Tommy".to_string(),
                occupation: "Farmer".to_string(),
                personality: "Gruff but kind".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: "Plain-spoken and sharp-minded.".to_string(),
                mood: "tired".to_string(),
                relationship_summary: String::new(),
                current_activity: Some("having a quiet drink".to_string()),
                grounding_revision: 2,
                activity_fingerprint: "interval-tommy".to_string(),
            },
        ],
    }
}

/// Build an `AnyClient` pointing at the given wiremock URI.
///
/// `run_tier2_for_group` now dispatches directly against an `AnyClient`
/// (the per-category Simulation client resolved by the caller) rather
/// than the shared `InferenceQueue`, so the test fixture mirrors that.
fn mock_client(server_uri: &str) -> AnyClient {
    AnyClient::open_ai(OpenAiClient::new(server_uri, None))
}

/// Mount a `/v1/chat/completions` response carrying `content` as the
/// assistant message text.
///
/// Emits an SSE body so the streaming code path in the worker can parse
/// it — `submit_json_streaming` (used by Tier 2 / Tier 3) routes through
/// `generate_stream_with_format`, which expects
/// `data: {...}\n\ndata: [DONE]\n\n`.
async fn mount_tier2_response(server: &MockServer, content: &str) {
    let chunk = serde_json::json!({
        "choices": [{"delta": {"content": content}}],
    });
    let stop = serde_json::json!({
        "choices": [{"delta": {}, "finish_reason": "stop"}],
    });
    let body = format!("data: {chunk}\n\ndata: {stop}\n\ndata: [DONE]\n\n");
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/event-stream")
                .set_body_string(body),
        )
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

    let client = mock_client(&server.uri());
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Afternoon",
        "Clear",
        &lang,
        None,
    )
    .await;

    let event = event.expect("multi-NPC group should return Some on successful LLM response");
    assert_eq!(event.location, LocationId(2));
    assert_eq!(event.participants, vec![NpcId(1), NpcId(2)]);
    assert!(event.summary.contains("Padraig"));
    assert_eq!(event.grounding.len(), 2);
    assert!(event.mood_changes.is_empty());
    assert!(event.relationship_changes.is_empty());
}

/// #1785: occupation-flavoured free prose is not authoritative for physical
/// action. Colm's authored Crossroads activity wins even when the model tries
/// to put the blacksmith's apprentice back at an anvil.
#[tokio::test]
async fn tier2_canonicalizes_adversarial_activity_conflict() {
    let server = MockServer::start().await;
    mount_tier2_response(
        &server,
        r#"{"summary":"Colm Gallagher hammers away at a horseshoe while Tommy watches.","mood_changes":[],"relationship_changes":[]}"#,
    )
    .await;

    let client = mock_client(&server.uri());
    let group = Tier2Group {
        location: LocationId(1),
        location_name: "The Crossroads".to_string(),
        other_location_names: vec!["The Forge".to_string()],
        npcs: vec![
            NpcSnapshot {
                id: NpcId(11),
                name: "Colm Gallagher".to_string(),
                occupation: "Blacksmith's Apprentice".to_string(),
                personality: "Eager".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: String::new(),
                mood: "restless".to_string(),
                relationship_summary: String::new(),
                current_activity: Some("running errands and delivering repaired tools".to_string()),
                grounding_revision: 11,
                activity_fingerprint: "interval-colm".to_string(),
            },
            NpcSnapshot {
                id: NpcId(5),
                name: "Tommy O'Brien".to_string(),
                occupation: "Retired Farmer".to_string(),
                personality: "Storyteller".to_string(),
                pronouns: "he/him".to_string(),
                intelligence_prose: String::new(),
                mood: "reflective".to_string(),
                relationship_summary: String::new(),
                current_activity: Some("sitting on the wall, telling stories".to_string()),
                grounding_revision: 5,
                activity_fingerprint: "interval-tommy".to_string(),
            },
        ],
    };
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Morning",
        "Clear",
        &LanguageSettings::english_only(),
        None,
    )
    .await
    .expect("schema-valid response should produce a canonically narrated event");

    assert!(
        event
            .summary
            .contains("running errands and delivering repaired tools"),
        "canonical authored activity must reach the event: {}",
        event.summary
    );
    assert!(
        !event.summary.contains("hammers") && !event.summary.contains("horseshoe"),
        "contradictory model action must not reach memory, gossip, or UI: {}",
        event.summary
    );
    assert_eq!(event.grounding.len(), 2);
    assert!(
        event
            .grounding
            .iter()
            .all(|anchor| anchor.location == LocationId(1)
                && matches!(anchor.grounding_revision, 11 | 5)
                && !anchor.activity_fingerprint.is_empty()),
        "every participant must carry stable location/activity/revision anchors"
    );
}

#[tokio::test]
async fn tier2_location_conflict_retries_once_then_drops_event() {
    let server = MockServer::start().await;
    mount_tier2_response(
        &server,
        r#"{"summary":"Padraig and Tommy wait by The Mill.","mood_changes":[],"relationship_changes":[]}"#,
    )
    .await;

    let client = mock_client(&server.uri());
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Afternoon",
        "Clear",
        &lang,
        None,
    )
    .await;

    assert!(
        event.is_none(),
        "an event that still names another canonical location after retry must be dropped"
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        2,
        "location conflict should trigger exactly one corrective retry"
    );
}

#[tokio::test]
async fn tier2_multi_npc_with_mood_and_relationship_changes() {
    let server = MockServer::start().await;
    mount_tier2_response(
        &server,
        r#"{"summary":"Tommy complains about the rent. Padraig sympathises.","mood_changes":[{"npc_id":2,"new_mood":"frustrated"}],"relationship_changes":[{"from":1,"to":2,"delta":0.1}]}"#,
    )
    .await;

    let client = mock_client(&server.uri());
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Morning",
        "Clear",
        &lang,
        None,
    )
    .await;

    let event = event.unwrap();
    assert_eq!(event.mood_changes.len(), 1);
    assert_eq!(event.relationship_changes.len(), 1);
    assert!((event.relationship_changes[0].delta - 0.1).abs() < f64::EPSILON);
}

/// Regression — when the wizard's `small-only` variant routes Tier 2 to
/// the in-process simulator, the simulator must produce a string the
/// `Tier2Response` parser accepts. Without the
/// `AnyClient::Simulator::generate_stream_with_format` JSON-detection
/// shim landed alongside the `small-only` routing change, the simulator
/// streamed plain Markov text into a JSON parser and Tier 2 ticks
/// flooded the log with a parse failure every 5 game-seconds (one per
/// nearby location). This test pins the contract: feed the simulator
/// the actual prompt `build_tier2_prompt` produces and verify the
/// returned event has the right shape rather than an Inference error.
#[tokio::test]
async fn tier2_through_simulator_parses_as_empty_event() {
    let client = AnyClient::simulator();
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event =
        run_tier2_for_group(&client, "sim", &group, "Afternoon", "Clear", &lang, None).await;

    let event = event.expect(
        "simulator-routed Tier 2 must produce a parseable event (not a JSON parse failure)",
    );
    assert_eq!(event.location, LocationId(2));
    assert_eq!(event.participants, vec![NpcId(1), NpcId(2)]);
    // The simulator can't actually reason about NPC interactions, so the
    // parse fills `Tier2Response` fields from `#[serde(default)]`. The
    // contract we care about is that the parser DOESN'T fail: tier2_event
    // exists, fields are well-formed (any string / any empty vec is OK).
    let _ = event.summary;
    let _ = event.mood_changes;
    let _ = event.relationship_changes;
}

#[tokio::test]
async fn tier2_http_error_returns_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Morning",
        "Clear",
        &lang,
        None,
    )
    .await;

    assert!(event.is_none(), "HTTP error must return None, not panic");
}

#[tokio::test]
async fn tier2_malformed_json_returns_none() {
    let server = MockServer::start().await;
    mount_tier2_response(&server, "this is not json at all").await;

    let client = mock_client(&server.uri());
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Morning",
        "Clear",
        &lang,
        None,
    )
    .await;

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

    let client = mock_client(&server.uri());
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Morning",
        "Clear",
        &lang,
        None,
    )
    .await;

    assert!(
        event.is_none(),
        "empty choices array must return None, not panic"
    );
}

#[tokio::test]
async fn tier2_missing_optional_fields_defaults_to_empty() {
    let server = MockServer::start().await;
    mount_tier2_response(&server, r#"{"summary":"They nod at each other."}"#).await;

    let client = mock_client(&server.uri());
    let group = two_npc_group();
    let lang = LanguageSettings::english_only();
    let event = run_tier2_for_group(
        &client,
        "test-model",
        &group,
        "Morning",
        "Clear",
        &lang,
        None,
    )
    .await;

    let event = event.expect("missing optional fields should still parse via serde defaults");
    assert_eq!(
        event.summary,
        "At Darcy's Pub — Padraig: tending bar; Tommy: having a quiet drink."
    );
    assert!(event.mood_changes.is_empty());
    assert!(event.relationship_changes.is_empty());
}
