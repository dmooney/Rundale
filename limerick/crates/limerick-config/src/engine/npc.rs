//! NPC memory, cognition, and relationship tuning (`[engine.npc]`).
//!
//! Holds [`NpcConfig`] plus its two nested config domains
//! ([`CognitiveTierConfig`], [`RelationshipLabelConfig`]). Arrival-reaction
//! tuning lives in the sibling [`super::reactions`] module.

use serde::Deserialize;

use super::reactions::ReactionConfig;

/// NPC memory, cognition, and relationship tuning.
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NpcConfig {
    /// Maximum number of entries in NPC short-term memory.
    #[serde(default = "default_memory_capacity")]
    pub memory_capacity: usize,
    /// Buffer size for detecting `---` separator in streamed NPC responses.
    #[serde(default = "default_separator_holdback")]
    pub separator_holdback: usize,
    /// Number of recent memories included in dialogue context.
    #[serde(default = "default_memory_context_count")]
    pub memory_context_count: usize,
    /// Max characters for dialogue memory entries.
    #[serde(default = "default_memory_truncation_dialogue")]
    pub memory_truncation_dialogue: usize,
    /// Max characters for event log memory entries.
    #[serde(default = "default_memory_truncation_event_log")]
    pub memory_truncation_event_log: usize,
    /// Max characters for event summary in simulation.
    #[serde(default = "default_event_summary_truncation")]
    pub event_summary_truncation: usize,
    /// Max characters for event summary in debug display.
    #[serde(default = "default_event_summary_debug_truncation")]
    pub event_summary_debug_truncation: usize,
    /// Cognitive tier distance thresholds.
    #[serde(default)]
    pub cognitive_tiers: CognitiveTierConfig,
    /// Relationship strength label thresholds.
    #[serde(default)]
    pub relationship_labels: RelationshipLabelConfig,
    /// Number of recent player reactions included in dialogue context.
    #[serde(default = "default_reaction_context_count")]
    pub reaction_context_count: usize,
    /// NPC arrival reaction tuning.
    #[serde(default)]
    pub reactions: ReactionConfig,
    /// Hard cap on displayed NPC dialogue length in characters.
    ///
    /// Applied as a post-generation guard after inference completes. The
    /// system prompt already asks for "2-4 sentences" but small models
    /// occasionally ignore the instruction; this cap is the defensive
    /// backstop that prevents runaway responses reaching the player (#1224).
    /// Set to 0 to disable (not recommended in production).
    #[serde(default = "default_dialogue_display_max_chars")]
    pub dialogue_display_max_chars: usize,
    /// Word-level Jaccard similarity (0.0–1.0) at or above which a fresh NPC
    /// line is treated as a near-identical repeat of that NPC's own previous
    /// line and replaced with a varied fallback (#1228).
    ///
    /// Applied as a post-generation guard after inference completes, in the
    /// shared dialogue path. Small / quantized models occasionally echo their
    /// previous line or loop a single clause; this deterministic backstop keeps
    /// the degenerate output from reaching the player regardless of provider.
    /// Intra-line clause collapse always runs; this threshold only governs the
    /// cross-turn check. Set to 0.0 to disable the cross-turn check (not
    /// recommended in production); 1.0 requires an identical word set.
    #[serde(default = "default_dialogue_repetition_threshold")]
    pub dialogue_repetition_threshold: f32,
    /// Enable dialogue quality and continuity improvements (#1387, #1388).
    ///
    /// When `true` (the default), the Tier 1 prompt assembler:
    /// - injects the NPC's own recent dialogue lines as a "do not repeat"
    ///   list (anti-verbatim-recycling, #1387);
    /// - adds a "do not re-ask already-answered questions" continuity directive
    ///   to the conversation history block (#1388);
    /// - uses a familiarity-aware interlocutor address that drops "stranger"
    ///   after sufficient prior exchanges (#1388).
    ///
    /// Set to `false` to kill-switch back to the pre-fix behaviour. Controlled
    /// at runtime by the `dialogue-quality-continuity` feature flag
    /// (`flags.is_disabled("dialogue-quality-continuity")` → false).
    #[serde(default = "default_dialogue_quality_continuity")]
    pub dialogue_quality_continuity: bool,
    /// Inject real place-name grounding into the system prompt (#1394).
    ///
    /// When `true` (the default), `prepare_npc_conversation_turn` builds a
    /// sorted list of every location name from the world graph and passes it to
    /// the system-prompt assembler, which adds an anti-sycophancy instruction
    /// forbidding the NPC from confirming nonexistent places or people. Set to
    /// `false` to disable. Controlled at runtime by the
    /// `npc-dialogue-grounding` feature flag
    /// (`flags.is_disabled("npc-dialogue-grounding")` → false).
    #[serde(default = "default_grounding_enabled")]
    pub grounding_enabled: bool,
    /// Trim the display-length cap back to a sentence boundary (#1400).
    ///
    /// When `true` (the default), the post-generation display cap
    /// (`dialogue_display_max_chars`) rewinds to the last sentence terminator
    /// (`.`, `!`, `?`, `…`, or a closing quote following one) within the budget
    /// before appending `…`, so a clipped reply never ends mid-word or
    /// mid-clause ("...out and about, and…"). When no boundary exists in the
    /// budget it falls back to the legacy raw char-boundary clip. Set to
    /// `false` in `[npc]` config (`dialogue_sentence_boundary_trim = false`) to
    /// kill-switch back to the raw clip. This shares the post-generation
    /// display-cap seam (`apply_npc_dialogue_turn`), which reads `NpcConfig`
    /// defaults, so the toggle lives on the config rather than the runtime
    /// feature-flag layer (matching `dialogue_display_max_chars`).
    #[serde(default = "default_dialogue_sentence_boundary_trim")]
    pub dialogue_sentence_boundary_trim: bool,
    /// Enable cross-NPC opener de-duplication within a single multi-NPC turn (#1422).
    ///
    /// When `true` (the default), the shared orchestration layer strips the
    /// duplicated stock opener sentence from a co-located NPC's reply when it
    /// near-exactly matches an opener already used by an earlier NPC in the same
    /// turn. This prevents the "Ye've come to the right place …" tic from
    /// appearing across three different NPCs in one run. Deterministic and
    /// provider-agnostic. Controlled at runtime by the `dialogue-anti-repetition`
    /// feature flag (`flags.is_disabled("dialogue-anti-repetition")` → false).
    #[serde(default = "default_dialogue_anti_repetition")]
    pub dialogue_anti_repetition: bool,
    /// Enable the post-generation fabricated-person confirmation guard (#1459).
    ///
    /// When `true` (the default), a post-generation scan checks the finalized
    /// dialogue for affirmative confirmation of a named person from the player
    /// input who is NOT in the NPC's known-roster. If the guard fires it
    /// replaces the entire dialogue with a stock non-recognition decline. The
    /// 14B model ignores the PEOPLE-YOU-KNOW prompt directive for presupposed
    /// names; this is the deterministic backstop. Controlled at runtime by the
    /// `dialogue-person-confirmation-guard` feature flag (default-on).
    #[serde(default = "default_person_confirmation_guard_enabled")]
    pub person_confirmation_guard_enabled: bool,
    /// Enable the post-generation verbosity / run-on guard (#1460).
    ///
    /// When `true` (the default), the finalized dialogue passes through three
    /// structural fixes: (a) strip bare leaked mood-adjective, (b) trim
    /// mid-sentence truncation ellipsis to the last complete sentence, (c) cap
    /// trailing question stack to at most one question. These target degenerate
    /// Qwen2.5-14B outputs where the model emits 5-6 identical questions,
    /// truncates mid-sentence with "…", or leaks the literal mood word.
    /// Controlled at runtime by the `dialogue-verbosity-guard` feature flag
    /// (default-on).
    #[serde(default = "default_verbosity_guard_enabled")]
    pub verbosity_guard_enabled: bool,
}

