//! Demo / auto-player commands — context snapshot, LLM-driven turns, and prompt helpers.

use std::sync::Arc;

use crate::AppState;
use parish_core::inference::{AnyClient, GenerateParams};

// Demo payload types and the prompt-builder live in `parish-core::ipc::demo`
// so the builder can be constrained to GUI-facing inputs only (issue #998).
pub use parish_core::ipc::demo::{
    DemoAdjacentLocation, DemoContextSnapshot, DemoNpcInfo, build_demo_context,
};

/// Demo configuration returned by `get_demo_config`.
#[derive(serde::Serialize, Clone)]
pub struct DemoConfigPayload {
    pub auto_start: bool,
    pub extra_prompt: Option<String>,
    pub turn_pause_secs: f32,
    pub max_turns: Option<u32>,
}

/// Returns the demo configuration (CLI flags parsed at startup).
#[tauri::command]
pub async fn get_demo_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DemoConfigPayload, String> {
    let dc = &state.demo_config;
    Ok(DemoConfigPayload {
        auto_start: dc.auto_start,
        extra_prompt: dc.extra_prompt.clone(),
        turn_pause_secs: dc.turn_pause_secs,
        max_turns: dc.max_turns,
    })
}

/// Builds a context snapshot for the LLM demo player.
///
/// Returns location, time, weather, NPCs present, and adjacent locations.
/// The `recent_log` field is empty; the frontend fills it from the text log
/// store before passing the snapshot to `get_llm_player_action`.
#[tauri::command]
pub async fn get_demo_context(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DemoContextSnapshot, String> {
    {
        let config = state.config.lock().await;
        if !config.flags.is_enabled("demo-mode") {
            return Err("Demo mode is not active.".to_string());
        }
    }

    // Lock order: world → npc_manager (matches AppState contract).
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;

    // Issue #998: build the demo snapshot from the same GUI-facing IPC
    // payloads the frontend already consumes. Anything the GUI hides
    // (occupation pre-intro, fog-of-war neighbours) is automatically
    // hidden from the LLM prompt.
    let world_snapshot = parish_core::ipc::handlers::snapshot_from_world(&world);
    let npcs = parish_core::ipc::handlers::build_npcs_here(&world, &npc_manager);
    let map =
        parish_core::ipc::handlers::build_map_data(&world, state.transport.default_mode(), false);

    use chrono::{Datelike, Timelike};
    let now = world.clock.now();
    // Regression (fixed: #5): include HH:MM alongside the time-of-day word so the
    // demo auto-player can see clock progression between turns and
    // pick an appropriate greeting register.
    let game_time = format!(
        "{}, {} {} {}, {:02}:{:02} ({})",
        now.format("%A"),
        now.day(),
        now.format("%B"),
        now.year(),
        now.hour(),
        now.minute(),
        world.clock.time_of_day(),
    );
    let season = format!("{}", world.clock.season());
    let extra_prompt = state.demo_config.extra_prompt.clone();

    Ok(build_demo_context(
        &world_snapshot,
        &npcs,
        &map,
        game_time,
        season,
        extra_prompt,
    ))
}

/// Detects if a string is command-form intent description (bare verb-first
/// pattern like "ask about X", "tell Name Y", "whisper to X") rather than
/// first-person speech or direct command. This guard prevents the demo
/// auto-player from leaking the LLM's internal intent reasoning as player chat.
///
/// Command-form intent leaks are specifically dialogue-related intent
/// descriptions that the LLM might output during reasoning:
/// - "ask about X", "ask the Y", "ask a Z"
/// - "tell Name something"
/// - "whisper to Name"
///
/// Movement/exploration commands like "go to X" and "look" are valid player inputs
/// and NOT considered intent leaks.
///
/// Returns `true` if the text matches a dialogue intent leak pattern.
fn is_command_form_intent_leak(text: &str) -> bool {
    let trimmed = text.trim();

    // Bare direct commands that are valid (single word, no object) are NOT intent leaks.
    // These are legitimate player inputs for the game engine.
    let valid_direct_commands = ["look", "wait", "go", "listen", "think"];
    if valid_direct_commands.contains(&trimmed) {
        return false;
    }

    // Check for dialogue-form intent leaks: bare verbs used for dialogue.
    // These are patterns where the LLM outputs its internal reasoning about
    // what dialogue to attempt, rather than the dialogue itself.
    let dialogue_intent_patterns = [
        // Dialogue-related intent leaks
        "ask about ",
        "ask the ",
        "ask a ",
        "ask if ",
        "ask ", // bare "ask" followed by something (but not bare "ask" alone)
        "tell ",
        "whisper ",
        "whisper to ",
    ];

    let lower = trimmed.to_lowercase();
    for pattern in &dialogue_intent_patterns {
        if lower.starts_with(pattern) {
            return true;
        }
    }

    false
}

/// Extracts the player action from an LLM response.
///
/// Handles four patterns:
/// 1. Completion: model received `{"action": "` and completed it — response is
///    something like `go to the mill"}`. Extract up to the closing quote.
/// 2. Full JSON: model output `{"action": "go to the mill"}` — scan for `{`
///    and JSON-parse from there.
/// 3. Envelope-leak: model emitted only the JSON suffix — e.g.
///    `action": "hello"}` or `hello"}` — strip the wrapper bits.
/// 4. Fallback: no JSON at all — strip thinking preamble, take last line.
fn extract_action_from_response(text: &str) -> String {
    // Strip thinking blocks first so all patterns operate on clean text.
    let stripped = strip_thinking_block(text);
    let trimmed = stripped.trim();

    // Pattern 1: fill-in-the-blank completion — response starts with the
    // action text and ends with `"}` or just `"`. The model completed
    // `{"action": "` → `go to the mill"}`.
    // We also handle `go to the mill"` (no closing brace).
    let completion = trimmed
        .trim_end_matches('}')
        .trim_end()
        .trim_end_matches('"')
        .trim();
    // Valid completion: no opening brace in the extracted text (it's pure action).
    if !completion.is_empty() && !completion.starts_with('{') && !completion.contains("action") {
        // Check that the raw response looked like a completion (no full JSON object).
        if !trimmed.contains("{\"action\"") && !trimmed.contains("{ \"action\"") {
            // Guard against command-form intent leaks (e.g., "ask about ...").
            if !is_command_form_intent_leak(completion) {
                return completion.to_string();
            }
        }
    }

    // Pattern 2: full JSON object anywhere in the response.
    let mut search = trimmed;
    while let Some(start) = search.find('{') {
        let candidate = &search[start..];
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(candidate)
            && let Some(action) = val.get("action").and_then(|v| v.as_str())
        {
            let action = action.trim();
            if !action.is_empty() {
                // Guard against command-form intent leaks.
                if !is_command_form_intent_leak(action) {
                    return action.to_string();
                }
            }
        }
        search = &search[start + 1..];
    }

    // Pattern 3: envelope-leak — model emitted only the JSON suffix (no
    // matching `{...}` object) such as:
    //   action": "Good morning..."}
    //   "action": "Good morning..."}
    //   Good morning..."}
    // Strip a leading `[{][\s]*["]?action["]\s*:\s*"` prefix and a trailing
    // `"\s*}` (or bare `"}`) suffix, then return the inner text.
    if let Some(cleaned) = strip_envelope_leak(trimmed)
        && !cleaned.is_empty()
    {
        // Guard against command-form intent leaks.
        if !is_command_form_intent_leak(&cleaned) {
            return cleaned;
        }
    }

    // Pattern 4: fallback — take last meaningful line from already-stripped text.
    // Skip if the text looks like JSON (Pattern 2 already tried to extract from it).
    if trimmed.contains("{\"") || trimmed.contains("{ \"") || trimmed.starts_with('{') {
        // All JSON patterns either returned a leak or found nothing.
        return String::new();
    }
    let fallback = trimmed.trim_matches('"').trim_matches('\'').to_string();
    // Guard against command-form intent leaks before returning fallback.
    if !is_command_form_intent_leak(&fallback) {
        fallback
    } else {
        // If all patterns extracted a command-form intent leak, return empty
        // to indicate the LLM failed to produce valid player input.
        String::new()
    }
}

/// Strips a leaked JSON envelope from `text`. Returns `Some(inner)` when at
/// least one envelope marker (a leading `action":` prefix or a trailing `"}`
/// suffix) was found and removed. Returns `None` if neither side looks like
/// a leak, so the caller can apply its own fallback.
fn strip_envelope_leak(text: &str) -> Option<String> {
    let mut s = text.trim();
    let mut changed = false;

    // Leading wrapper: optional `{`, optional whitespace, optional `"`,
    // literal `action`, optional `"`, whitespace, `:`, whitespace, `"`.
    // We hand-roll this instead of pulling a regex dep.
    let original = s;
    let mut rest = s;
    rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix('{') {
        rest = stripped.trim_start();
    }
    if let Some(stripped) = rest.strip_prefix('"') {
        rest = stripped;
    }
    if let Some(stripped) = rest.strip_prefix("action") {
        let mut after = stripped;
        if let Some(stripped) = after.strip_prefix('"') {
            after = stripped;
        }
        after = after.trim_start();
        if let Some(stripped) = after.strip_prefix(':') {
            let mut after = stripped.trim_start();
            if let Some(stripped) = after.strip_prefix('"') {
                after = stripped;
                s = after;
                changed = true;
            }
        }
    }
    if !changed {
        s = original;
    }

    // Trailing wrapper: optional `}`, whitespace, `"` (in reverse order
    // since we work from the end).
    let original_tail = s;
    let mut tail = s.trim_end();
    if let Some(stripped) = tail.strip_suffix('}') {
        tail = stripped.trim_end();
    }
    if let Some(stripped) = tail.strip_suffix('"') {
        tail = stripped;
        s = tail;
        changed = true;
    } else {
        s = original_tail;
    }

    if changed {
        Some(s.trim().to_string())
    } else {
        None
    }
}

/// Strips reasoning preamble from LLM responses so only the action remains.
///
/// Handles two patterns:
/// 1. Tagged blocks: `<thinking>...</thinking>` / `<think>...</think>` from
///    reasoning models (deepseek-r1, qwq). Takes everything after the last
///    closing tag.
/// 2. Plain-text multi-paragraph reasoning: if the response has blank-line-
///    separated paragraphs, takes the last paragraph. This covers models that
///    output reasoning prose before the final action without tags.
///
/// Falls back to the full trimmed text if neither pattern applies.
fn strip_thinking_block(text: &str) -> &str {
    let trimmed = text.trim();

    // Strip tagged thinking blocks first.
    for close_tag in &["</thinking>", "</think>"] {
        if let Some(pos) = trimmed.rfind(close_tag) {
            let after = trimmed[pos + close_tag.len()..].trim();
            if !after.is_empty() {
                return after;
            }
        }
    }

    // If the response has multiple blank-line-separated paragraphs, take the
    // last one — reasoning models often output rationale before the action.
    if let Some(last_para) = trimmed.rsplit("\n\n").find(|p| !p.trim().is_empty()) {
        let candidate = last_para.trim();
        // Only use the last paragraph if it looks like a short action (≤ 3
        // lines), not if the whole thing is one paragraph of prose.
        let line_count = candidate.lines().count();
        if line_count <= 3 && candidate.len() < trimmed.len() {
            return candidate;
        }
    }

    // Last-line fallback: models sometimes separate reasoning from action with
    // a single newline. If the last non-empty line is much shorter than the
    // full response, treat it as the action.
    let lines: Vec<&str> = trimmed.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > 1
        && let Some(&last) = lines.last()
    {
        let last = last.trim();
        if last.len() <= 200 && last.len() < trimmed.len() {
            return last;
        }
    }

    trimmed
}

/// Truncate `s` to at most `max_chars` characters, suffixing `...` when
/// truncation occurs. Used to keep tracing previews bounded for the
/// `raw_preview` field on the empty-action retry path (fixed: #18).
fn truncate_for_log(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

/// Builds the demo-turn system prompt for the LLM-as-player.
///
/// Extracted from `get_llm_player_action` so the role anchor (fixed: #51)
/// is unit-testable without driving the full Tauri command flow. The
/// optional `extra_prompt` is appended verbatim after the "Explore
/// naturally" paragraph — usually loaded from
/// `mods/rundale/demo-prompt.txt`.
fn build_demo_system_prompt(extra_prompt: Option<&str>) -> String {
    let extra_section = extra_prompt
        .map(|p| format!("\n\n{}", p))
        .unwrap_or_default();

    format!(
        "You are playing Rundale, an Irish living-world simulation set in 1820. You are a \
wandering stranger named Aiden Carney exploring the townlands of east Roscommon. The world \
is populated by historical Irish villagers — farmers, priests, weavers, matchmakers — each \
living their own life.\n\
\n\
ROLE: You are ALWAYS Aiden Carney. Speak ONLY in Aiden's voice — never as a priest, miller, \
shopkeeper, schoolmaster, or any other local NPC. If the previous turn in the prompt ends \
with your own line and no NPC reply, that means the NPC's reply is still in flight; you \
still speak as Aiden on the next turn — do NOT take the NPC's side of the exchange. Do not \
answer your own questions on the NPC's behalf, and do not roleplay an answer from a \
villager.\n\
\n\
Date: 1820. Catholic Emancipation: 1829 (not yet). Famine: 1845 (not yet).\n\
\n\
Speak as a 1820 traveller would: plain, short, period-appropriate. Avoid modern words \
like: fascinating, amazing, definitely, totally, decided to visit, taking in the sights, \
healing properties.\n\
\n\
Explore naturally: talk to people, learn their stories, travel between locations, and \
respond to whatever you encounter. Act as a curious outsider would.{extra}\n\
\n\
Respond with a JSON object containing a single field \"action\" — the text the player \
would type into the game. Do NOT use meta-commands like \"talk to X\"; write the actual \
words or command directly.\n\
\n\
NO NARRATION: The engine has no narrative parser. Do NOT describe what \
you are doing in past tense, third person, or participial style. Inputs like \
\"Walking up to the cabin, I knock gently on the door\", \"Sittin' here, I \
notice a book half-open on the table\", or \"I'll take a seat on the bench\" \
vanish into the dialogue path and produce no game-state change. Legal action \
shapes are: (a) spoken dialogue in first-person present tense (\"Good \
mornin'. Have ye news from the road?\"), (b) movement commands (\"go to The \
Mill\"), or (c) a bare command verb the engine recognises (\"look\"). If you \
want to do something physical, say what you would say aloud — never narrate \
the action.\n\
\n\
Do NOT repeat yourself: if your last action appears in the \"Your last actions\" or \
\"Recent events\" block of the user prompt, pick a different action — try a different \
greeting, ask a different question, or travel somewhere new. The location description \
is already shown to you in the prompt; you do not need to issue a bare \"look\" command.\n\
\n\
Do NOT mirror NPC catchphrases. The \"Recent events\" block carries NPC replies in \
their own voice — they may use stock tags like \"Just askin', mind ye\", \"so it is\", \
\"sure\", or \"mayhap\" as their personal vocal habits. Aiden has his own voice: use \
plain Hiberno-English without adopting another character's verbal tics. If an NPC \
ends every line with \"so it is\", do not start ending yours with it too.\n\
\n\
MOVEMENT CADENCE: A traveller does not loiter. After 3–5 turns \
at one location, move to a new place — pick a name from the \"You can go to: \
...\" line in the user prompt and emit a movement command on its own (no \
spoken line wrapped around it). Bare \"go to X\" / \"walk to X\" / \"head to X\" \
is the correct shape: the engine parses these as movement, not dialogue. If \
you have visited only one location in the last 5 turns, your next action \
should be a movement command.\n\
\n\
WHEN ALONE: If the user prompt's status block contains the line \
\"NPCs here: none\", there is nobody to hear you. Do NOT speak, ask questions, \
roleplay knocking on doors, or wait around. Your ONLY useful action at an \
empty location is to move. Pick a destination from the \"You can go to: ...\" \
line and emit a bare movement command (\"go to X\" / \"walk to X\" / \"head \
to X\"). Speaking at an empty location burns a turn and accomplishes nothing.\n\
\n\
Examples:\n\
  {{\"action\": \"Good mornin' to ye. A fair day for the road.\"}}\n\
  {{\"action\": \"I've come from up the road. What news do ye have hereabouts?\"}}\n\
  {{\"action\": \"Might I ask about the harvest, then?\"}}\n\
  {{\"action\": \"go to The Mill\"}}\n\
  {{\"action\": \"walk to St. Brigid's Church\"}}\n\
  {{\"action\": \"head to Connolly's Shop\"}}\n\
\n\
Your entire response must be a single JSON object — nothing before or after it.",
        extra = extra_section,
    )
}

/// Asks the LLM to choose the next player action given the current game context.
///
/// The frontend fills `ctx.recent_log` from the text log store before calling
/// this command. Returns the trimmed action string.
#[tauri::command]
pub async fn get_llm_player_action(
    ctx: DemoContextSnapshot,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    {
        let config = state.config.lock().await;
        if !config.flags.is_enabled("demo-mode") {
            return Err("Demo mode is not active.".to_string());
        }
    }

    // Resolve client and model (base client; no per-category override for demo).
    let (client_opt, model) = {
        let config = state.config.lock().await;
        let client_guard = state.client.lock().await;
        let model = config.model_name.clone();
        let client = client_guard.as_ref().cloned();
        (client, model)
    };

    let Some(client) = client_opt else {
        return Err("No LLM client configured.".to_string());
    };

    let system_prompt = build_demo_system_prompt(ctx.extra_prompt.as_deref());

    // Issue #998: render via the shared `parish_core::ipc::demo` helper so the
    // prompt format stays in lockstep with the typed snapshot (no
    // `Name (Title)` parens that the LLM can misread as a vocative).
    let user_prompt = parish_core::ipc::demo::render_user_prompt(&ctx);

    // Surface the constructed prompt so demo-mode logs prove what the LLM
    // actually saw — required by the issue-998 verification flow.
    tracing::info!(
        location = %ctx.location_name,
        user_prompt = %user_prompt,
        "demo turn: prompt built"
    );

    let raw =
        generate_player_action_paused(&client, &model, &user_prompt, &system_prompt, 0.9, &state)
            .await?;

    // Primary: extract the "action" field from JSON output.
    // The system prompt asks for {"action": "..."}, which is robust against
    // any amount of preamble or reasoning text the model emits before it.
    let mut action_text = extract_action_from_response(&raw);
    tracing::info!(
        location = %ctx.location_name,
        raw_len = raw.len(),
        action = %action_text,
        "demo turn: LLM chose action"
    );

    // Regression (fixed: #18) — bounded single retry on empty action. Cycle 3 of the
    // demo audit logged two consecutive turns where the LLM returned
    // 137/139 chars but the parser surfaced an empty action; the
    // player input was recorded as nothing and no NPC turn fired.
    // Common cause: model emitted {"action": ""} or completion lacking
    // the `action` key. Retry once at temperature 1.0 with the same
    // prompt — most retries succeed on the bump. Bounded to one extra
    // call so a wedged model can't pin the slot.
    if action_text.is_empty() && !raw.trim().is_empty() {
        tracing::warn!(
            location = %ctx.location_name,
            raw_len = raw.len(),
            raw_preview = %truncate_for_log(&raw, 200),
            "demo turn: parsed action empty despite non-empty completion; retrying once"
        );
        // Freeze the clock across the retry inference too (#1207 #32).
        let retry_raw = generate_player_action_paused(
            &client,
            &model,
            &user_prompt,
            &system_prompt,
            1.0,
            &state,
        )
        .await?;
        let retry_action = extract_action_from_response(&retry_raw);
        if !retry_action.is_empty() {
            tracing::info!(
                location = %ctx.location_name,
                raw_len = retry_raw.len(),
                action = %retry_action,
                "demo turn: retry produced non-empty action"
            );
            action_text = retry_action;
        } else {
            tracing::warn!(
                location = %ctx.location_name,
                raw_len = retry_raw.len(),
                raw_preview = %truncate_for_log(&retry_raw, 200),
                "demo turn: retry also produced empty action; skipping turn"
            );
        }
    }

    // Quality sensors — emit WARN on any structural issue in the parsed
    // player action. These don't gate execution; they surface bugs in the
    // demo log so the judging pass can pick them up.
    for issue in parish_core::npc::quality::detect_all_text_issues(&action_text) {
        tracing::warn!(
            site = "demo-player-action",
            kind = issue.kind.as_str(),
            detail = %issue.detail,
            "quality issue in LLM player action"
        );
    }

    Ok(action_text)
}

/// Runs one auto-player generation with the world clock frozen for the
/// duration of the inference call.
///
/// #1207 #32: freeze the world clock while the auto-player "thinks", exactly as
/// NPC turns do (`clock.inference_pause()`). The 36x demo speed-factor
/// otherwise advances game-time during the multi-second player-decision
/// inference, so a standing conversation burned game-hours and movement time
/// looked wildly inconsistent. The clock is resumed before the error is
/// propagated so an inference failure can't leave it stuck paused. Extracted
/// from `get_llm_player_action` (#1200 TD-012) — it deduplicates the primary
/// and retry call sites, which previously inlined identical pause/generate/
/// resume blocks.
async fn generate_player_action_paused(
    client: &AnyClient,
    model: &str,
    user_prompt: &str,
    system_prompt: &str,
    temperature: f32,
    state: &Arc<AppState>,
) -> Result<String, String> {
    state.world.lock().await.clock.inference_pause();
    let profile = state
        .config
        .lock()
        .await
        .inference_profile(parish_core::config::InferenceSubrole::DemoPlayer);
    let audit_sink = state
        .inference_queue
        .lock()
        .await
        .as_ref()
        .and_then(parish_core::inference::InferenceQueue::audit_sink);
    let params = GenerateParams {
        max_tokens: Some(profile.max_output_tokens),
        temperature: Some(temperature),
        frequency_penalty: None,
        enable_thinking: None,
        reasoning_effort: None,
        thinking_level: Some(profile.thinking_level),
        service_tier: Some(profile.service_tier),
        reasoning_intent: (profile.configuration_epoch > 0).then_some(profile.reasoning_intent),
        reasoning_dialect: profile.reasoning_dialect,
    };
    let audit = parish_core::inference::DirectInferenceAudit::new(
        audit_sink,
        model,
        user_prompt,
        Some(system_prompt),
        parish_core::config::InferenceSubrole::DemoPlayer,
        false,
        params.max_tokens,
        params.thinking_level,
        params.service_tier,
        params.temperature,
        parish_core::inference::InferencePriority::Interactive,
    );
    let gen_result = audit
        .record(
            client
                .generate_detailed_with_format(
                    model,
                    user_prompt,
                    Some(system_prompt),
                    None,
                    params,
                )
                .await,
        )
        .await;
    state.world.lock().await.clock.inference_resume();
    gen_result
        .map(|result| result.text)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        build_demo_system_prompt, extract_action_from_response, strip_thinking_block,
        truncate_for_log,
    };

    /// Regression test (fixed: #18) — pin the failure shapes where the parser returns
    /// empty so the retry path's gate (`action_text.is_empty()`) is
    /// well-defined.
    #[test]
    fn extract_action_returns_empty_on_action_field_set_to_empty_string() {
        // {"action": ""} — model emitted the envelope but with an
        // empty action. Parser returns "" → retry fires.
        assert_eq!(extract_action_from_response(r#"{"action": ""}"#), "");
    }

    #[test]
    fn extract_action_returns_empty_on_bare_empty_input() {
        // Bare empty string — nothing to recover. Retry would still
        // fire on a non-empty raw completion in the actual loop.
        assert_eq!(extract_action_from_response(""), "");
    }

    #[test]
    fn truncate_for_log_short_string_passes_through() {
        assert_eq!(truncate_for_log("hello", 200), "hello");
    }

    #[test]
    fn truncate_for_log_long_string_is_clipped_with_ellipsis() {
        let long: String = "x".repeat(500);
        let truncated = truncate_for_log(&long, 200);
        assert_eq!(truncated.chars().filter(|&c| c == 'x').count(), 200);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn demo_system_prompt_names_aiden_carney() {
        // Regression (fixed: #51) — AC1: system prompt must explicitly name the
        // auto-player so the model has a role anchor stronger than
        // the generic "wandering stranger" phrasing.
        let prompt = build_demo_system_prompt(None);
        assert!(
            prompt.contains("Aiden Carney"),
            "system prompt missing player name anchor:\n{prompt}"
        );
    }

    #[test]
    fn demo_system_prompt_forbids_speaking_as_npc() {
        // Regression (fixed: #51) — AC2: prompt must direct the model to speak only
        // in Aiden's voice and not roleplay an NPC's reply, even when
        // the prior turn lacks an NPC line.
        let prompt = build_demo_system_prompt(None);
        assert!(
            prompt.contains("Speak ONLY in Aiden's voice"),
            "system prompt missing speak-only-as-Aiden directive:\n{prompt}"
        );
        assert!(
            prompt.contains("never as a priest, miller, shopkeeper"),
            "system prompt missing never-as-NPC list:\n{prompt}"
        );
        assert!(
            prompt.contains("NPC's reply is still in flight"),
            "system prompt missing in-flight-reply guidance:\n{prompt}"
        );
        assert!(
            prompt.contains("do NOT take the NPC's side"),
            "system prompt missing don't-flip-roles directive:\n{prompt}"
        );
    }

    #[test]
    fn demo_system_prompt_forbids_mirroring_npc_catchphrases() {
        // Regression (fixed: #26): the auto-player was adopting NPC stock tags
        // ("Just askin', mind ye") from the recent-events buffer.
        // Prompt must explicitly tell the model not to mirror NPC
        // verbal tics.
        let prompt = build_demo_system_prompt(None);
        assert!(
            prompt.contains("Do NOT mirror NPC catchphrases"),
            "system prompt missing catchphrase-mirror guard:\n{prompt}"
        );
        assert!(
            prompt.contains("Aiden has his own voice"),
            "system prompt missing own-voice anchor:\n{prompt}"
        );
        assert!(
            prompt.contains("Just askin', mind ye"),
            "system prompt should name the canonical example phrase:\n{prompt}"
        );
    }

    #[test]
    fn demo_system_prompt_carries_movement_cadence_directive() {
        // Regression (fixed: #1/#30): auto-player produced exactly 1 movement in 38+
        // turns because the prompt has no explicit cadence rule and
        // movement is 1 of 4 few-shot examples. Prompt must (a) name
        // movement as a first-class action, (b) carry a "move after
        // N turns" cadence rule, (c) show ≥ 2 movement few-shots
        // alongside the dialogue ones, and (d) cite all three canonical
        // movement verbs so the model picks across them.
        let prompt = build_demo_system_prompt(None);
        assert!(
            prompt.contains("MOVEMENT CADENCE"),
            "system prompt missing movement-cadence header:\n{prompt}"
        );
        assert!(
            prompt.contains("After 3–5 turns"),
            "system prompt missing 3-5 turn cadence rule:\n{prompt}"
        );
        assert!(
            prompt.contains("go to The Mill"),
            "system prompt missing 'go to' movement example:\n{prompt}"
        );
        assert!(
            prompt.contains("walk to St. Brigid's Church"),
            "system prompt missing 'walk to' movement example:\n{prompt}"
        );
        assert!(
            prompt.contains("head to Connolly's Shop"),
            "system prompt missing 'head to' movement example:\n{prompt}"
        );
        assert!(
            prompt.contains("the engine parses these as movement"),
            "system prompt must distinguish movement commands from \
             dialogue so the model emits bare 'go to X' rather than \
             wrapping it in a spoken line:\n{prompt}"
        );
    }

    #[test]
    fn demo_system_prompt_forbids_narrative_action_style() {
        // Regression (fixed: #47): cycle 9 caught 8/18 turns in narrative form
        // ("Walking up to the cabin, I knock gently on the door";
        // "Sittin' here, I notice a book half-open on the table";
        // "I'll take a seat on the bench") — all silently dropped by
        // the engine. The prompt must (a) name the failure mode, (b)
        // cite at least one concrete negative example so the model
        // pattern-matches, and (c) enumerate the legal action shapes.
        let prompt = build_demo_system_prompt(None);
        assert!(
            prompt.contains("NO NARRATION"),
            "system prompt missing no-narration header:\n{prompt}"
        );
        assert!(
            prompt.contains("Walking up to the cabin"),
            "system prompt missing participial-narration negative example:\n{prompt}"
        );
        assert!(
            prompt.contains("vanish into the dialogue path"),
            "system prompt must spell out the engine's failure mode so the \
             model has a reason to obey:\n{prompt}"
        );
        assert!(
            prompt.contains("Legal action shapes are"),
            "system prompt must enumerate the legal action shapes:\n{prompt}"
        );
    }

    #[test]
    fn demo_system_prompt_layers_extra_prompt() {
        // Regression (fixed: #51) — AC3: operator extra prompt must still appear.
        let prompt = build_demo_system_prompt(Some("RUNDALE-SPECIFIC: stay east of the river."));
        assert!(
            prompt.contains("RUNDALE-SPECIFIC: stay east of the river."),
            "extra prompt missing from layered system prompt:\n{prompt}"
        );
        // And the anchor still appears alongside the extra content.
        assert!(prompt.contains("Speak ONLY in Aiden's voice"));
    }

    #[test]
    fn extracts_action_from_json() {
        let input = r#"{"action": "Good morning! How are you today?"}"#;
        assert_eq!(
            extract_action_from_response(input),
            "Good morning! How are you today?"
        );
    }

    #[test]
    fn extracts_action_from_json_after_preamble() {
        let input = "Let me think... However, we are a wandering stranger.\n{\"action\": \"go to the crossroads\"}";
        assert_eq!(extract_action_from_response(input), "go to the crossroads");
    }

    #[test]
    fn extracts_action_from_json_after_thinking_tags() {
        let input = "<think>reasoning here</think>\n{\"action\": \"look\"}";
        assert_eq!(extract_action_from_response(input), "look");
    }

    #[test]
    fn falls_back_to_stripping_when_no_json() {
        let input = "Some reasoning.\nMight I ask about the harvest, then?";
        assert_eq!(
            extract_action_from_response(input),
            "Might I ask about the harvest, then?"
        );
    }

    #[test]
    fn strips_envelope_leak_with_action_prefix() {
        // Live demo bug: model emits only the JSON suffix, no opening `{`.
        let input = r#"action": "Good morning, Peig Hannigan. My name is [Your Name]. I'm just wandering."}"#;
        assert_eq!(
            extract_action_from_response(input),
            "Good morning, Peig Hannigan. My name is [Your Name]. I'm just wandering."
        );
    }

    #[test]
    fn strips_trailing_envelope_suffix() {
        // Live demo bug: model leaves only the trailing `"}` after a clean reply.
        let input = r#"I'm just curious about the community."}"#;
        assert_eq!(
            extract_action_from_response(input),
            "I'm just curious about the community."
        );
    }

    #[test]
    fn strips_thinking_block_before_action() {
        let input =
            "<thinking>\nI should greet the farmer.\n</thinking>\nHello there, good morning!";
        assert_eq!(strip_thinking_block(input), "Hello there, good morning!");
    }

    #[test]
    fn strips_think_tag_variant() {
        let input = "<think>reasoning</think>\ngo to the mill";
        assert_eq!(strip_thinking_block(input), "go to the mill");
    }

    #[test]
    fn no_thinking_block_returns_trimmed() {
        let input = "  ask Brigid about the harvest  ";
        assert_eq!(strip_thinking_block(input), "ask Brigid about the harvest");
    }

    #[test]
    fn only_thinking_block_falls_back_to_full() {
        let input = "<thinking>just thinking, nothing after</thinking>";
        assert_eq!(
            strip_thinking_block(input),
            "<thinking>just thinking, nothing after</thinking>"
        );
    }

    #[test]
    fn uses_last_closing_tag_for_nested() {
        let input = "<thinking>outer <think>inner</think> more</thinking>\nlook around";
        assert_eq!(strip_thinking_block(input), "look around");
    }

    #[test]
    fn strips_plain_text_reasoning_before_action() {
        let input = "Looking at the context, I see Peig is here. I should greet her warmly.\n\nHello Peig, good morning!";
        assert_eq!(strip_thinking_block(input), "Hello Peig, good morning!");
    }

    #[test]
    fn single_paragraph_returned_as_is() {
        let input = "Hello Seamus, how goes the harvest?";
        assert_eq!(strip_thinking_block(input), input);
    }

    #[test]
    fn strips_single_newline_reasoning_before_action() {
        let input = "I need to explore. The crossroads is nearby.\ngo to the crossroads";
        assert_eq!(strip_thinking_block(input), "go to the crossroads");
    }

    #[test]
    fn strips_multi_sentence_reasoning_single_newline() {
        let input = "Based on my previous interaction with Peig, I should explore. The mill is unvisited.\nask about the mill";
        assert_eq!(strip_thinking_block(input), "ask about the mill");
    }

    #[test]
    fn is_command_form_intent_leak_rejects_ask_patterns() {
        assert!(super::is_command_form_intent_leak(
            "ask about the places nearby that are worth visiting"
        ));
        assert!(super::is_command_form_intent_leak("ask about the harvest"));
        assert!(super::is_command_form_intent_leak("ask the priest"));
        assert!(super::is_command_form_intent_leak("ask a stranger"));
        assert!(super::is_command_form_intent_leak("ask if anyone knows"));
    }

    #[test]
    fn is_command_form_intent_leak_rejects_tell_patterns() {
        assert!(super::is_command_form_intent_leak("tell Brigid my name"));
        assert!(super::is_command_form_intent_leak(
            "tell the stranger something"
        ));
    }

    #[test]
    fn is_command_form_intent_leak_rejects_whisper_patterns() {
        assert!(super::is_command_form_intent_leak("whisper a secret"));
        assert!(super::is_command_form_intent_leak("whisper to Brigid"));
    }

    #[test]
    fn is_command_form_intent_leak_accepts_look_patterns() {
        // "look" variants are movement/exploration commands, not dialogue intent leaks.
        assert!(!super::is_command_form_intent_leak("look at the stranger"));
        assert!(!super::is_command_form_intent_leak("look for water"));
    }

    #[test]
    fn is_command_form_intent_leak_accepts_movement_commands() {
        // Movement commands are valid player inputs, NOT intent leaks.
        assert!(!super::is_command_form_intent_leak("go to the mill"));
        assert!(!super::is_command_form_intent_leak("go back home"));
        assert!(!super::is_command_form_intent_leak("go into the house"));
        assert!(!super::is_command_form_intent_leak(
            "go towards the village"
        ));
        assert!(!super::is_command_form_intent_leak("walk to the mill"));
        assert!(!super::is_command_form_intent_leak("travel to Dublin"));
        assert!(!super::is_command_form_intent_leak("move to the window"));
        assert!(!super::is_command_form_intent_leak("climb to the hill"));
    }

    #[test]
    fn is_command_form_intent_leak_accepts_bare_commands() {
        // Bare valid commands should NOT be flagged as intent leaks.
        assert!(!super::is_command_form_intent_leak("look"));
        assert!(!super::is_command_form_intent_leak("wait"));
        assert!(!super::is_command_form_intent_leak("go"));
        assert!(!super::is_command_form_intent_leak("listen"));
        assert!(!super::is_command_form_intent_leak("think"));
    }

    #[test]
    fn is_command_form_intent_leak_accepts_natural_speech() {
        // Natural first-person speech should pass through.
        assert!(!super::is_command_form_intent_leak(
            "Good mornin'. Might I look about the village a while?"
        ));
        assert!(!super::is_command_form_intent_leak(
            "I've come from up the road. What news do ye have hereabouts?"
        ));
        assert!(!super::is_command_form_intent_leak(
            "Might I ask about the harvest, then?"
        ));
        assert!(!super::is_command_form_intent_leak(
            "Hello there, good morning!"
        ));
    }

    #[test]
    fn extract_action_rejects_intent_leak_from_json() {
        let input = r#"{"action": "ask about the places nearby that are worth visiting"}"#;
        // Should return empty string since the intent leak is detected.
        assert_eq!(extract_action_from_response(input), "");
    }

    #[test]
    fn extract_action_rejects_dialogue_intent_leak_from_completion() {
        let input = r#"ask about the places nearby that are worth visiting"}"#;
        // Completion pattern that is a dialogue intent leak should return empty.
        assert_eq!(extract_action_from_response(input), "");
    }

    #[test]
    fn extract_action_accepts_natural_speech_in_json() {
        let input = r#"{"action": "Might I ask about the harvest, then?"}"#;
        assert_eq!(
            extract_action_from_response(input),
            "Might I ask about the harvest, then?"
        );
    }

    #[test]
    fn extract_action_accepts_bare_commands() {
        let input = r#"{"action": "look"}"#;
        assert_eq!(extract_action_from_response(input), "look");

        let input2 = r#"{"action": "go"}"#;
        assert_eq!(extract_action_from_response(input2), "go");
    }
}
