#![cfg(feature = "mobile")]
//! A readable walkthrough of the interpreted-action path (#1993).
//!
//! It drives the production `MobileSession` submission path and prints what a
//! player would see alongside the Endpoint role each turn used, so the proof
//! bundle's transcript can be regenerated verbatim:
//!
//! ```sh
//! cargo test -p limerick-core --no-default-features --features mobile \
//!     --test mobile_intent_transcript -- --nocapture
//! ```
//!
//! The assertions are deliberately about authoritative state and the selected
//! role; `mobile_intent_contract.rs` holds the exhaustive lifecycle coverage.

use limerick_core::mobile::{
    EndpointCandidate, EndpointIntentCandidate, EndpointRole, MobileSession, SemanticEventKind,
};
use std::collections::BTreeMap;

fn render(label: &str, result: &limerick_core::mobile::MobileOperationResult) {
    for event in &result.events {
        let kind = format!("{:?}", event.kind);
        let body = event.content.clone().unwrap_or_default();
        let speaker = event.speaker.clone().unwrap_or_default();
        if event.kind == SemanticEventKind::ResponseCompleted {
            println!("      [{kind}] outcome={:?}", event.terminal_outcome);
        } else if speaker.is_empty() {
            println!("      [{kind}] {body}");
        } else {
            println!("      [{kind}] {speaker}: {body}");
        }
    }
    let _ = label;
}