impl Default for NpcConfig {
    fn default() -> Self {
        Self {
            memory_capacity: default_memory_capacity(),
            separator_holdback: default_separator_holdback(),
            memory_context_count: default_memory_context_count(),
            memory_truncation_dialogue: default_memory_truncation_dialogue(),
            memory_truncation_event_log: default_memory_truncation_event_log(),
            event_summary_truncation: default_event_summary_truncation(),
            event_summary_debug_truncation: default_event_summary_debug_truncation(),
            cognitive_tiers: CognitiveTierConfig::default(),
            relationship_labels: RelationshipLabelConfig::default(),
            reaction_context_count: default_reaction_context_count(),
            reactions: ReactionConfig::default(),
            dialogue_display_max_chars: default_dialogue_display_max_chars(),
            dialogue_repetition_threshold: default_dialogue_repetition_threshold(),
            dialogue_quality_continuity: default_dialogue_quality_continuity(),
            grounding_enabled: default_grounding_enabled(),
            dialogue_sentence_boundary_trim: default_dialogue_sentence_boundary_trim(),
            dialogue_anti_repetition: default_dialogue_anti_repetition(),
            person_confirmation_guard_enabled: default_person_confirmation_guard_enabled(),
            verbosity_guard_enabled: default_verbosity_guard_enabled(),
        }
    }
}

fn default_reaction_context_count() -> usize {
    5
}

