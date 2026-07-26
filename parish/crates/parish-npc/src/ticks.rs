//! Tier 1, Tier 2, and Tier 3 tick functions for NPC simulation.
//!
//! Tier 1 ticks run per player interaction (full LLM inference).
//! Tier 2 ticks run every 5 game-minutes for nearby NPCs (lighter inference).
//! Tier 3 ticks run every 1 game-day for distant NPCs (batch inference).
//!
//! This file is the hub module. Implementation lives in the submodules below;
//! everything is re-exported so external `use parish_npc::ticks::*` paths
//! are unchanged.

mod gossip;
mod prompt;
mod tier1;
mod tier2;
mod tier3;
mod truncate;

// ── public re-exports ─────────────────────────────────────────────────────

// Relationship helpers (used by parish-core and callers building prompts)
pub use prompt::{
    Tier1ContextParams, build_enhanced_context_with_config,
    build_enhanced_system_prompt_with_config, format_relationships_natural,
    live_turn_contract_block, relationship_label, relationship_label_with_config,
};

// Tier 1 — response application + witness memory
pub use tier1::{apply_tier1_response_with_config, record_witness_memories};

// Tier 2 — snapshot types, prompt, inference, event application
pub use tier2::{
    GroundedTier2ApplyOutcome, NpcSnapshot, Tier2Group, apply_grounded_tier2_event_with_config,
    build_tier2_prompt, npc_snapshot_from_npc, npc_snapshot_from_npc_at, run_tier2_for_group,
    tier2_activity_fingerprint_from_npc_at, tier2_parse_failures_total,
    tier2_summary_location_conflict,
};

// Tier 3 — snapshot types, prompt, inference, update application
pub use tier3::{
    TIER3_BATCH_SIZE, Tier3Context, Tier3Snapshot, apply_tier3_updates, build_tier3_prompt,
    tick_tier3, tier3_snapshot_from_npc,
};

// Gossip helpers
pub use gossip::propagate_gossip_at_location;
