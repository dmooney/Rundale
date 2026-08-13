//! Per-location markdown log files.
//!
//! Each location in `WorldState::graph` gets a markdown file on disk under
//! `<user-data-dir>/<app>/logs/<branch>/` named `loc-NNN-slug.md`. The file
//! contains:
//!
//! * A **profile** section (vital stats, description, geography,
//!   connections, residents) bounded by HTML comment markers
//!   `<!-- PROFILE_START -->` / `<!-- PROFILE_END -->`. Rewritten on every
//!   session start by [`LocationLogManager::write_all_profiles`].
//! * A **journal** section, append-only, that grows as [`GameEvent`]s
//!   relevant to a location flow off the world's broadcast bus into
//!   [`LocationLogManager::process_event`].
//!
//! Mirrors the structure of [`crate::character_log`]; reuses its
//! `rewrite_profile_section` / `append_journal_entry` helpers.
//!
//! Gated by the `location-logs` feature flag, default on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use parish_npc::NpcId;
use parish_npc::manager::NpcManager;
use parish_persistence::paths::resolve_user_data_dir;
use parish_types::LocationId;
use parish_types::events::GameEvent;
use parish_world::WorldState;
use parish_world::graph::{GeoKind, Hazard, LocationData};

use crate::character_log::{
    append_journal_entry, append_journal_entry_batch, player_diary_label_for,
    rewrite_profile_section,
};

/// Feature-flag name controlling whether location logs are written.
pub const FEATURE_FLAG: &str = "location-logs";

/// Writes per-location markdown logs to disk.
///
/// One instance per session — the log directory is fixed at construction
/// time so a later cwd change cannot redirect file I/O (rule #9).
///
/// Stateless beyond the log directory: every `NpcArrived` / `NpcDeparted`
/// / `PlayerMoved` event on the bus describes a real physical movement
/// (published from `schedule::tick_schedules`, `ticks::apply_tier3_updates`,
/// and `game_session::apply_movement`), so the writer has nothing to
/// dedup.
#[derive(Clone, Debug)]
pub struct LocationLogManager {
    log_dir: PathBuf,
    enabled: bool,
}

impl LocationLogManager {
    /// Resolves the log directory for `app_name`/`branch_id` and creates it.
    pub fn new(app_name: &str, branch_id: i64, enabled: bool) -> Self {
        if !enabled {
            return Self::disabled();
        }
        let log_dir = resolve_user_data_dir(app_name)
            .join("logs")
            .join(format!("branch-{}", branch_id));
        Self::new_at_dir(log_dir, true)
    }

