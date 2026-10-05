//! Tests through the C boundary against the real engine and the canonical
//! world (`mods/rundale`). The Endpoint host is scripted: each test answers
//! the pending invocations the way the Swift host does.
//!
//! Oracle: `ios-port` request-lifecycle tests and the RundaleKit
//! `SessionReducer` lifecycle fixtures (portable-turn-api.md §2.4).

use super::*;
use limerick_core::turn::INTERRUPTED_MESSAGE;
use std::path::Path;

const CATTLE_LINE: &str = "The wet ground has made moving cattle hard this week.";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn borrowed(value: &str) -> limerick_mobile_bytes_t {
    limerick_mobile_bytes_t {
        ptr: value.as_ptr(),
        len: value.len(),
    }
}

/// Copies an owned response into a JSON value and frees it.
fn take(response: limerick_mobile_owned_bytes_t) -> Value {
    assert!(!response.ptr.is_null(), "response must carry an envelope");
    // SAFETY: the pointer and length came from `owned_bytes`.
    let bytes = unsafe { slice::from_raw_parts(response.ptr, response.len) }.to_vec();
    assert_eq!(
        limerick_mobile_owned_bytes_free(response),
        limerick_mobile_status_t::LIMERICK_MOBILE_OK
    );
    serde_json::from_slice(&bytes).expect("response is JSON")
}

fn open_raw(
    kind: limerick_mobile_open_kind_t,
    request: &str,
) -> (limerick_mobile_status_t, u64, Value) {
    let mut handle = 99;
    let mut response = empty_owned();
    let status = limerick_mobile_open(kind, borrowed(request), &mut handle, &mut response);
    (status, handle, take(response))
}

/// A session on a save in a temporary directory.
struct Game {
    dir: tempfile::TempDir,
    handle: u64,
}

/// Copies the canonical world into `dir`.
fn copy_world(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_world(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

impl Game {
    fn request(dir: &tempfile::TempDir) -> String {
        let local = dir.path().join("mod");
        let mod_dir = if local.exists() {
            local
        } else {
            repo_root().join("mods/rundale")
        };
        json!({
            "save_path": dir.path().join("game.sqlite"),
            "mod_dir": mod_dir,
        })
        .to_string()
    }

    fn new() -> (Self, Value) {
        Self::open_in(tempfile::tempdir().unwrap())
    }

    /// A game on a copy of the canonical world where Róisín is a cattle
    /// drover like Mícheál, so "Drover" names two people in the cottage.
    fn with_two_drovers() -> (Self, Value) {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path().join("mod");
        copy_world(&repo_root().join("mods/rundale"), &world);
        let npcs = std::fs::read_to_string(world.join("npcs.json")).unwrap();
        std::fs::write(
            world.join("npcs.json"),
            npcs.replace(
                "Spinner and household bookkeeper",
                "Smallholder and cattle drover",
            ),
        )
        .unwrap();
        Self::open_in(dir)
    }

    fn open_in(dir: tempfile::TempDir) -> (Self, Value) {
        let (status, handle, envelope) = open_raw(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
            &Self::request(&dir),
        );
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_OK,
            "{envelope}"
        );
        assert_ne!(handle, 0);
        (Self { dir, handle }, envelope["value"].clone())
    }

    /// Closes the session and opens the same save again, as a relaunch.
    fn relaunch(&mut self) -> Value {
        assert_eq!(
            limerick_mobile_close(self.handle),
            limerick_mobile_status_t::LIMERICK_MOBILE_OK
        );
        let (status, handle, envelope) = open_raw(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME,
            &Self::request(&self.dir),
        );
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_OK,
            "{envelope}"
        );
        self.handle = handle;
        envelope["value"].clone()
    }

    fn dispatch(&self, operation: Value) -> (limerick_mobile_status_t, Value) {
        let text = operation.to_string();
        let mut response = empty_owned();
        let status = limerick_mobile_dispatch(self.handle, borrowed(&text), &mut response);
        (status, take(response))
    }

    fn op(&self, operation: Value) -> Value {
        let (status, envelope) = self.dispatch(operation.clone());
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_OK,
            "{operation} -> {envelope}"
        );
        envelope["value"].clone()
    }

    fn submit(&self, text: &str) -> Value {
        self.op(json!({"op": "submit", "text": text, "draft_id": "draft-1"}))
    }

    fn pending(&self) -> Value {
        self.op(json!({"op": "pending_endpoint"}))
    }

    fn snapshot(&self) -> Value {
        self.op(json!({"op": "snapshot"}))
    }

    fn resolve(&self, pending: &Value, output: Value) -> Value {
        self.op(json!({
            "op": "resolve",
            "call_id": pending["callID"],
            "attempt_id": pending["attemptID"],
            "base_revision": pending["baseRevision"],
            "output": output,
        }))
    }

    fn frame(&self, pending: &Value, sequence: u64, text: &str) -> Value {
        self.op(json!({
            "op": "frame",
            "call_id": pending["callID"],
            "attempt_id": pending["attemptID"],
            "sequence": sequence,
            "text": text,
        }))
    }

    /// Walks to Connolly Cottage, where Mícheál and Róisín are all morning.
    fn go_to_the_cottage(&self) -> Value {
        let moved = self.submit("go to Connolly Cottage");
        assert_eq!(moved["terminalOutcome"], "succeeded", "{moved}");
        let names = people(&self.snapshot());
        assert!(names.len() >= 2, "both Connollys are home: {names:?}");
        moved
    }

    /// Answers intent calls until the attempt waits on a dialogue call, and
    /// returns that dialogue invocation.
    fn until_dialogue(&self, mut last: Value) -> Value {
        for _ in 0..4 {
            let pending = self.pending();
            assert!(!pending.is_null(), "expected a pending call after {last}");
            match pending["endpoint"]["role"].as_str() {
                Some("dialogue") => return pending,
                Some("intent") => {
                    assert_eq!(pending["input"]["role"], "player_intent");
                    assert_eq!(
                        pending["stream"], false,
                        "the intent definition does not stream"
                    );
                    last = self.resolve(
                        &pending,
                        json!({"intent": "talk", "target": null, "dialogue": null, "atmosphere": null}),
                    );
                }
                other => panic!("unexpected Endpoint role {other:?}"),
            }
        }
        panic!("no dialogue call");
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = limerick_mobile_close(self.handle);
    }
}