fn default_memory_capacity() -> usize {
    20
}
fn default_separator_holdback() -> usize {
    24
}
fn default_memory_context_count() -> usize {
    5
}
fn default_memory_truncation_dialogue() -> usize {
    // TODO #7: 250 trimmed in-spec replies mid-sentence in the
    // recent-events buffer fed to subsequent NPC turns. After the
    // round-22 repetition_penalty fix, Tier 1 replies stabilise
    // around 400–600 chars; 600 keeps them intact so downstream
    // NPCs read the full prior dialogue, and only degenerate-loop
    // outliers clip.
    600
}
fn default_memory_truncation_event_log() -> usize {
    // TODO #7: 150 was too tight given the dialogue cap raise.
    // 300 leaves headroom for the outer
    // `"{speaker} said: '{input}'. Responded: {dialogue}"` wrapper
    // around the now-larger dialogue body without double-clipping.
    300
}
fn default_event_summary_truncation() -> usize {
    100
}
fn default_event_summary_debug_truncation() -> usize {
    50
}
fn default_dialogue_display_max_chars() -> usize {
    // 800 chars is roughly 5-6 sentences at typical spoken cadence.
    // The system prompt requests "2-4 sentences"; this cap is the
    // post-generation backstop that clips runaway model output before
    // it reaches the player UI (#1224). Replies under ~600 chars
    // pass through unchanged in normal operation.
    800
}
fn default_dialogue_quality_continuity() -> bool {
    true
}

fn default_grounding_enabled() -> bool {
    true
}

fn default_dialogue_sentence_boundary_trim() -> bool {
    true
}

fn default_person_confirmation_guard_enabled() -> bool {
    true
}

fn default_verbosity_guard_enabled() -> bool {
    true
}

fn default_dialogue_repetition_threshold() -> f32 {
    // 0.92 word-level Jaccard: two lines must share ~92% of their word set to
    // count as a near-identical repeat. Exact normalized equality always
    // triggers regardless of this value. 0.92 catches the #1228 case (an NPC
    // echoing its prior line near-verbatim) while leaving normal turn-to-turn
    // variation — which reuses common function words but introduces new content
    // words — comfortably below the bar. Enabled by default.
    0.92
}

fn default_dialogue_anti_repetition() -> bool {
    true
}

/// Cognitive tier assignment based on distance from player.
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CognitiveTierConfig {
    /// Maximum distance for Tier 1 (same location).
    #[serde(default = "default_tier1_max_distance")]
    pub tier1_max_distance: u32,
    /// Maximum distance for Tier 2 (nearby).
    #[serde(default = "default_tier2_max_distance")]
    pub tier2_max_distance: u32,
    /// Maximum distance for Tier 3 (distant but still LLM-simulated).
    #[serde(default = "default_tier3_max_distance")]
    pub tier3_max_distance: u32,
    /// Tier 2 simulation tick interval in game-minutes.
    #[serde(default = "default_tier2_tick_interval_minutes")]
    pub tier2_tick_interval_minutes: i64,
    /// Tier 3 simulation tick interval in game-hours (1 game-day = 24).
    #[serde(default = "default_tier3_tick_interval_hours")]
    pub tier3_tick_interval_hours: i64,
    /// Maximum NPCs per Tier 3 batch LLM call.
    #[serde(default = "default_tier3_batch_size")]
    pub tier3_batch_size: usize,
    /// Tier 4 rules-engine tick interval in game-days (1 season ≈ 90 days).
    #[serde(default = "default_tier4_tick_interval_days")]
    pub tier4_tick_interval_days: i64,
}

impl Default for CognitiveTierConfig {
    fn default() -> Self {
        Self {
            tier1_max_distance: default_tier1_max_distance(),
            tier2_max_distance: default_tier2_max_distance(),
            tier3_max_distance: default_tier3_max_distance(),
            tier2_tick_interval_minutes: default_tier2_tick_interval_minutes(),
            tier3_tick_interval_hours: default_tier3_tick_interval_hours(),
            tier3_batch_size: default_tier3_batch_size(),
            tier4_tick_interval_days: default_tier4_tick_interval_days(),
        }
    }
}

fn default_tier1_max_distance() -> u32 {
    0
}
fn default_tier2_max_distance() -> u32 {
    2
}
fn default_tier2_tick_interval_minutes() -> i64 {
    5
}
fn default_tier3_max_distance() -> u32 {
    5
}
fn default_tier3_tick_interval_hours() -> i64 {
    24
}
fn default_tier3_batch_size() -> usize {
    10
}
fn default_tier4_tick_interval_days() -> i64 {
    90
}

/// Relationship strength thresholds for descriptive labels.
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipLabelConfig {
    /// Threshold for "very close".
    #[serde(default = "default_very_close")]
    pub very_close: f64,
    /// Threshold for "friendly".
    #[serde(default = "default_friendly")]
    pub friendly: f64,
    /// Threshold for "acquainted".
    #[serde(default = "default_acquainted")]
    pub acquainted: f64,
    /// Threshold for "cool".
    #[serde(default = "default_cool")]
    pub cool: f64,
    /// Threshold for "strained".
    #[serde(default = "default_strained")]
    pub strained: f64,
}

impl Default for RelationshipLabelConfig {
    fn default() -> Self {
        Self {
            very_close: default_very_close(),
            friendly: default_friendly(),
            acquainted: default_acquainted(),
            cool: default_cool(),
            strained: default_strained(),
        }
    }
}

fn default_very_close() -> f64 {
    0.7
}
fn default_friendly() -> f64 {
    0.3
}
fn default_acquainted() -> f64 {
    0.0
}
fn default_cool() -> f64 {
    -0.3
}
fn default_strained() -> f64 {
    -0.7
}