    /// Constructs a manager rooted at an explicit directory. Exposed for
    /// tests that need a `tempfile::tempdir` without env-var setup.
    pub fn new_at_dir(log_dir: PathBuf, enabled: bool) -> Self {
        if !enabled {
            return Self::disabled();
        }
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            tracing::warn!(
                path = %log_dir.display(),
                error = %e,
                "failed to create location-log directory",
            );
        }
        Self { log_dir, enabled }
    }

    fn disabled() -> Self {
        Self {
            log_dir: PathBuf::new(),
            enabled: false,
        }
    }

    /// Returns the directory all log files are written under.
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    /// Returns whether this manager is active.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Path of a location's log file: `loc-NNN-slug.md`.
    pub fn location_log_path(&self, loc: &LocationData) -> PathBuf {
        let slug = slugify(&loc.name);
        self.log_dir
            .join(format!("loc-{:03}-{}.md", loc.id.0, slug))
    }

    /// Collects the log-file path for every location in the graph, in stable
    /// id order, for a world-wide fan-out event (TD-031). Returning an owned
    /// `Vec` lets the caller hand `&Path` references to the batched append
    /// helper without borrowing `world` across the write loop.
    fn world_wide_paths(&self, world: &WorldState) -> Vec<PathBuf> {
        let mut ids = world.graph.location_ids();
        ids.sort_by_key(|id| id.0);
        ids.into_iter()
            .filter_map(|id| world.graph.get(id).map(|loc| self.location_log_path(loc)))
            .collect()
    }

    /// Rewrites the PROFILE section in every per-location log file.
    /// Journal sections of existing files are preserved verbatim.
    pub fn write_all_profiles(&self, world: &WorldState, npc_manager: &NpcManager) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let names: HashMap<NpcId, (String, String)> = npc_manager
            .all_npcs()
            .map(|n| (n.id, (n.name.clone(), n.occupation.clone())))
            .collect();

        let mut ids = world.graph.location_ids();
        ids.sort_by_key(|id| id.0);
        for id in ids {
            let Some(loc) = world.graph.get(id) else {
                continue;
            };
            let path = self.location_log_path(loc);
            let profile = format_location_profile(loc, world, &names);
            rewrite_profile_section(&path, &profile)
                .with_context(|| format!("rewrite location profile {}", path.display()))?;
        }
        Ok(())
    }

    /// Appends a journal entry to the appropriate location log file(s).
    pub fn process_event(
        &self,
        event: &GameEvent,
        world: &WorldState,
        npc_manager: &NpcManager,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let ts = event.timestamp();

        let path_for = |loc_id: LocationId| -> Option<PathBuf> {
            world
                .graph
                .get(loc_id)
                .map(|loc| self.location_log_path(loc))
        };
        let loc_name = |loc_id: LocationId| -> String {
            world
                .graph
                .get(loc_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("location {}", loc_id.0))
        };

        match event {
            GameEvent::ReactionRecorded { .. } => {}
            GameEvent::PlayerMoved { from, to, .. } => {
                let Some(path) = path_for(*to) else {
                    return Ok(());
                };
                let body = format!("*Arrived from {}*\n", loc_name(*from));
                append_journal_entry(&path, ts, Some("Player arrived"), &body)?;
            }
            GameEvent::NpcArrived {
                npc_id, location, ..
            } => {
                let Some(npc) = npc_manager.get(*npc_id) else {
                    return Ok(());
                };
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let suffix = format!("{} arrived", npc.name);
                append_journal_entry(&path, ts, Some(&suffix), "")?;
            }
            GameEvent::NpcDeparted {
                npc_id,
                location,
                to,
                ..
            } => {
                let Some(npc) = npc_manager.get(*npc_id) else {
                    return Ok(());
                };
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let suffix = format!("{} departed", npc.name);
                let body = format!("*Headed to {}*\n", loc_name(*to));
                append_journal_entry(&path, ts, Some(&suffix), &body)?;
            }
            GameEvent::NpcActivity {
                npc_id,
                location,
                activity,
                ..
            } => {
                if activity.trim().is_empty() {
                    return Ok(());
                }
                let Some(npc) = npc_manager.get(*npc_id) else {
                    return Ok(());
                };
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let suffix = format!("{} activity", npc.name);
                let body = format!("*{}*\n", activity);
                append_journal_entry(&path, ts, Some(&suffix), &body)?;
            }
            GameEvent::GossipSpread {
                source,
                location,
                content,
                ..
            } => {
                if content.trim().is_empty() {
                    return Ok(());
                }
                let Some(npc) = npc_manager.get(*source) else {
                    return Ok(());
                };
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let suffix = format!("Gossip from {}", npc.name);
                let body = format!("*{}*\n", content);
                append_journal_entry(&path, ts, Some(&suffix), &body)?;
            }
            GameEvent::DialogueOccurred {
                npc_id,
                location,
                summary,
                player_said,
                npc_said,
                ..
            } => {
                let Some(npc) = npc_manager.get(*npc_id) else {
                    return Ok(());
                };
                // Route by the event-time location, not the NPC's current
                // location — the NPC may have moved since publish (#1035).
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let player_line = player_said.as_deref().unwrap_or("").trim();
                let npc_line = npc_said
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| summary.trim());
                if player_line.is_empty() && npc_line.is_empty() {
                    return Ok(());
                }
                let player_label = player_diary_label_for(world, npc_manager, *npc_id);
                let mut body = String::new();
                if !player_line.is_empty() {
                    body.push_str(&format!("**{}:** {}\n", player_label, player_line));
                }
                if !npc_line.is_empty() {
                    body.push_str(&format!("**{}:** {}\n", npc.name, npc_line));
                }
                let heading = format!("Dialogue with {}", npc.name);
                append_journal_entry(&path, ts, Some(&heading), &body)?;
            }
            GameEvent::AddressedAbsentNpc { name, location, .. } => {
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let heading = format!("Missed introduction: {}", name);
                let body = format!("*The player called for {} — not present.*\n", name);
                append_journal_entry(&path, ts, Some(&heading), &body)?;
            }
            GameEvent::PlayerTaskAssigned { task, .. } => {
                let Some(path) = path_for(task.location) else {
                    return Ok(());
                };
                let assigner = npc_manager
                    .get(task.assigned_by)
                    .map(|npc| npc.name.as_str())
                    .unwrap_or("An unknown local");
                let heading = format!("Task assigned by {assigner}");
                let body = format!("*{}*\n", task.description);
                append_journal_entry(&path, ts, Some(&heading), &body)?;
            }
            GameEvent::PlayerTaskProgressed { task, action, .. } => {
                let Some(path) = path_for(task.location) else {
                    return Ok(());
                };
                let body = format!("*{}*\n\nAction: {}\n", task.description, action);
                append_journal_entry(&path, ts, Some("Player began assigned work"), &body)?;
            }
            GameEvent::WeatherChanged { new_weather, .. } => {
                // Weather is world-wide — every location experiences the
                // same shift. Log to every location's journal so the
                // parish-level state is uniformly visible (#1023 F4).
                //
                // TD-031: build the entry once and fan it out through the
                // batched helper instead of re-rendering the identical block
                // for each of the ~22 Rundale locations.
                let body = format!("*Weather: {}*\n", new_weather);
                let paths = self.world_wide_paths(world);
                let count = append_journal_entry_batch(
                    paths.iter().map(PathBuf::as_path),
                    ts,
                    Some("Weather"),
                    &body,
                );
                tracing::debug!(
                    locations = count,
                    "batched weather journal fan-out (TD-031)"
                );
            }
            GameEvent::FestivalStarted { name, .. } => {
                // Festivals are world-wide events — every location's
                // residents experience the calendar shift. Log to every
                // location for the same reason as WeatherChanged
                // (#1023 F4). Batched fan-out (TD-031).
                let body = format!("*Festival begins: {}*\n", name);
                let heading = format!("Festival: {}", name);
                let paths = self.world_wide_paths(world);
                let count = append_journal_entry_batch(
                    paths.iter().map(PathBuf::as_path),
                    ts,
                    Some(&heading),
                    &body,
                );
                tracing::debug!(
                    locations = count,
                    "batched festival journal fan-out (TD-031)"
                );
            }
            GameEvent::LifeEvent {
                npc_id,
                description,
                location,
                ..
            } => {
                let Some(npc) = npc_manager.get(*npc_id) else {
                    return Ok(());
                };
                // Route by the event-time location, not the NPC's current
                // location — the NPC may have moved since publish (#1077/#1079).
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let body = format!("*{} — {}*\n", npc.name, description);
                append_journal_entry(&path, ts, Some("Life event"), &body)?;
            }
            GameEvent::MoodChanged {
                npc_id,
                new_mood,
                location,
                ..
            } => {
                let Some(npc) = npc_manager.get(*npc_id) else {
                    return Ok(());
                };
                // Route by the event-time location, not the NPC's current
                // location — the NPC may have moved since publish (#1077/#1079).
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let body = format!("*{}: mood shifted to {}*\n", npc.name, new_mood);
                append_journal_entry(&path, ts, Some("Mood"), &body)?;
            }
            GameEvent::RelationshipChanged {
                npc_a,
                npc_b,
                delta,
                ..
            } => {
                let Some(a) = npc_manager.get(*npc_a) else {
                    return Ok(());
                };
                let Some(b) = npc_manager.get(*npc_b) else {
                    return Ok(());
                };
                // Log only when both NPCs are co-located — the relationship
                // shifted because something happened here. Cross-location
                // shifts (gossip, memory) belong in the character log only.
                if a.location() != b.location() {
                    return Ok(());
                }
                let Some(path) = path_for(a.location()) else {
                    return Ok(());
                };
                let body = format!("*{} ↔ {}: bond shifted by {:+.2}*\n", a.name, b.name, delta);
                append_journal_entry(&path, ts, Some("Relationship"), &body)?;
            }
            GameEvent::NpcInteraction {
                participants,
                location,
                summary,
                ..
            } => {
                let trimmed = summary.trim();
                if trimmed.is_empty() {
                    return Ok(());
                }
                let Some(path) = path_for(*location) else {
                    return Ok(());
                };
                let names: Vec<String> = participants
                    .iter()
                    .map(|p| {
                        npc_manager
                            .get(*p)
                            .map(|n| n.name.clone())
                            .unwrap_or_else(|| format!("NPC({})", p.0))
                    })
                    .collect();
                let suffix = format!("Interaction ({} present)", names.len());
                let body = format!("**{}:** {}\n", names.join(", "), trimmed);
                append_journal_entry(&path, ts, Some(&suffix), &body)?;
            }
        }
        Ok(())
    }
}