#[test]
fn interpreted_action_walkthrough() {
    let mut session = MobileSession::open_new().unwrap();
    let scene = session.snapshot().read_model;
    println!("=== Rundale mobile session (production MobileSession path) ===");
    println!("location  : {} ({})", scene.scene.name, scene.time_of_day);
    println!(
        "nearby    : {:?}",
        scene
            .nearby_people
            .iter()
            .map(|p| p.display_name.clone())
            .collect::<Vec<_>>()
    );
    println!();

    // 1. Deterministic command — fully offline.
    println!("> /people");
    let r = session.submit(None, "/people", None).unwrap();
    println!(
        "  endpoint  : {:?}",
        r.endpoint_invocation.as_ref().map(|i| i.role)
    );
    render("people", &r);
    println!();

    // 2. Inference-requiring conversation, while Peig is still in the village.
    println!("> Any word from beyond the parish?");
    let accepted = session
        .submit(None, "Any word from beyond the parish?", None)
        .unwrap();
    let inv = accepted.endpoint_invocation.clone().unwrap();
    println!("  endpoint  : role={}", inv.role.wire_value());
    println!("  <- Intent Endpoint returns {{\"intent\":\"talk\",\"target\":\"Peig\"}}");
    let dispatched = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: inv.attempt_id.clone(),
            base_revision: inv.base_revision,
            payload: serde_json::json!({"intent":"talk","target":"Peig"}),
            metadata: BTreeMap::new(),
        })
        .unwrap();
    let dialogue = dispatched.endpoint_invocation.clone().unwrap();
    println!(
        "  endpoint  : role={} speaker={}",
        dialogue.role.wire_value(),
        dialogue.speaker.as_ref().unwrap().display_name
    );
    render("dispatch", &dispatched);
    println!("  <- Dialogue Endpoint returns one grounded utterance");
    let committed = session
        .receive_candidate(EndpointCandidate {
            attempt_id: dialogue.attempt_id.clone(),
            base_revision: dialogue.base_revision,
            dialogue: "Little enough came up the road this morning, and what did is still in the pigeonholes.".to_string(),
            metadata: BTreeMap::new(),
            structured: true,
        })
        .unwrap();
    render("commit", &committed);
    assert_eq!(
        session
            .world()
            .conversation_log
            .recent_at(limerick_types::LocationId(1), 5)
            .len(),
        1,
        "dialogue commits exactly one exchange"
    );
    println!();

    // 3. Inference-requiring movement.
    println!("> Off to the letter office");
    println!(
        "  local parse: {:?}",
        limerick_input::parse_intent_local("Off to the letter office").map(|i| i.intent)
    );
    let accepted = session
        .submit(None, "Off to the letter office", None)
        .unwrap();
    let inv = accepted.endpoint_invocation.clone().unwrap();
    println!(
        "  endpoint  : role={} key={}",
        inv.role.wire_value(),
        inv.idempotency_key
    );
    println!(
        "  speaker   : {:?}",
        inv.speaker.as_ref().map(|s| s.display_name.clone())
    );
    assert_eq!(inv.role, EndpointRole::Intent);
    render("submit", &accepted);
    println!("  <- Intent Endpoint returns {{\"intent\":\"move\",\"target\":\"Letter Office\"}}");
    let dispatched = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: inv.attempt_id.clone(),
            base_revision: inv.base_revision,
            payload: serde_json::json!({"intent":"move","target":"Letter Office"}),
            metadata: BTreeMap::new(),
        })
        .unwrap();
    println!(
        "  endpoint  : {:?}",
        dispatched.endpoint_invocation.as_ref().map(|i| i.role)
    );
    render("dispatch", &dispatched);
    println!(
        "  location  : {} (revision {})",
        session.snapshot().read_model.scene.name,
        session.state_revision().raw_value
    );
    println!();

    // 3. Back to the village, then inferred conversation.
    println!("> Back to Kilteevan Village");
    let accepted = session
        .submit(None, "Back to Kilteevan Village", None)
        .unwrap();
    let inv = accepted.endpoint_invocation.clone().unwrap();
    let dispatched = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: inv.attempt_id.clone(),
            base_revision: inv.base_revision,
            payload: serde_json::json!({"intent":"move","target":"Kilteevan Village"}),
            metadata: BTreeMap::new(),
        })
        .unwrap();
    render("dispatch", &dispatched);
    println!("  location  : {}", session.snapshot().read_model.scene.name);
    println!();

    // 4. An interpretation this client cannot execute.
    println!("> Take up the spade by the wall");
    let accepted = session
        .submit(None, "Take up the spade by the wall", None)
        .unwrap();
    let inv = accepted.endpoint_invocation.clone().unwrap();
    println!("  endpoint  : role={}", inv.role.wire_value());
    println!("  <- Intent Endpoint returns {{\"intent\":\"interact\",\"target\":\"the spade\"}}");
    let dispatched = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: inv.attempt_id.clone(),
            base_revision: inv.base_revision,
            payload: serde_json::json!({"intent":"interact","target":"the spade"}),
            metadata: BTreeMap::new(),
        })
        .unwrap();
    render("reject", &dispatched);
    assert_eq!(
        dispatched.terminal_outcome,
        Some(limerick_core::mobile::ResponseTerminalOutcome::Failed),
        "an action this client cannot execute is rejected, not answered"
    );
    println!();

    // 5. Stop during interpretation.
    println!("> Away to the connolly cottage   (then Stop before the result arrives)");
    let accepted = session
        .submit(None, "Away to the connolly cottage", None)
        .unwrap();
    render("submit", &accepted);
    let inv = accepted.endpoint_invocation.clone().unwrap();
    println!("  endpoint  : role={}", inv.role.wire_value());
    let stopped = session.stop(&inv.attempt_id).unwrap();
    render("stop", &stopped);
    let late = session
        .receive_intent_candidate(EndpointIntentCandidate {
            attempt_id: inv.attempt_id.clone(),
            base_revision: inv.base_revision,
            payload: serde_json::json!({"intent":"move","target":"Connolly Cottage"}),
            metadata: BTreeMap::new(),
        })
        .unwrap();
    assert!(
        late.ignored,
        "a result that arrives after Stop must not act"
    );
    println!("  late result ignored: {}", late.ignored);
    println!(
        "  location  : {} (revision {})",
        session.snapshot().read_model.scene.name,
        session.state_revision().raw_value
    );
    println!();
    println!("=== end of session ===");
}