fn people(snapshot: &Value) -> Vec<String> {
    snapshot["readModel"]["nearbyPeople"]
        .as_array()
        .unwrap()
        .iter()
        .map(|person| person["displayName"].as_str().unwrap().to_string())
        .collect()
}

fn kinds(result: &Value) -> Vec<String> {
    result["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["kind"].as_str().unwrap().to_string())
        .collect()
}

fn events_of_kind<'a>(result: &'a Value, kind: &str) -> Vec<&'a Value> {
    result["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == kind)
        .collect()
}

fn sequences(result: &Value) -> Vec<u64> {
    result["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["sequence"]["rawValue"].as_u64().unwrap())
        .collect()
}

fn revision(snapshot: &Value) -> u64 {
    snapshot["stateRevision"]["rawValue"].as_u64().unwrap()
}

fn request_record<'a>(snapshot: &'a Value, id: &Value) -> &'a Value {
    snapshot["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["id"] == *id)
        .expect("the request is in the snapshot")
}

#[test]
fn a_new_game_opens_on_the_journaled_opening_scene_and_resumes_without_repeating_it() {
    let (mut game, opening) = Game::new();
    assert_eq!(opening["contractVersion"], json!({"major": 1, "minor": 0}));
    assert_eq!(revision(&opening), 0);
    assert_eq!(opening["readModel"]["scene"]["name"], "Kilteevan Village");
    assert_eq!(opening["readModel"]["timeOfDay"], "Morning");
    let events = opening["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "scene_changed");
    assert_eq!(events[0]["metadata"]["sceneName"], "Kilteevan Village");
    assert_eq!(events[0]["metadata"]["sceneID"], "1");
    let text = events[0]["content"].as_str().unwrap();
    assert!(
        text.starts_with("A muddy road runs between low stone walls"),
        "{text}"
    );
    assert_scene_body(text, &people(&opening));

    let (status, _, refused) = open_raw(
        limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
        &Game::request(&game.dir),
    );
    assert_ne!(status, limerick_mobile_status_t::LIMERICK_MOBILE_OK);
    assert_eq!(refused["error"]["code"], "save_exists");

    let resumed = game.relaunch();
    assert_eq!(resumed["sessionID"], opening["sessionID"]);
    assert_eq!(
        resumed["events"], opening["events"],
        "the opening is not journaled twice"
    );
}

/// A scene body is the standing description and who is present: the time of
/// day, weather, and exits are the header's and `/exits`'s.
fn assert_scene_body(text: &str, present: &[String]) {
    for restated in ["It is morning", "weather is", "sky hangs", "You can go to"] {
        assert!(!text.contains(restated), "{restated:?} in {text}");
    }
    let presence = match present {
        [] => None,
        [one] => Some(format!("{one} is here.")),
        [rest @ .., last] => Some(format!("{} and {last} are here.", rest.join(", "))),
    };
    match presence {
        None => assert!(!text.contains(" here."), "{text}"),
        Some(line) => {
            let line = limerick_core::ipc::capitalize_first(&line);
            assert!(text.ends_with(&format!("\n\n{line}")), "{text}");
        }
    }
}

#[test]
fn arriving_somewhere_shows_one_scene_with_its_name_description_and_who_is_there() {
    let (game, _) = Game::new();
    let moved = game.go_to_the_cottage();
    let scenes = events_of_kind(&moved, "scene_changed");
    assert_eq!(scenes.len(), 1, "{moved}");
    let scene = scenes[0];
    assert_eq!(scene["metadata"]["sceneName"], "Connolly Cottage");
    assert_eq!(scene["metadata"]["sceneID"], "3");
    let text = scene["content"].as_str().unwrap();
    assert!(
        text.starts_with("A peat fire warms the single room."),
        "{text}"
    );
    assert_scene_body(text, &people(&game.snapshot()));
    for narration in events_of_kind(&moved, "narration") {
        let line = narration["content"].as_str().unwrap();
        assert!(!line.contains("A peat fire"), "described twice: {moved}");
        assert!(!line.contains("You can go to"), "exits listed: {moved}");
    }
}

/// The one scene a move shows, as `(title, text)`.
fn arrival(moved: &Value) -> (String, String) {
    let scenes = events_of_kind(moved, "scene_changed");
    assert_eq!(scenes.len(), 1, "{moved}");
    (
        scenes[0]["metadata"]["sceneName"]
            .as_str()
            .unwrap()
            .to_string(),
        scenes[0]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

/// Phase 4 test plan, arrival feedback: a place the player has been shows
/// who is there without repeating its description, also after a relaunch;
/// an empty one shows no text; `look` still describes it.
#[test]
fn returning_somewhere_lists_who_is_there_without_repeating_the_description() {
    let (mut game, _) = Game::new();
    let (_, first) = arrival(&game.go_to_the_cottage());
    assert!(
        first.starts_with("A peat fire warms the single room."),
        "{first}"
    );

    // The opening scene described the village, so this is a return.
    let (title, text) = arrival(&game.submit("go to Kilteevan Village"));
    assert_eq!(title, "Kilteevan Village");
    assert!(!text.contains("muddy road"), "described again: {text}");
    let presence = people(&game.snapshot());
    assert_eq!(
        presence.len(),
        1,
        "Peig is on the village road: {presence:?}"
    );
    assert_eq!(
        text,
        limerick_core::ipc::capitalize_first(&format!("{} is here.", presence[0]))
    );

    // A first visit is described, and an empty place lists no one.
    let (title, text) = arrival(&game.submit("go to the Letter Office"));
    assert_eq!(title, "Letter Office");
    assert!(text.starts_with("A narrow counter"), "{text}");
    assert!(!text.contains(" here."), "{text}");

    game.relaunch();
    game.submit("go to Kilteevan Village");
    let (title, text) = arrival(&game.submit("go to the Letter Office"));
    assert_eq!(title, "Letter Office");
    assert_eq!(text, "", "an empty place visited before has no text");

    let (_, text) = arrival(&game.submit("go to Kilteevan Village"));
    assert!(!text.contains("muddy road"), "{text}");
    let (_, text) = arrival(&game.submit("go to Connolly Cottage"));
    assert!(
        !text.contains("A peat fire"),
        "described after relaunch: {text}"
    );
    assert!(text.ends_with("are here."), "{text}");

    let looked = game.submit("look");
    let described = events_of_kind(&looked, "narration")
        .iter()
        .any(|event| event["content"].as_str().unwrap().contains("A peat fire"));
    assert!(described, "look still describes the room: {looked}");
}

// Oracle: `successful_candidate_commits_one_exchange_and_retry_is_rejected`,
// `acceptance_precedes_endpoint_and_has_grounding`, and RundaleKit
// `testCompletionCommitsAndLateOldAttemptCannotWin`.
#[test]
fn a_talk_turn_streams_provisional_text_and_commits_the_endpoint_reply_once() {
    let (game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, how are the cattle this week?");
    assert_eq!(submitted["accepted"], true);
    assert_eq!(
        kinds(&submitted),
        ["player_command"],
        "acceptance precedes any call"
    );
    let command = &submitted["events"][0];
    assert_eq!(command["accepted"], true);
    assert_eq!(command["sourceDraftID"], "draft-1");
    assert_eq!(command["attemptID"], submitted["attemptID"]);
    let request = submitted["logicalRequestID"].clone();

    let dialogue = game.until_dialogue(submitted);
    assert_eq!(
        dialogue["endpoint"],
        json!({"role": "dialogue", "slug": "rundale-dialogue", "version": 1})
    );
    assert_eq!(dialogue["stream"], true, "the dialogue definition streams");
    let input = &dialogue["input"];
    assert_eq!(input["role"], "npc_dialogue");
    assert_eq!(input["speaker"]["displayName"], "Mícheál Connolly");
    assert_eq!(input["logicalRequestID"], request);
    assert_eq!(
        input["idempotencyKey"],
        format!(
            "{}:{}",
            request.as_str().unwrap(),
            dialogue["attemptID"].as_str().unwrap()
        )
    );
    assert_eq!(input["currentLocation"]["displayName"], "Connolly Cottage");

    let first = game.frame(&dialogue, 1, "The wet ground ");
    let provisional = &first["events"][0];
    assert_eq!(provisional["provisional"], true);
    assert_eq!(provisional["streamUpdate"], "append");
    let speaker = provisional["speaker"].clone();
    assert!(speaker.is_string(), "{provisional}");
    assert_eq!(provisional["content"], "The wet ground ");
    let before = game.snapshot();
    assert_eq!(
        provisional["sequence"], before["eventCursor"],
        "a provisional frame does not advance the durable cursor"
    );
    game.frame(&dialogue, 2, "has made moving cattle hard.");

    let committed = game.resolve(&dialogue, json!({"dialogue": CATTLE_LINE}));
    assert_eq!(committed["terminalOutcome"], "succeeded", "{committed}");
    let lines = events_of_kind(&committed, "npc_dialogue");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["content"], CATTLE_LINE);
    assert_eq!(
        lines[0]["speaker"], speaker,
        "the provisional row has the committed speaker label"
    );
    assert_eq!(
        lines[0]["transcriptItemID"], provisional["transcriptItemID"],
        "the committed line replaces the streamed row in place"
    );
    assert_eq!(lines[0]["provisional"], false);
    let completion = events_of_kind(&committed, "response_completed");
    assert_eq!(completion.len(), 1);
    let seqs = sequences(&committed);
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]), "{seqs:?}");

    let after = game.snapshot();
    assert_eq!(revision(&after), revision(&before) + 1);
    assert_eq!(completion[0]["stateRevision"], after["stateRevision"]);
    assert_eq!(after["pendingInference"], false);
    let record = request_record(&after, &request);
    assert_eq!(record["phase"], "completed");
    assert_eq!(record["committedStateRevision"], after["stateRevision"]);

    // Late callbacks for the committed attempt change nothing.
    let late = game.resolve(&dialogue, json!({"dialogue": "A second answer."}));
    assert_eq!(late["ignored"], true);
    assert!(late["events"].as_array().unwrap().is_empty());
    assert_eq!(game.frame(&dialogue, 3, "late")["ignored"], true);
    let (status, retry) = game.dispatch(json!({"op": "retry", "logical_request_id": request}));
    assert_ne!(status, limerick_mobile_status_t::LIMERICK_MOBILE_OK);
    assert_eq!(retry["error"]["code"], "rejected");
    assert_eq!(revision(&game.snapshot()), revision(&after));
}