// ── Profile formatter ───────────────────────────────────────────────────────

/// Strips runtime render placeholders (e.g. `{time}`, `{weather}`) from a
/// location's description template for use in the stable profile pane.
///
/// The template is authored for runtime substitution by
/// `parish_world::description::render_description`; the profile is a fixed
/// "vital stats" pane, so current time / weather belong in journal entries,
/// not here (#1030). Placeholders embedded in a substantive sentence
/// (e.g. "The {weather} sky stretches over the midlands") are removed
/// in-place, while sentences that are *only* time/weather filler
/// (e.g. "It is {time}.") are dropped entirely so no dangling clause or
/// raw `{token}` survives.
fn strip_description_placeholders(template: &str) -> String {
    // Stopwords that don't count as descriptive content when deciding
    // whether a placeholder-stripped sentence still says anything.
    const STOPWORDS: &[&str] = &[
        "the", "and", "is", "are", "was", "were", "its", "his", "her", "outside", "over", "with",
        "of", "in", "on", "it",
    ];
    fn content_words(s: &str) -> usize {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 3 && !STOPWORDS.contains(&w.to_lowercase().as_str()))
            .count()
    }

    let mut kept: Vec<String> = Vec::new();
    for sentence in split_sentences(template) {
        if !sentence.contains('{') {
            kept.push(sentence.trim().to_string());
            continue;
        }
        let cleaned = clean_sentence(&remove_placeholder_tokens(&sentence));
        // Keep the sentence only if it still carries real description;
        // otherwise it was pure time/weather filler — drop it.
        if content_words(&cleaned) >= 3 {
            kept.push(cleaned);
        }
    }
    kept.join(" ").trim().to_string()
}

