//! LLM prompt construction and greeting resolution for NPC arrival reactions.
//!
//! `build_reaction_prompt` assembles the system and user messages sent to the
//! inference client. `resolve_llm_greeting` fires the request with a short
//! timeout and falls back to the canned text on failure.

use std::time::Duration;

use crate::{LanguageSettings, Npc};
use parish_inference::{AnyClient, GenerateParams};
use parish_world::time::TimeOfDay;

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
            "You have not met this person before. You are working here as the {}. \
             Introduce yourself briefly.",
            npc.occupation
        )
    } else if !is_introduced {
        "You have not met this person before. You may introduce yourself or simply acknowledge them."
            .to_string()
    } else if at_workplace {
        format!(
            "You know this person. You are working here as the {}.",
            npc.occupation
        )
    } else {
        "You have met this person before.".to_string()
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
         Greet the way an 1820 villager would: \"Aye, {name_first}\", \
         \"Bedad,\", \"Faith,\", \"Begob,\", \"God save ye,\", or by simply \
         calling out their name. Cap the reply at ~20 words.",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = personality_snippet,
        mood = npc.mood,
        name_first = npc.name.split_whitespace().next().unwrap_or(&npc.name),
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
    let result = tokio::time::timeout(
        timeout,
        client.generate_stream(
            model,
            &context,
            Some(&system),
            sink_tx,
            GenerateParams {
                max_tokens: Some(100),
                temperature: None,
                frequency_penalty: None,
            },
        ),
    )
    .await;

    match result {
        Ok(Ok(text)) => {
            let trimmed = text.trim();
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
        _ => reaction.canned_text.clone(),
    }
}