// Oracle: `stop_wins_and_late_candidate_cannot_commit` and RundaleKit
// `testLateCallbacksFromCancelledAttemptCannotCommit`,
// `testRetryCreatesCurrentAttemptBeforeRejectingOldAttemptEvents`.
#[test]
fn stop_wins_and_a_late_result_cannot_commit_until_a_retry_runs_a_new_attempt() {
    let (game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, how are the cattle this week?");
    let request = submitted["logicalRequestID"].clone();
    let dialogue = game.until_dialogue(submitted);
    game.frame(&dialogue, 1, "The wet ");
    let base = revision(&game.snapshot());

    let stopped = game.op(json!({"op": "stop"}));
    assert_eq!(stopped["terminalOutcome"], "cancelled");
    assert_eq!(stopped["logicalRequestID"], request);
    assert!(events_of_kind(&stopped, "npc_dialogue").is_empty());
    assert!(game.pending().is_null());

    let late = game.resolve(&dialogue, json!({"dialogue": CATTLE_LINE}));
    assert_eq!(late["ignored"], true);
    assert_eq!(game.frame(&dialogue, 2, "ground")["ignored"], true);
    let after_stop = game.snapshot();
    assert_eq!(
        revision(&after_stop),
        base,
        "a stopped attempt commits nothing"
    );
    assert_eq!(request_record(&after_stop, &request)["phase"], "cancelled");
    let nothing = game.op(json!({"op": "stop"}));
    assert_eq!(nothing["ignored"], true, "nothing is open to stop");

    let retried = game.op(json!({"op": "retry", "logical_request_id": request}));
    assert_eq!(retried["logicalRequestID"], request);
    assert_ne!(retried["attemptID"], dialogue["attemptID"]);
    let first = &retried["events"][0];
    assert_eq!(first["kind"], "progress");
    assert_eq!(first["metadata"]["retry"], "true");
    assert_eq!(
        first["attemptID"], retried["attemptID"],
        "the retry marker opens the new attempt"
    );

    let again = game.until_dialogue(retried);
    let stale = game.resolve(&dialogue, json!({"dialogue": "From the stopped attempt."}));
    assert_eq!(
        stale["ignored"], true,
        "the old attempt cannot answer the new one"
    );
    let committed = game.resolve(&again, json!({"dialogue": CATTLE_LINE}));
    assert_eq!(committed["terminalOutcome"], "succeeded");
    assert_eq!(revision(&game.snapshot()), base + 1);
}

/// A failed dialogue call shows the mod's line for the reason the host gave
/// (`mods/rundale/loading.toml` `[failure_lines]`), or the engine's generic
/// retry line when the host gave none.
#[test]
fn a_failed_call_shows_the_mods_line_for_its_reason() {
    let game_mod =
        limerick_core::game_mod::GameMod::load(&repo_root().join("mods/rundale")).unwrap();
    let lines = &game_mod.loading.failure_lines;
    let mut shown = std::collections::BTreeSet::new();
    for reason in limerick_core::turn_inference::FailureReason::ALL
        .map(|reason| Some(reason.key()))
        .into_iter()
        .chain([None])
    {
        let (game, _) = Game::new();
        game.go_to_the_cottage();
        let submitted = game.submit("Mícheál, how are the cattle this week?");
        let dialogue = game.until_dialogue(submitted);
        let mut failure = json!({
            "op": "fail",
            "call_id": dialogue["callID"],
            "attempt_id": dialogue["attemptID"],
            "base_revision": dialogue["baseRevision"],
            "error_kind": "transport",
            "message": "diagnostic only",
        });
        if let Some(reason) = reason {
            failure["reason"] = json!(reason);
        }
        let failed = game.op(failure);
        assert_eq!(failed["terminalOutcome"], "failed", "{failed}");
        let errors = events_of_kind(&failed, "error");
        let line = errors[0]["content"].as_str().unwrap();
        assert!(!line.contains("diagnostic only"), "{line}");
        match reason {
            Some(reason) => assert_eq!(line, lines[reason], "{reason}"),
            None => assert_eq!(
                line,
                limerick_core::game_loop::npc_turn::DIALOGUE_RETRY_MESSAGE
            ),
        }
        shown.insert(line.to_string());
    }
    assert_eq!(shown.len(), 8, "every reason has its own line");

    let (game, _) = Game::new();
    game.go_to_the_cottage();
    let dialogue = game.until_dialogue(game.submit("Mícheál, how are the cattle?"));
    let (status, envelope) = game.dispatch(json!({
        "op": "fail",
        "call_id": dialogue["callID"],
        "attempt_id": dialogue["attemptID"],
        "base_revision": dialogue["baseRevision"],
        "error_kind": "transport",
        "reason": "gremlins",
    }));
    assert_eq!(
        status,
        limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR
    );
    assert_eq!(envelope["error"]["code"], "protocol_error", "{envelope}");
}

// Oracle: `failed_attempt_can_retry_with_new_attempt_identity`,
// `correlated_endpoint_failure_is_terminal_and_retryable`,
// `endpoint_failure_requires_the_invocation_base_revision`, and RundaleKit
// `testFailedCompletionCannotAdvanceCommittedStateRevision`.
#[test]
fn an_endpoint_failure_ends_the_attempt_failed_and_a_retry_can_commit() {
    let (game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, how are the cattle this week?");
    let request = submitted["logicalRequestID"].clone();
    let dialogue = game.until_dialogue(submitted);
    let base = revision(&game.snapshot());

    let wrong_revision = game.op(json!({
        "op": "fail",
        "call_id": dialogue["callID"],
        "attempt_id": dialogue["attemptID"],
        "base_revision": base + 7,
        "error_kind": "transport",
        "message": "offline",
    }));
    assert_eq!(
        wrong_revision["ignored"], true,
        "a failure must carry the base revision"
    );

    let failed = game.op(json!({
        "op": "fail",
        "call_id": dialogue["callID"],
        "attempt_id": dialogue["attemptID"],
        "base_revision": dialogue["baseRevision"],
        "error_kind": "transport",
        "message": "offline",
    }));
    assert_eq!(failed["terminalOutcome"], "failed", "{failed}");
    assert!(failed["error"].is_string());
    let completion = events_of_kind(&failed, "response_completed");
    assert_eq!(completion[0]["terminalOutcome"], "failed");
    assert!(
        completion[0].get("stateRevision").is_none(),
        "only success carries a revision"
    );
    assert_eq!(revision(&game.snapshot()), base);
    assert_eq!(
        request_record(&game.snapshot(), &request)["phase"],
        "failed"
    );

    let retried = game.op(json!({"op": "retry", "logical_request_id": request}));
    assert_ne!(retried["attemptID"], dialogue["attemptID"]);
    let again = game.until_dialogue(retried);
    assert_eq!(
        again["input"]["logicalRequestID"], request,
        "a retry keeps the logical request"
    );
    let committed = game.resolve(&again, json!({"dialogue": CATTLE_LINE}));
    assert_eq!(committed["terminalOutcome"], "succeeded");
    let record = request_record(&game.snapshot(), &request).clone();
    assert_eq!(record["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(record["attempts"][0]["terminalOutcome"], "failed");
    assert_eq!(record["attempts"][1]["terminalOutcome"], "succeeded");
}

// Oracle: `accepted_restart_becomes_interrupted_and_does_not_rerun` and
// RundaleKit `testRestoredAdapterSuppressesOpeningReplayAndInterruptsActiveAttempt`.
#[test]
fn a_relaunch_interrupts_the_open_request_without_rerunning_it() {
    let (mut game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, how are the cattle this week?");
    let request = submitted["logicalRequestID"].clone();
    let dialogue = game.until_dialogue(submitted);
    let base = revision(&game.snapshot());

    let resumed = game.relaunch();
    assert_eq!(revision(&resumed), base);
    assert!(resumed["activeRequestID"].is_null());
    assert_eq!(resumed["pendingInference"], false);
    assert!(game.pending().is_null(), "nothing re-runs after restart");
    assert_eq!(request_record(&resumed, &request)["phase"], "interrupted");
    let tail = resumed["events"].as_array().unwrap();
    let notice = tail
        .iter()
        .rev()
        .find(|event| event["kind"] == "narration")
        .unwrap();
    assert_eq!(notice["content"], INTERRUPTED_MESSAGE);
    assert_eq!(
        tail.last().unwrap()["terminalOutcome"],
        "interrupted",
        "{}",
        tail.last().unwrap()
    );

    let late = game.resolve(&dialogue, json!({"dialogue": CATTLE_LINE}));
    assert_eq!(late["ignored"], true);
    let retried = game.op(json!({"op": "retry", "logical_request_id": request}));
    let again = game.until_dialogue(retried);
    assert_eq!(
        game.resolve(&again, json!({"dialogue": CATTLE_LINE}))["terminalOutcome"],
        "succeeded"
    );
}

// Oracle: `phase3_ambiguity_survives_resume_and_selection_continues_original_request`,
// ios-port FFI `unresolved_clarification_survives_bounded_restart_projection`,
// and RundaleKit `testFixtureClarificationContinuesSameLogicalRequest`.
#[test]
fn an_ambiguous_addressee_asks_and_the_answer_survives_restart_and_continues_the_request() {
    let (mut game, _) = Game::with_two_drovers();
    game.go_to_the_cottage();
    let mut result = game.submit("Drover, is it a good day for the fair?");
    let request = result["logicalRequestID"].clone();
    for _ in 0..3 {
        if result["status"] != "awaiting_inference" {
            break;
        }
        let pending = game.pending();
        assert_eq!(pending["endpoint"]["role"], "intent", "{pending}");
        result = game.resolve(
            &pending,
            json!({"intent": "talk", "target": "Drover", "dialogue": "is it a good day for the fair?", "atmosphere": null}),
        );
    }
    assert_eq!(result["status"], "awaiting_clarification", "{result}");
    let question = events_of_kind(&result, "clarification_required");
    let prompt = &question[0]["clarification"];
    let choices = prompt["choices"].as_array().unwrap();
    assert_eq!(choices.len(), 2, "{prompt}");
    let mut entities: Vec<&str> = choices
        .iter()
        .map(|choice| choice["entityID"].as_str().unwrap())
        .collect();
    entities.sort();
    assert_eq!(entities, ["2", "3"], "Mícheál and Róisín");

    let resumed = game.relaunch();
    let record = request_record(&resumed, &request);
    assert_eq!(
        record["phase"], "awaiting_clarification",
        "the question survives restart"
    );
    assert_eq!(resumed["activeRequestID"], request);
    let choices = record["pendingClarification"]["choices"]
        .as_array()
        .unwrap();
    assert_eq!(choices.len(), 2);
    let roisin = choices
        .iter()
        .find(|choice| choice["entityID"] == "3")
        .unwrap();
    let choice = roisin["id"].clone();

    let answered = game.op(json!({
        "op": "answer_clarification",
        "logical_request_id": request,
        "choice_id": choice,
    }));
    assert_eq!(
        answered["logicalRequestID"], request,
        "the answer continues the same request"
    );
    assert_eq!(kinds(&answered)[0], "clarification_selected");
    let dialogue = game.until_dialogue(answered);
    assert_eq!(
        dialogue["input"]["speaker"]["displayName"],
        "Róisín Connolly"
    );
    let committed = game.resolve(
        &dialogue,
        json!({"dialogue": "It is, if the rain holds off."}),
    );
    assert_eq!(committed["terminalOutcome"], "succeeded");
}

/// How the intent Endpoint answered "ask Connolly about the household".
enum IntentAnswer {
    Target(&'static str),
    Failed,
}

/// Product spec §5.3: "ask Connolly" at the cottage names both Connollys,
/// so the turn asks which one rather than speaking to whoever is listed
/// first, on the canonical world through the phone's intent Endpoint. The
/// player's own words name who is asked, so the question does not depend on
/// the model: it returns "Connolly", a garbled target (seen live:
/// "Connolly.mdl"), or fails (seen live: 502 MODEL_ERROR).
#[test]
fn a_family_name_shared_at_the_cottage_asks_which_connolly() {
    for answer in [
        IntentAnswer::Target("Connolly"),
        IntentAnswer::Target("Connolly.mdl"),
        IntentAnswer::Failed,
    ] {
        let (game, _) = Game::new();
        game.go_to_the_cottage();
        let mut result = game.submit("ask Connolly about the household");
        for _ in 0..3 {
            if result["status"] != "awaiting_inference" {
                break;
            }
            let pending = game.pending();
            assert_eq!(pending["endpoint"]["role"], "intent", "{pending}");
            result = match answer {
                IntentAnswer::Target(target) => game.resolve(
                    &pending,
                    json!({"intent": "talk", "target": target, "dialogue": null, "atmosphere": null}),
                ),
                IntentAnswer::Failed => game.op(json!({
                    "op": "fail",
                    "call_id": pending["callID"],
                    "attempt_id": pending["attemptID"],
                    "base_revision": pending["baseRevision"],
                    "error_kind": "protocol",
                    "message": "MODEL_ERROR",
                })),
            };
        }
        assert_eq!(result["status"], "awaiting_clarification", "{result}");
        assert!(
            events_of_kind(&result, "npc_dialogue").is_empty(),
            "no one answers before the player chooses: {result}"
        );
        let question = events_of_kind(&result, "clarification_required");
        let prompt = &question[0]["clarification"];
        let mut entities: Vec<&str> = prompt["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|choice| choice["entityID"].as_str().unwrap())
            .collect();
        entities.sort();
        assert_eq!(entities, ["2", "3"], "Mícheál and Róisín: {prompt}");
    }
}

/// Submits `text` and answers any intent call (naming `target`) until the
/// turn ends or waits on a dialogue call; returns the last result.
fn submit_through_intent(game: &Game, text: &str, target: &str) -> Value {
    let mut result = game.submit(text);
    for _ in 0..2 {
        let pending = game.pending();
        if pending.is_null() || pending["endpoint"]["role"] != "intent" {
            break;
        }
        result = game.resolve(
            &pending,
            json!({"intent": "talk", "target": target, "dialogue": null, "atmosphere": null}),
        );
    }
    result
}

fn narration(result: &Value) -> Vec<&str> {
    events_of_kind(result, "narration")
        .iter()
        .map(|event| event["content"].as_str().unwrap())
        .collect()
}

/// Spec Milestone 3: the player cannot converse with someone who is not
/// there. Mícheál is at Connolly Cottage all morning, so asking for him in
/// the village is answered "not here", never by whoever is in the village
/// and never through the dialogue Endpoint (#2047).
#[test]
fn a_person_named_who_is_elsewhere_is_reported_absent_without_an_endpoint_call() {
    let (game, _) = Game::new();
    let mut checked_with_peig = false;
    for with_peig in [false, true] {
        if with_peig {
            // 08:00: Peig waits on the village road for the post.
            command(&game, "/wait 60");
        }
        let present = people(&game.snapshot());
        assert_eq!(present.is_empty(), !with_peig, "{present:?}");
        for text in [
            "ask Mícheál about the cattle",
            "Mícheál, how are the cattle?",
        ] {
            let result = submit_through_intent(&game, text, "Mícheál");
            assert!(game.pending().is_null(), "{text}: {}", game.pending());
            assert_eq!(result["terminalOutcome"], "succeeded", "{result}");
            assert!(
                events_of_kind(&result, "npc_dialogue").is_empty(),
                "{result}"
            );
            assert_eq!(narration(&result), ["Mícheál is not here."], "{text}");
        }
        checked_with_peig = with_peig;
    }
    assert!(checked_with_peig);

    // Speaking to Peig about Mícheál still reaches Peig.
    let asked = submit_through_intent(&game, "Peig, have you seen Mícheál?", "Peig");
    let dialogue = game.pending();
    assert_eq!(dialogue["endpoint"]["role"], "dialogue", "{asked}");
    assert!(
        !narration(&asked)
            .iter()
            .any(|line| line.contains("not here")),
        "{asked}"
    );
}

/// The Endpoint path commits a reply after structural checks only: a reply
/// naming a person and place the world does not have is not rewritten.
#[test]
fn dialogue_content_guards_are_off_on_the_endpoint_path() {
    let (game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, who keeps the chapel in Tulsk?");
    let dialogue = game.until_dialogue(submitted);
    let reply = "Father Brennan keeps the chapel in Tulsk, and he'll see you after Mass.";
    let committed = game.resolve(&dialogue, json!({"dialogue": reply}));
    assert_eq!(committed["terminalOutcome"], "succeeded", "{committed}");
    assert_eq!(
        events_of_kind(&committed, "npc_dialogue")[0]["content"],
        reply
    );
}

#[test]
fn transcript_pages_are_bounded_in_both_directions() {
    let (game, _) = Game::new();
    game.submit("go to the Letter Office");
    game.submit("go to Kilteevan Village");
    let all = game.op(json!({"op": "read_events", "limit": 100}));
    let every = sequences(&all);
    assert!(every.len() > 4, "{every:?}");
    assert_eq!(all["hasMore"], false);

    let first = game.op(json!({"op": "read_events", "after": 0, "limit": 2}));
    assert_eq!(sequences(&first), every[..2]);
    assert_eq!(first["hasMore"], true);
    assert_eq!(first["cursor"]["rawValue"], every[1]);
    let next = game.op(json!({"op": "read_events", "after": {"rawValue": every[1]}, "limit": 100}));
    assert_eq!(sequences(&next), every[2..]);

    let newest = game.op(json!({"op": "read_event_page_before", "before": u64::MAX, "limit": 3}));
    assert_eq!(sequences(&newest), every[every.len() - 3..]);
    assert_eq!(newest["hasMore"], true);
    let older = game.op(json!({
        "op": "read_event_page_before",
        "before": newest["cursor"],
        "limit": 100,
    }));
    assert_eq!(sequences(&older), every[..every.len() - 3]);
    assert_eq!(older["hasMore"], false);

    let (status, envelope) = game.dispatch(json!({"op": "read_events", "limit": 101}));
    assert_eq!(
        status,
        limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR
    );
    assert_eq!(envelope["error"]["code"], "protocol_error");
}

#[test]
fn a_second_submission_while_a_request_is_open_is_rejected() {
    let (game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, how are the cattle this week?");
    game.until_dialogue(submitted);
    let (status, envelope) =
        game.dispatch(json!({"op": "submit", "text": "go to the Letter Office"}));
    assert_ne!(status, limerick_mobile_status_t::LIMERICK_MOBILE_OK);
    assert_eq!(envelope["error"]["code"], "request_in_progress");
}

/// ADR-025 §4: a save this build cannot read is kept unchanged and a new
/// game starts, with a notice, instead of leaving the player stuck.
#[test]
fn an_unreadable_save_is_kept_aside_and_a_new_game_starts() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("game.sqlite");
    let garbage = b"not a save database".to_vec();
    std::fs::write(&save, &garbage).unwrap();
    let (status, handle, envelope) = open_raw(
        limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME,
        &Game::request(&dir),
    );
    assert_eq!(
        status,
        limerick_mobile_status_t::LIMERICK_MOBILE_OK,
        "{envelope}"
    );
    let events = envelope["value"]["events"].as_array().unwrap().clone();
    assert_eq!(events.len(), 2);
    assert!(
        events[0]["content"]
            .as_str()
            .unwrap()
            .starts_with("A muddy road")
    );
    assert_eq!(
        events[1]["content"],
        limerick_core::save_compat::INCOMPATIBLE_SAVE_AT_LAUNCH_MESSAGE
    );
    let kept: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().contains(".refused-"))
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(
        std::fs::read(&kept[0]).unwrap(),
        garbage,
        "left exactly as it was"
    );
    assert_eq!(
        limerick_mobile_close(handle),
        limerick_mobile_status_t::LIMERICK_MOBILE_OK
    );
}

#[test]
fn open_rejects_malformed_payloads() {
    let cases = [
        (
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
            "not json",
        ),
        (limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW, "[]"),
        (
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME,
            "{}",
        ),
        (
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME,
            r#"{"save_path":"/tmp/x.sqlite"}"#,
        ),
    ];
    for (kind, request) in cases {
        let (status, handle, envelope) = open_raw(kind, request);
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
            "{request}"
        );
        assert_eq!(handle, 0);
        assert_eq!(envelope["error"]["code"], "protocol_error", "{request}");
    }
}

#[test]
fn open_reports_missing_content() {
    let dir = tempfile::tempdir().unwrap();
    let request = json!({
        "save_path": dir.path().join("game.sqlite"),
        "mod_dir": dir.path().join("no-such-mod"),
    })
    .to_string();
    let (status, handle, envelope) = open_raw(
        limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME,
        &request,
    );
    assert_eq!(
        status,
        limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
    );
    assert_eq!(handle, 0);
    assert_eq!(envelope["error"]["code"], "content_unavailable");
    assert!(
        !dir.path().join("game.sqlite").exists(),
        "no save is created"
    );
}

#[test]
fn open_rejects_invalid_utf8_and_oversized_payloads() {
    let invalid = [0xff_u8, 0xfe];
    let mut handle = 0;
    let mut response = empty_owned();
    let status = limerick_mobile_open(
        limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
        limerick_mobile_bytes_t {
            ptr: invalid.as_ptr(),
            len: invalid.len(),
        },
        &mut handle,
        &mut response,
    );
    assert_eq!(
        status,
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_UTF8
    );
    assert_eq!(take(response)["error"]["code"], "invalid_utf8");

    let oversized = "x".repeat(MAX_REQUEST_BYTES + 1);
    let (status, _, envelope) = open_raw(
        limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
        &oversized,
    );
    assert_eq!(status, limerick_mobile_status_t::LIMERICK_MOBILE_TOO_LARGE);
    assert_eq!(envelope["error"]["code"], "too_large");
}

#[test]
fn open_rejects_null_outputs() {
    let request = "{}";
    let mut response = empty_owned();
    assert_eq!(
        limerick_mobile_open(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
            borrowed(request),
            ptr::null_mut(),
            &mut response,
        ),
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT
    );
    let mut handle = 0;
    assert_eq!(
        limerick_mobile_open(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
            borrowed(request),
            &mut handle,
            ptr::null_mut(),
        ),
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT
    );
}

#[test]
fn dispatch_rejects_malformed_operations_and_unknown_handles() {
    let (game, _) = Game::new();
    for operation in [
        "",
        "nope",
        "[]",
        r#"{"text":"no op"}"#,
        r#"{"op":7}"#,
        r#"{"op":"dance"}"#,
    ] {
        let mut response = empty_owned();
        let status = limerick_mobile_dispatch(game.handle, borrowed(operation), &mut response);
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
            "{operation}"
        );
        assert_eq!(
            take(response)["error"]["code"],
            "protocol_error",
            "{operation}"
        );
    }
    let mut response = empty_owned();
    let status =
        limerick_mobile_dispatch(u64::MAX, borrowed(r#"{"op":"snapshot"}"#), &mut response);
    assert_eq!(
        status,
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE
    );
    assert_eq!(take(response)["error"]["code"], "invalid_handle");
}

#[test]
fn a_closed_handle_is_invalid() {
    let (game, _) = Game::new();
    let handle = game.handle;
    assert_eq!(
        limerick_mobile_close(handle),
        limerick_mobile_status_t::LIMERICK_MOBILE_OK
    );
    assert_eq!(
        limerick_mobile_close(handle),
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE
    );
    let mut response = empty_owned();
    let status = limerick_mobile_dispatch(handle, borrowed(r#"{"op":"snapshot"}"#), &mut response);
    assert_eq!(
        status,
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE
    );
    take(response);
}

#[test]
fn free_accepts_empty_and_rejects_dangling_lengths() {
    assert_eq!(
        limerick_mobile_owned_bytes_free(empty_owned()),
        limerick_mobile_status_t::LIMERICK_MOBILE_OK
    );
    assert_eq!(
        limerick_mobile_owned_bytes_free(limerick_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 4,
        }),
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT
    );
}

#[test]
fn panics_are_contained_as_internal_errors() {
    assert_eq!(
        panic_contained(|| panic!("boom")),
        limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
    );
}

#[test]
fn the_endpoint_configuration_turns_every_content_guard_off() {
    let config = session::endpoint_game_config();
    assert!(
        config
            .flags
            .is_disabled(limerick_core::npc::DIALOGUE_CONTENT_GUARDS_FLAG)
    );
}

/// The Swift package vendors a copy of the C header next to its module
/// map. The two copies must not drift.
#[test]
fn swift_bridge_header_matches_crate_header() {
    let crate_header = include_str!("../include/limerick_mobile_ffi.h");
    let bridge_header = include_str!(
        "../../../../mobile/RundaleBridge/Sources/LimerickMobileFFI/include/limerick_mobile_ffi.h"
    );
    assert_eq!(crate_header, bridge_header);
}

#[test]
fn slash_commands_the_phone_does_not_offer_are_refused_without_a_request() {
    let (game, _) = Game::new();
    let before = game.snapshot();
    for text in ["/save", "  /quit", "/provider openai", "/nonsense"] {
        let (status, envelope) = game.dispatch(json!({"op": "submit", "text": text}));
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR
        );
        assert_eq!(envelope["error"]["code"], "command_unavailable");
        let message = envelope["error"]["message"].as_str().unwrap();
        assert!(message.contains("/help"), "{message}");
        assert!(
            !message.contains("/wait"),
            "names no hidden command: {message}"
        );
    }
    assert!(game.pending().is_null(), "no Endpoint call is made");
    assert_eq!(game.snapshot(), before, "nothing is journaled");
}

/// Submits a slash command and returns its one-line-or-more answer, after
/// checking it ran as an ordinary request with no Endpoint call.
fn command(game: &Game, text: &str) -> String {
    let result = game.submit(text);
    assert_eq!(result["accepted"], true, "{result}");
    assert_eq!(result["terminalOutcome"], "succeeded", "{result}");
    assert!(game.pending().is_null(), "{text} made an Endpoint call");
    let commands = events_of_kind(&result, "player_command");
    assert_eq!(commands[0]["content"], text, "{result}");
    events_of_kind(&result, "narration")
        .iter()
        .map(|event| event["content"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// #2120 and product spec §5.4: the phone's slash commands run as local
/// turns on the shared engine.
#[test]
fn the_phone_slash_commands_answer_locally_as_turns() {
    let (game, opening) = Game::new();
    let names: Vec<&str> = opening["readModel"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["/look", "/people", "/exits", "/help"], "advertised");

    let help = command(&game, "/help");
    for advertised in names {
        assert!(help.contains(advertised), "{help}");
    }
    for hidden in ["/wait", "/pause", "/resume", "/debug", "/flags"] {
        assert!(!help.contains(hidden), "{hidden} is not advertised: {help}");
    }

    let looked = command(&game, "/look");
    assert!(
        looked.starts_with("A muddy road runs between low stone walls"),
        "{looked}"
    );
    let exits = command(&game, "/exits");
    assert!(
        exits.contains("Letter Office") && exits.contains("Connolly Cottage"),
        "{exits}"
    );

    game.go_to_the_cottage();
    let people = command(&game, "/people");
    assert!(
        people.contains("cattle drover") && people.contains("Spinner"),
        "{people}"
    );
    assert_eq!(command(&game, "/npcs"), people, "/npcs is /people");

    let debug = command(&game, "/debug");
    assert!(debug.contains("[DEBUG OVERVIEW]"), "{debug}");
    let flags = command(&game, "/flags");
    assert!(!flags.trim().is_empty(), "{flags}");

    let paused = command(&game, "/pause");
    assert_eq!(paused, "The clocks of the parish stand still.");
    assert!(
        command(&game, "/debug clock").contains("Paused: yes"),
        "the pause commits"
    );
    command(&game, "/resume");
    assert!(!command(&game, "/debug clock").contains("Paused: yes"));

    let waited = command(&game, "/wait 240");
    assert!(waited.contains("You wait for 240 minutes"), "{waited}");
    let clock = command(&game, "/debug clock");
    assert!(
        clock.contains("Game time: 11:0"),
        "the wait commits: {clock}"
    );
}

/// #2146: the read model carries the completion tree for every command the
/// phone runs, and every NPC a `/debug` name may complete to; a name typed
/// without fadas resolves on the engine.
#[test]
fn the_read_model_offers_every_command_and_debug_names_match_without_fadas() {
    let (game, opening) = Game::new();
    let read_model = &opening["readModel"];
    let words = |tree: &Value| -> Vec<String> {
        tree.as_array()
            .unwrap()
            .iter()
            .map(|word| word["word"].as_str().unwrap().to_string())
            .collect()
    };
    let completions = &read_model["commandCompletions"];
    assert_eq!(
        words(completions),
        [
            "/look", "/people", "/exits", "/help", "/wait", "/pause", "/resume", "/debug", "/flags"
        ]
    );
    let debug = completions
        .as_array()
        .unwrap()
        .iter()
        .find(|word| word["word"] == "/debug")
        .unwrap();
    assert_eq!(
        words(&debug["next"])[..4],
        ["memory", "schedule", "relationships", "gossip"]
    );
    assert_eq!(debug["next"][0]["takesNpc"], true);
    assert_eq!(debug["next"][4]["word"], "clock");
    assert_eq!(debug["next"][4]["takesNpc"], false);

    let everyone: Vec<&str> = read_model["everyone"]
        .as_array()
        .unwrap()
        .iter()
        .map(|npc| npc["name"].as_str().unwrap())
        .collect();
    for name in ["Peig Hannigan", "Mícheál Connolly", "Róisín Connolly"] {
        assert!(everyone.contains(&name), "{name} in {everyone:?}");
    }

    // Away from the cottage: every NPC, not only nearby ones.
    let memory = command(&game, "/debug memory micheal");
    assert!(
        memory.starts_with("[DEBUG MEMORY: Mícheál Connolly]"),
        "{memory}"
    );
    let schedule = command(&game, "/debug schedule roisin");
    assert!(
        schedule.starts_with("[DEBUG SCHEDULE: Róisín Connolly]"),
        "{schedule}"
    );
}

fn bug_report(game: &Game, description: &str) -> String {
    let report = game.op(json!({
        "op": "bug_report",
        "description": description,
        "build": "0.1 (42)",
    }));
    let text = report["text"].as_str().unwrap().to_string();
    assert_eq!(
        report["characters"].as_u64().unwrap() as usize,
        text.chars().count()
    );
    assert!(text.chars().count() <= limerick_diagnostics::mobile_report::REPORT_BUDGET);
    text
}

/// What a bug report must leave alone: the journal, the revision, and the
/// requests (#2022).
fn untouched(game: &Game) -> (Value, Value, Value) {
    let snapshot = game.snapshot();
    (
        snapshot["eventCursor"].clone(),
        snapshot["stateRevision"].clone(),
        snapshot["requests"].clone(),
    )
}

#[test]
fn a_bug_report_carries_the_scene_transcript_and_answered_calls_and_changes_nothing() {
    let (game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, how are the cattle this week?");
    let dialogue = game.until_dialogue(submitted);

    // A stale answer is ignored by the engine and must not be reported.
    game.op(json!({
        "op": "fail",
        "call_id": dialogue["callID"],
        "attempt_id": dialogue["attemptID"],
        "base_revision": dialogue["baseRevision"]["rawValue"].as_u64().unwrap() + 7,
        "error_kind": "transport",
        "message": "stale",
    }));
    assert_eq!(
        game.pending()["callID"],
        dialogue["callID"],
        "a stale answer leaves the awaited call pending"
    );
    let waiting = bug_report(&game, "");
    assert!(
        waiting.contains("Open request: waiting on rundale-dialogue.v1\n"),
        "{waiting}"
    );
    assert!(!waiting.contains("stale"), "{waiting}");

    game.op(json!({
        "op": "fail",
        "call_id": dialogue["callID"],
        "attempt_id": dialogue["attemptID"],
        "base_revision": dialogue["baseRevision"],
        "error_kind": "timed_out",
        "reason": "offline",
        "message": "the network went away",
    }));

    let before = untouched(&game);
    let text = bug_report(&game, "Mícheál never answered");
    assert_eq!(untouched(&game), before, "a bug report changes nothing");

    assert!(
        text.starts_with("Rundale bug report\nMícheál never answered\n"),
        "{text}"
    );
    assert!(
        text.contains("Build: 0.1 (42) · engine contract 1.0\n"),
        "{text}"
    );
    assert!(text.contains("Scene: Connolly Cottage · "), "{text}");
    assert!(!text.contains("Open request:"), "the request ended: {text}");
    assert!(
        text.contains("> Mícheál, how are the cattle this week?\n"),
        "the player's words: {text}"
    );
    assert!(
        text.contains("- rundale-intent.v1 "),
        "the intent call: {text}"
    );
    assert!(
        text.contains("failed (timed_out/offline)\n"),
        "the dialogue failure: {text}"
    );
    assert!(text.contains("error: the network went away\n"), "{text}");
    assert!(
        text.contains("asked: Mícheál Connolly at Connolly Cottage: "),
        "who was asked, and where: {text}"
    );
}

#[test]
fn a_bug_report_after_relaunch_keeps_the_transcript_but_no_calls() {
    let (mut game, _) = Game::new();
    game.go_to_the_cottage();
    let submitted = game.submit("Mícheál, how are the cattle this week?");
    let dialogue = game.until_dialogue(submitted);
    game.resolve(&dialogue, json!({"dialogue": CATTLE_LINE}));
    assert!(bug_report(&game, "").contains("rundale-dialogue.v1"));

    game.relaunch();
    let text = bug_report(&game, "");
    assert!(
        text.contains("Rundale bug report\n(no description)\n"),
        "{text}"
    );
    assert!(text.contains(CATTLE_LINE), "the journal survives: {text}");
    assert!(
        text.ends_with("\nEndpoint calls since launch: none\n"),
        "calls are kept in memory only: {text}"
    );
}