/// Removes `{token}` placeholders from a string. Any `{...}` run is dropped;
/// surrounding whitespace is left to [`clean_sentence`] to normalise.
fn remove_placeholder_tokens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            for n in chars.by_ref() {
                if n == '}' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Collapses repeated spaces and removes spaces left dangling before
/// sentence punctuation (e.g. "the  sky" → "the sky", "It is ." → "It is.").
fn clean_sentence(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == ' ' {
            if out.ends_with(' ') {
                continue;
            }
        } else if matches!(c, '.' | ',' | '!' | '?' | ';' | ':') && out.ends_with(' ') {
            out.pop();
        }
        out.push(c);
    }
    out.trim().to_string()
}

/// Splits text into sentences, keeping each terminating `.`/`!`/`?`.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '!' | '?') {
            // Sentence ends when the terminator is followed by whitespace or EOT.
            if chars.peek().map(|n| n.is_whitespace()).unwrap_or(true) {
                sentences.push(current.trim().to_string());
                current.clear();
            }
        }
    }
    if !current.trim().is_empty() {
        sentences.push(current.trim().to_string());
    }
    sentences
}

/// Renders the full PROFILE markdown for a location.
pub fn format_location_profile(
    loc: &LocationData,
    world: &WorldState,
    npcs_by_id: &HashMap<NpcId, (String, String)>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {} — Location Log\n", loc.name));
    out.push_str(&format!(
        "*{} · {}*\n\n",
        if loc.indoor { "Indoor" } else { "Outdoor" },
        if loc.public { "Public" } else { "Private" },
    ));

    out.push_str("## Description\n\n");
    out.push_str(&strip_description_placeholders(&loc.description_template));
    out.push_str("\n\n");

    out.push_str("## Geography\n\n");
    if loc.lat != 0.0 || loc.lon != 0.0 {
        out.push_str(&format!(
            "- Coordinates: {:.4}°{}, {:.4}°{}\n",
            loc.lat.abs(),
            if loc.lat >= 0.0 { "N" } else { "S" },
            loc.lon.abs(),
            if loc.lon >= 0.0 { "E" } else { "W" },
        ));
    }
    out.push_str(&format!("- Kind: {}\n", geo_kind_label(loc.geo_kind)));
    if !loc.aliases.is_empty() {
        out.push_str(&format!("- Also known as: {}\n", loc.aliases.join(", ")));
    }
    if let Some(source) = loc.geo_source.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("- Source: {}\n", source));
    }
    out.push('\n');

    if let Some(myth) = loc
        .mythological_significance
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        out.push_str("## Mythological Significance\n\n");
        out.push_str(&format!("*{}*\n\n", myth.trim()));
    }

    out.push_str("## Connections\n\n");
    if loc.connections.is_empty() {
        out.push_str("*(none)*\n\n");
    } else {
        for conn in &loc.connections {
            let target_name = world
                .graph
                .get(conn.target)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("location {}", conn.target.0));
            let hazard_tag = hazard_label(conn.hazard)
                .map(|h| format!(" *(⚠ {})*", h))
                .unwrap_or_default();
            out.push_str(&format!(
                "- **{}** — {}{}\n",
                target_name,
                conn.path_description.trim(),
                hazard_tag,
            ));
        }
        out.push('\n');
    }

    if !loc.associated_npcs.is_empty() {
        out.push_str("## Residents\n\n");
        let mut residents: Vec<NpcId> = loc.associated_npcs.clone();
        residents.sort_by_key(|id| id.0);
        for id in residents {
            let (name, occupation) = npcs_by_id
                .get(&id)
                .cloned()
                .unwrap_or_else(|| (format!("NPC({})", id.0), String::new()));
            if occupation.trim().is_empty() {
                out.push_str(&format!("- {}\n", name));
            } else {
                out.push_str(&format!("- {} ({})\n", name, occupation));
            }
        }
        out.push('\n');
    }

    out
}

