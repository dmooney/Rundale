//! LLM prompt construction and greeting resolution for NPC arrival reactions.
//!
//! `build_reaction_prompt` assembles the system and user messages sent to the
//! inference client. `resolve_llm_greeting` fires the request with a short
//! timeout and falls back to the canned text on failure.

use std::time::Duration;

use crate::{LanguageSettings, Npc};
use parish_inference::{AnyClient, GenerateParams};
use parish_world::time::TimeOfDay;

use super::register::has_calculating_register;
use super::types::NpcReaction;

// ── Prompt construction ──────────────────────────────────────────────────────

/// Builds a short system prompt for an LLM-generated arrival greeting.
pub fn build_reaction_prompt(
    npc: &Npc,
    location_name: &str,
    time_of_day: TimeOfDay,
    weather: &str,
    is_introduced: bool,
    at_workplace: bool,
    language: &LanguageSettings,
) -> (String, String) {
    use crate::language_directive;

    let time_str = match time_of_day {
        TimeOfDay::Dawn => "dawn",
        TimeOfDay::Morning => "morning",
        TimeOfDay::Midday => "midday",
        TimeOfDay::Afternoon => "afternoon",
        TimeOfDay::Dusk => "dusk",
        TimeOfDay::Night => "night",
        TimeOfDay::Midnight => "late at night",
    };

    let personality_snippet: String = npc.personality.chars().take(200).collect();

    let intro_context = if !is_introduced && at_workplace {
        format!(
            "A stranger has just walked in. You are working here as the {}. \
             Greet them — address the newcomer directly. You may give your name \
             as part of the welcome, but speak TO them, not about yourself.",
            npc.occupation
        )
    } else if !is_introduced {
        "A stranger has just arrived. Greet them directly — address the newcomer, \
         welcome them, or acknowledge their presence. Speak TO the newcomer. \
         You may give your name if it feels natural, but the greeting must be \
         directed outward at the person who just arrived."
            .to_string()
    } else if at_workplace {
        format!(
            "You know this person. You are working here as the {}.",
            npc.occupation
        )
    } else {
        "You have met this person before.".to_string()
    };

    let register_guidance = if has_calculating_register(npc) {
        " Register guidance: your calculating mood must override any cheerful \
         or warmly generic opener. Let the first line feel measured, appraising, \
         and business-minded; weigh the newcomer before welcoming them."
    } else {
        ""
    };

    let mut system = format!(
        "You are {name}, a {age}-year-old {occupation} in rural Ireland, 1820.\n\
         {personality}\n\
         Current mood: {mood}\n\n\
         Write a single brief greeting or reaction (1-2 sentences max). \
         Dialogue only, no narration or action descriptions. \
         Do not use any modern language.\n\n\
         FORBIDDEN phrases — never say any of these, they break the 1820 \
         rural-Irish voice and read as an AI assistant: \
         \"How may I assist\", \"How can I help\", \"Is there anything I can \
         do for you\", \"How may I be of service\", \"What brings you here \
         today\". \
         Address the newcomer directly — speak TO them. \
         Greet the way an 1820 villager would welcome a stranger: \
         \"God save ye,\", \"Bedad,\", \"Faith,\", \"Begob,\", \
         \"You're welcome,\", \"And who might you be?\". \
         Cap the reply at ~20 words.{register_guidance}",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = personality_snippet,
        mood = npc.mood,
        register_guidance = register_guidance,
    );

    system.push_str("\n\n");
    system.push_str(&language_directive(language));

    let context = format!(
        "A newcomer has just arrived at {location}. It is {time}, {weather}.\n{intro}",
        location = location_name,
        time = time_str,
        weather = weather,
        intro = intro_context,
    );

    (system, context)
}

// ── LLM greeting resolution ──────────────────────────────────────────────────