fn geo_kind_label(k: GeoKind) -> &'static str {
    match k {
        GeoKind::Real => "Real",
        GeoKind::Manual => "Manual",
        GeoKind::Fictional => "Fictional",
    }
}

fn hazard_label(h: Hazard) -> Option<&'static str> {
    match h {
        Hazard::None => None,
        Hazard::Flood => Some("Flood"),
        Hazard::Lakeshore => Some("Lakeshore"),
        Hazard::Exposed => Some("Exposed"),
    }
}

/// Returns a lowercase ASCII slug of `s` suitable for a filename.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};

    #[test]
    fn strip_placeholders_removes_tokens_and_filler() {
        // Standalone time/weather sentences are dropped; embedded
        // placeholders are removed in place while keeping the sentence.
        let crossroads = "A quiet crossroads where four narrow roads meet. \
            A weathered stone wall lines the eastern side, half-hidden by brambles. \
            The {weather} sky stretches over the flat midlands. It is {time}.";
        let out = strip_description_placeholders(crossroads);
        assert!(!out.contains('{'), "raw token survived: {out}");
        assert!(!out.contains('}'), "raw token survived: {out}");
        assert!(out.contains("A quiet crossroads where four narrow roads meet."));
        // Embedded placeholder removed cleanly, sentence retained.
        assert!(
            out.contains("The sky stretches over the flat midlands."),
            "{out}"
        );
        // Pure time filler dropped.
        assert!(!out.contains("It is"), "time filler not dropped: {out}");

        // Placeholder embedded in the primary descriptive sentence must NOT
        // delete that sentence's content.
        let church = "A small stone church with a slate roof, its stained-glass \
            windows catching the {time} light. The sky is {weather}.";
        let out2 = strip_description_placeholders(church);
        assert!(!out2.contains('{'), "{out2}");
        assert!(
            out2.contains("stained-glass windows catching the light."),
            "{out2}"
        );
        assert!(
            !out2.contains("The sky is"),
            "weather filler not dropped: {out2}"
        );

        // A combined filler sentence collapses away entirely.
        let pub_desc = "The warm interior of Darcy's Pub. \
            It is {time} and the weather outside is {weather}.";
        let out3 = strip_description_placeholders(pub_desc);
        assert_eq!(out3, "The warm interior of Darcy's Pub.");
    }
    use parish_npc::Npc;
    use parish_npc::manager::NpcManager;
    use parish_npc::types::Intelligence;
    use parish_world::WorldState;
    use parish_world::graph::{Connection, LocationData, WorldGraph};
    use std::collections::HashMap;

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 3, 14, 30, 0).unwrap()
    }

    fn make_world_two_locations() -> WorldState {
        // Build a minimal two-node bidirectional graph via JSON so we
        // exercise the same load path the real world uses.
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "The Crossroads",
                    "description_template": "A windy fork in the road.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.7234,
                    "lon": -8.1234,
                    "associated_npcs": [],
                    "mythological_significance": "An ancient fairy fort.",
                    "aliases": [],
                    "geo_kind": "real",
                    "connections": [
                        {"target": 2, "path_description": "a worn path", "hazard": "flood"}
                    ]
                },
                {
                    "id": 2,
                    "name": "Darcy's Pub",
                    "description_template": "Warm hearth, low ceiling.",
                    "indoor": true,
                    "public": true,
                    "lat": 53.7300,
                    "lon": -8.1100,
                    "associated_npcs": [7],
                    "mythological_significance": null,
                    "aliases": [],
                    "geo_kind": "fictional",
                    "connections": [
                        {"target": 1, "path_description": "a worn path", "hazard": "flood"}
                    ]
                }
            ]
        }"#;
        let mut world = WorldState::new();
        world.graph = WorldGraph::load_from_str(json).unwrap();
        world.player_location = LocationId(1);
        world
    }

    fn make_npc(id: u32, name: &str, loc: LocationId) -> Npc {
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(id);
        npc.name = name.to_string();
        npc.brief_description = format!("a person called {}", name);
        npc.age = 40;
        npc.occupation = "Publican".to_string();
        npc.personality = "Warm-hearted".to_string();
        npc.pronouns = "they/them".to_string();
        npc.intelligence = Intelligence::new(3, 3, 3, 3, 3, 3);
        npc.set_location(loc);
        npc.mood = "content".to_string();
        npc
    }

    #[test]
    fn slugify_basic_cases() {
        assert_eq!(slugify("The Crossroads"), "the-crossroads");
        assert_eq!(slugify("Darcy's Pub"), "darcy-s-pub");
        assert_eq!(slugify("---weird---"), "weird");
        assert_eq!(slugify(""), "unnamed");
    }

    #[test]
    fn profile_section_contains_name_and_connections() {
        let world = make_world_two_locations();
        let loc = world.graph.get(LocationId(1)).unwrap();
        let names = HashMap::new();
        let profile = format_location_profile(loc, &world, &names);
        assert!(profile.contains("The Crossroads"), "{}", profile);
        assert!(profile.contains("Outdoor · Public"), "{}", profile);
        assert!(profile.contains("## Connections"), "{}", profile);
        assert!(profile.contains("Darcy's Pub"), "{}", profile);
        assert!(profile.contains("⚠ Flood"), "{}", profile);
        assert!(profile.contains("Mythological Significance"), "{}", profile);
    }

    #[test]
    fn profile_section_lists_residents() {
        let world = make_world_two_locations();
        let loc = world.graph.get(LocationId(2)).unwrap();
        let mut names = HashMap::new();
        names.insert(
            NpcId(7),
            ("Padraig Darcy".to_string(), "Publican".to_string()),
        );
        let profile = format_location_profile(loc, &world, &names);
        assert!(profile.contains("## Residents"), "{}", profile);
        assert!(profile.contains("Padraig Darcy (Publican)"), "{}", profile);
    }

    #[test]
    fn profile_rewrite_preserves_journal() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let npcs = NpcManager::new();
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::PlayerMoved {
            from: LocationId(2),
            to: LocationId(1),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        // Rewriting profiles must not destroy the journal entry above.
        mgr.write_all_profiles(&world, &npcs).unwrap();
        let path = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("Player arrived"), "{}", contents);
        assert!(contents.contains("The Crossroads"), "{}", contents);
    }

    #[test]
    fn player_moved_event_appends_to_destination_log() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let npcs = NpcManager::new();
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let dest = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let contents = std::fs::read_to_string(&dest).unwrap();
        assert!(contents.contains("Player arrived"), "{}", contents);
        assert!(
            contents.contains("Arrived from The Crossroads"),
            "{}",
            contents,
        );
    }

    #[test]
    fn npc_arrived_event_appends_to_location_log() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(2)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::NpcArrived {
            npc_id: NpcId(7),
            location: LocationId(2),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let path = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("Padraig Darcy arrived"), "{}", contents,);
    }

    #[test]
    fn dialogue_event_writes_to_npcs_current_location() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(2)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::DialogueOccurred {
            npc_id: NpcId(7),
            location: LocationId(2),
            summary: "discussed weather".into(),
            player_said: Some("Good afternoon, Padraig.".into()),
            npc_said: Some("Ah, God bless ye.".into()),
            request_id: None,
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let path = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("Dialogue with Padraig Darcy"),
            "dialogue heading should name the partner: {}",
            contents,
        );
        assert!(
            contents.contains("Good afternoon, Padraig."),
            "{}",
            contents,
        );
        assert!(contents.contains("Ah, God bless ye."), "{}", contents);
        // Should NOT appear in the other location's log.
        let other = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let other_contents = std::fs::read_to_string(&other).unwrap();
        assert!(
            !other_contents.contains("Good afternoon, Padraig."),
            "dialogue leaked to wrong location: {}",
            other_contents,
        );
    }

    /// #1035: the subscriber must route a DialogueOccurred to the event's
    /// own `location`, not the NPC's current location. The async bus can
    /// deliver the event after a schedule tick has moved the NPC.
    #[test]
    fn dialogue_routes_to_event_location_after_npc_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        // NPC has already moved to location 1 by the time the event is
        // consumed, but the dialogue happened at location 2.
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(1)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::DialogueOccurred {
            npc_id: NpcId(7),
            location: LocationId(2),
            summary: "discussed weather".into(),
            player_said: Some("Good afternoon, Padraig.".into()),
            npc_said: Some("Ah, God bless ye.".into()),
            request_id: None,
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        // Written to the event-time location (2), not the NPC's current one (1).
        let at_event_loc = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let event_contents = std::fs::read_to_string(&at_event_loc).unwrap();
        assert!(
            event_contents.contains("Good afternoon, Padraig."),
            "dialogue should be at the event location: {}",
            event_contents,
        );
        let at_current_loc = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let current_contents = std::fs::read_to_string(&at_current_loc).unwrap();
        assert!(
            !current_contents.contains("Good afternoon, Padraig."),
            "dialogue must NOT follow the NPC to its current location: {}",
            current_contents,
        );
    }

    #[test]
    fn mood_routes_to_event_location_after_npc_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        // NPC is now at location 1, but the mood shift happened at location 2.
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(1)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::MoodChanged {
            npc_id: NpcId(7),
            new_mood: "merry".into(),
            location: LocationId(2),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let at_event_loc = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let event_contents = std::fs::read_to_string(&at_event_loc).unwrap();
        assert!(
            event_contents.contains("mood shifted to merry"),
            "mood entry should be at the event location: {}",
            event_contents,
        );
        let at_current_loc = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let current_contents = std::fs::read_to_string(&at_current_loc).unwrap();
        assert!(
            !current_contents.contains("mood shifted to merry"),
            "mood entry must NOT follow the NPC to its current location: {}",
            current_contents,
        );
    }

    #[test]
    fn life_event_routes_to_event_location_after_npc_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        // NPC is now at location 1, but the life event happened at location 2.
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(1)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::LifeEvent {
            npc_id: NpcId(7),
            description: "fell gravely ill".into(),
            location: LocationId(2),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let at_event_loc = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let event_contents = std::fs::read_to_string(&at_event_loc).unwrap();
        assert!(
            event_contents.contains("fell gravely ill"),
            "life event should be at the event location: {}",
            event_contents,
        );
        let at_current_loc = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let current_contents = std::fs::read_to_string(&at_current_loc).unwrap();
        assert!(
            !current_contents.contains("fell gravely ill"),
            "life event must NOT follow the NPC to its current location: {}",
            current_contents,
        );
    }

    #[test]
    fn writer_appends_every_event_it_receives() {
        // Stateless contract: the writer no longer filters. Whoever
        // publishes `NpcArrived` to the bus is responsible for not
        // firing spurious copies. Two distinct events → two entries.
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(2)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let arrive = GameEvent::NpcArrived {
            npc_id: NpcId(7),
            location: LocationId(2),
            timestamp: ts(),
        };
        let depart = GameEvent::NpcDeparted {
            npc_id: NpcId(7),
            location: LocationId(2),
            to: LocationId(3),
            timestamp: Utc.with_ymd_and_hms(1820, 3, 3, 14, 35, 0).unwrap(),
        };
        let re_arrive = GameEvent::NpcArrived {
            npc_id: NpcId(7),
            location: LocationId(2),
            timestamp: Utc.with_ymd_and_hms(1820, 3, 3, 15, 0, 0).unwrap(),
        };
        mgr.process_event(&arrive, &world, &npcs).unwrap();
        mgr.process_event(&depart, &world, &npcs).unwrap();
        mgr.process_event(&re_arrive, &world, &npcs).unwrap();

        let path = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents.matches("Padraig Darcy arrived").count(), 2);
        assert_eq!(contents.matches("Padraig Darcy departed").count(), 1);
    }

    #[test]
    fn npc_activity_event_writes_authored_activity_to_location_log() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(1)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::NpcActivity {
            npc_id: NpcId(7),
            location: LocationId(2),
            activity: "tending bar".to_string(),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let path = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("Padraig Darcy activity"),
            "location log missing activity heading: {}",
            contents,
        );
        assert!(
            contents.contains("*tending bar*"),
            "location log missing activity body: {}",
            contents,
        );
    }

    #[test]
    fn gossip_spread_event_writes_to_location_log() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let mut npcs = NpcManager::new();
        npcs.add_npc(make_npc(7, "Padraig Darcy", LocationId(1)));
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::GossipSpread {
            source: NpcId(7),
            location: LocationId(2),
            content: "the landlord raised the rent again".to_string(),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let path = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("Gossip from Padraig Darcy"),
            "location log missing gossip heading: {}",
            contents,
        );
        assert!(
            contents.contains("*the landlord raised the rent again*"),
            "location log missing gossip body: {}",
            contents,
        );
    }

    #[test]
    fn addressed_absent_npc_event_writes_to_location_log() {
        // F9 / #1135: location-log captures the moment the player
        // called for someone who wasn't there.
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let npcs = NpcManager::new();
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::AddressedAbsentNpc {
            name: "Mrs. Hannigan".to_string(),
            location: LocationId(2),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let path = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("Missed introduction: Mrs. Hannigan"),
            "missing heading: {}",
            contents,
        );
        // Other location's log must NOT pick up the entry — the event
        // belongs to the location the player was in.
        let other = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let other_contents = std::fs::read_to_string(&other).unwrap();
        assert!(
            !other_contents.contains("Mrs. Hannigan"),
            "missed-introduction leaked to wrong location: {}",
            other_contents,
        );
    }

    #[test]
    fn weather_changed_event_logs_to_every_location() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let npcs = NpcManager::new();
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::WeatherChanged {
            new_weather: "Storm".to_string(),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        // Both locations must have the same Weather entry.
        let path1 = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let path2 = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let c1 = std::fs::read_to_string(&path1).unwrap();
        let c2 = std::fs::read_to_string(&path2).unwrap();
        assert!(
            c1.contains("Weather") && c1.contains("Storm"),
            "loc 1 missing Weather entry: {}",
            c1,
        );
        assert!(
            c2.contains("Weather") && c2.contains("Storm"),
            "loc 2 missing Weather entry: {}",
            c2,
        );
    }

    #[test]
    fn festival_started_event_logs_to_every_location() {
        let tmp = tempfile::tempdir().unwrap();
        let world = make_world_two_locations();
        let npcs = NpcManager::new();
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::FestivalStarted {
            name: "Bealtaine".to_string(),
            timestamp: ts(),
            location: None,
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let path1 = mgr.location_log_path(world.graph.get(LocationId(1)).unwrap());
        let path2 = mgr.location_log_path(world.graph.get(LocationId(2)).unwrap());
        let c1 = std::fs::read_to_string(&path1).unwrap();
        let c2 = std::fs::read_to_string(&path2).unwrap();
        assert!(
            c1.contains("Festival") && c1.contains("Bealtaine"),
            "loc 1 missing Festival entry: {}",
            c1,
        );
        assert!(
            c2.contains("Festival") && c2.contains("Bealtaine"),
            "loc 2 missing Festival entry: {}",
            c2,
        );
    }

    #[test]
    fn disabled_manager_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = LocationLogManager::new_at_dir(tmp.path().to_path_buf(), false);
        let world = make_world_two_locations();
        let npcs = NpcManager::new();
        mgr.write_all_profiles(&world, &npcs).unwrap();
        let event = GameEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            timestamp: ts(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        // log_dir is empty when disabled; nothing should have been written.
        assert!(mgr.log_dir().as_os_str().is_empty() || !mgr.log_dir().exists());
    }

    #[test]
    fn unused_connection_import_is_live() {
        // Keep `Connection` import live for non-trivial cross-module use.
        let _ = std::mem::size_of::<Connection>();
        let _ = std::mem::size_of::<LocationData>();
    }
}