/// Scene context for [`resolve_llm_greeting`].
///
/// Bundles the location, time, weather, and introduction/workplace flags
/// so the async function signature stays below clippy's argument threshold.
pub struct LlmGreetingParams<'a> {
    /// Name of the location the player entered.
    pub location_name: &'a str,
    /// Current time of day.
    pub time_of_day: TimeOfDay,
    /// Current weather description.
    pub weather: &'a str,
    /// Whether the player has been introduced to this NPC.
    pub is_introduced: bool,
    /// Whether the NPC is currently at their workplace.
    pub at_workplace: bool,
    /// LLM inference client.
    pub client: &'a AnyClient,
    /// Model identifier to pass to the client.
    pub model: &'a str,
    /// Timeout in seconds for the LLM call.
    pub timeout_secs: u64,
    /// Fully resolved runtime profile for arrival reactions.
    pub profile: parish_config::InferenceProfile,
    /// Common direct-call audit destination used by every runtime.
    pub audit_sink: Option<parish_inference::InferenceAuditSink>,
}

/// Attempts an LLM-generated greeting with a short timeout.
///
/// Returns the LLM text if it responds in time, or the canned fallback
/// text from the reaction if the call times out or errors.
pub async fn resolve_llm_greeting(
    reaction: &NpcReaction,
    npc: &Npc,
    params: &LlmGreetingParams<'_>,
) -> String {
    let lang = LanguageSettings::english_only();
    let (system, context) = build_reaction_prompt(
        npc,
        params.location_name,
        params.time_of_day,
        params.weather,
        params.is_introduced,
        params.at_workplace,
        &lang,
    );
    let client = params.client;
    let model = params.model;
    let timeout_secs = params.timeout_secs;

    // Streaming variant: discards chunks but uses the streaming code path
    // so the underlying provider streams tokens (TTFT visibility) and a
    // future preemption pathway can cancel mid-flight (#9). Reaction is a
    // short greeting (~14-20 tokens), so cancellation is rarely needed —
    // but keeping every NPC turn on the same code path avoids divergence.
    let timeout = Duration::from_secs(timeout_secs);
    let (sink_tx, mut sink_rx) =
        tokio::sync::mpsc::channel::<String>(parish_inference::TOKEN_CHANNEL_CAPACITY);
    tokio::spawn(async move { while sink_rx.recv().await.is_some() {} });
    let wire_params = GenerateParams {
        max_tokens: Some(params.profile.max_output_tokens),
        temperature: None,
        frequency_penalty: None,
        enable_thinking: None,
        reasoning_effort: None,
        thinking_level: Some(params.profile.thinking_level),
        service_tier: Some(params.profile.service_tier),
        reasoning_intent: (params.profile.configuration_epoch > 0)
            .then_some(params.profile.reasoning_intent),
        reasoning_dialect: params.profile.reasoning_dialect,
    };
    let audit = parish_inference::DirectInferenceAudit::new(
        params.audit_sink.clone(),
        model,
        &context,
        Some(&system),
        parish_config::InferenceSubrole::ArrivalReaction,
        true,
        wire_params.max_tokens,
        wire_params.thinking_level,
        wire_params.service_tier,
        wire_params.temperature,
        parish_inference::InferencePriority::Interactive,
    );
    let result = tokio::time::timeout(
        timeout,
        client.generate_stream_detailed_with_format(
            model,
            &context,
            Some(&system),
            sink_tx,
            None,
            wire_params,
        ),
    )
    .await;

    let detailed = match result {
        Ok(result) => result,
        Err(_) => {
            let mut metadata = client.fallback_metadata(model);
            metadata.terminal_status = Some("timeout".to_string());
            Err(parish_inference::ProviderCallError {
                message: format!("arrival reaction timed out after {timeout_secs}s"),
                partial_text: String::new(),
                metadata: Box::new(metadata),
            })
        }
    };
    let result = audit.record(detailed).await;

    match result {
        Ok(result) => {
            let trimmed = result.text.trim();
            if trimmed.is_empty() {
                reaction.canned_text.clone()
            } else {
                let cleaned = trimmed.split("---").next().unwrap_or(trimmed).trim();
                if cleaned.is_empty() {
                    reaction.canned_text.clone()
                } else {
                    cleaned.to_string()
                }
            }
        }
        Err(_) => reaction.canned_text.clone(),
    }
}
