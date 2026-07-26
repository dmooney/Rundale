//! Per-character markdown log files (#TBD).
//!
//! Each NPC — and the player — gets a markdown file on disk under
//! `<user-data-dir>/<app>/logs/<branch>/` containing:
//!
//! * A **profile** section (vital stats, intelligence, backstory,
//!   relationships, schedule) bounded by HTML comment markers
//!   `<!-- PROFILE_START -->` / `<!-- PROFILE_END -->`. This section is
//!   rewritten on every session start by [`CharacterLogManager::write_all_profiles`].
//! * A **journal** section, append-only, that grows over time as
//!   [`GameEvent`]s flow off the world's broadcast bus into
//!   [`CharacterLogManager::process_event`].
//!
//! The split is deliberate: profile data is volatile (an NPC's mood,
//! relationship strength, or schedule can shift between sessions) and
//! is fully derived from current world state — so we rewrite it. The
//! journal is historical and must never be discarded.
//!
//! Gated by the `character-logs` feature flag, default on.
//!
//! ## Wiring
//!
//! Every entry point (`parish-tauri`, `parish-server`, `parish-engine`) is
//! responsible for:
//! 1. Constructing a [`CharacterLogManager`] at startup with the active
//!    mod's `app_name` and the loaded branch id.
//! 2. Calling [`CharacterLogManager::write_all_profiles`] once after
//!    world + NPC state is loaded.
//! 3. Pumping every [`GameEvent`] that flows off the bus through
//!    [`CharacterLogManager::process_event`]. Tauri and Server do this
//!    inside the same broadcast subscriber they already run for the
//!    debug panel; the CLI drains synchronously after each REPL turn.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Timelike, Utc};

use parish_npc::manager::NpcManager;
use parish_npc::types::RelationshipKind;
use parish_npc::{Npc, NpcId};
use parish_persistence::paths::resolve_user_data_dir;
use parish_types::events::GameEvent;
use parish_types::{DayType, LocationId, Season};
use parish_world::WorldState;

/// HTML-comment marker that opens the rewritten profile section.
pub const PROFILE_START: &str = "<!-- PROFILE_START -->";
/// HTML-comment marker that closes the rewritten profile section.
pub const PROFILE_END: &str = "<!-- PROFILE_END -->";

/// Feature-flag name controlling whether character logs are written.
///
/// Default on (`!flags.is_disabled("character-logs")`).
pub const FEATURE_FLAG: &str = "character-logs";

/// Writes per-character markdown logs to disk.
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
pub struct CharacterLogManager {
    log_dir: PathBuf,
    enabled: bool,
}

impl CharacterLogManager {
    /// Resolves the log directory for `app_name`/`branch_id` and creates it.
    ///
    /// `enabled` reflects the `character-logs` feature flag. When `false`,
    /// every method on the returned manager is a no-op — the directory is
    /// not even probed.
    pub fn new(app_name: &str, branch_id: i64, enabled: bool) -> Self {
        if !enabled {
            return Self {
                log_dir: PathBuf::new(),
                enabled: false,
            };
        }
        let log_dir = resolve_user_data_dir(app_name)
            .join("logs")
            .join(format!("branch-{}", branch_id));
        Self::new_at_dir(log_dir, true)
    }

    /// Constructs a manager rooted at an explicit directory, bypassing
    /// the user-data-dir resolution that [`Self::new`] performs.
    ///
    /// Exposed for unit tests so they can point at a `tempfile::tempdir`
    /// without setting the `PARISH_USER_DATA_DIR` env var (which races
    /// in parallel test runs — discovered when tarpaulin's parallel
    /// runner failed `dialogue_event_writes_player_and_npc_lines`).
    pub fn new_at_dir(log_dir: PathBuf, enabled: bool) -> Self {
        if !enabled {
            return Self {
                log_dir: PathBuf::new(),
                enabled: false,
            };
        }
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            tracing::warn!(
                path = %log_dir.display(),
                error = %e,
                "failed to create character-log directory",
            );
        }
        Self { log_dir, enabled }
    }

    /// Returns the directory all log files are written under.
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    /// Returns whether this manager is active. When `false`, `write_*` /
    /// `process_event` are no-ops.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Path of the player's log file.
    pub fn player_log_path(&self) -> PathBuf {
        self.log_dir.join("player.md")
    }

    /// Path of an NPC's log file: `npc-NNN-slug.md`.
    pub fn npc_log_path(&self, npc: &Npc) -> PathBuf {
        let slug = slugify(&npc.name);
        self.log_dir
            .join(format!("npc-{:03}-{}.md", npc.id.0, slug))
    }

    /// Rewrites the PROFILE section in every per-character log file.
    ///
    /// The JOURNAL section of any existing file is preserved verbatim.
    /// Files that do not yet exist are created with an empty journal.
    pub fn write_all_profiles(&self, world: &WorldState, npc_manager: &NpcManager) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let names: HashMap<NpcId, String> = npc_manager
            .all_npcs()
            .map(|n| (n.id, n.name.clone()))
            .collect();

        rewrite_profile_section(&self.player_log_path(), &format_player_profile(world))
            .context("rewrite player profile")?;

        for npc in npc_manager.all_npcs() {
            let path = self.npc_log_path(npc);
            rewrite_profile_section(&path, &format_npc_profile(npc, world, &names))
                .with_context(|| format!("rewrite npc profile {}", path.display()))?;
        }
        Ok(())
    }

    /// Appends a journal entry to the appropriate log file(s) for `event`.
    ///
    /// `world` + `npc_manager` are used only to resolve names — they are
    /// not mutated.
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
        let name_of = |id: NpcId| -> String {
            npc_manager
                .get(id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("NPC({})", id.0))
        };
        let loc_of = |id: LocationId| -> String {
            world
                .graph
                .get(id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("location {}", id.0))
        };

        match event {
            GameEvent::DialogueOccurred {
                npc_id,
                summary,
                player_said,
                npc_said,
                ..
            } => {
                let Some(npc) = npc_manager.get(*npc_id) else {
                    return Ok(());
                };
                let path = self.npc_log_path(npc);
                let player_line = player_said.as_deref().unwrap_or("").trim();
                // Fall back to `summary` when the event came from a non-live
                // source that didn't populate `npc_said` (e.g. legacy replay).
                let npc_line = npc_said
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| summary.trim());
                if player_line.is_empty() && npc_line.is_empty() {
                    return Ok(());
                }
                // Journal is the NPC's diary — write from their POV.
                // The player appears by their known name (or "a
                // stranger" when this NPC hasn't been introduced) and
                // the NPC refers to themselves in the first person.
                let player_label = player_diary_label_for(world, npc_manager, *npc_id);
                let mut body = String::new();
                if !player_line.is_empty() {
                    body.push_str(&format!("**{}:** {}\n", player_label, player_line));
                }
                if !npc_line.is_empty() {
                    body.push_str(&format!("**I:** {}\n", npc_line));
                }
                append_journal_entry(&path, ts, None, &body)?;
            }
            GameEvent::MoodChanged {
                npc_id, new_mood, ..
            } => {
                if let Some(npc) = npc_manager.get(*npc_id) {
                    let body = format!("*Mood shifted to {}*\n", new_mood);
                    append_journal_entry(&self.npc_log_path(npc), ts, Some("Mood"), &body)?;
                }
            }
            GameEvent::RelationshipChanged {
                npc_a,
                npc_b,
                delta,
                ..
            } => {
                let a_name = name_of(*npc_a);
                let b_name = name_of(*npc_b);
                if let Some(npc) = npc_manager.get(*npc_a) {
                    let body = format!("*Relationship with {} shifted by {:+.2}*\n", b_name, delta);
                    append_journal_entry(&self.npc_log_path(npc), ts, Some("Relationship"), &body)?;
                }
                if let Some(npc) = npc_manager.get(*npc_b) {
                    let body = format!("*Relationship with {} shifted by {:+.2}*\n", a_name, delta);
                    append_journal_entry(&self.npc_log_path(npc), ts, Some("Relationship"), &body)?;
                }
            }
            GameEvent::NpcArrived {
                npc_id, location, ..
            } => {
                let loc = loc_of(*location);
                if let Some(npc) = npc_manager.get(*npc_id) {
                    append_journal_entry(
                        &self.npc_log_path(npc),
                        ts,
                        Some(&format!("Arrived at {}", loc)),
                        "",
                    )?;
                }
            }
            GameEvent::NpcDeparted {
                npc_id,
                location,
                to,
                ..
            } => {
                if let Some(npc) = npc_manager.get(*npc_id) {
                    let loc = loc_of(*location);
                    let to_name = loc_of(*to);
                    let body = format!("*Headed to {}*\n", to_name);
                    append_journal_entry(
                        &self.npc_log_path(npc),
                        ts,
                        Some(&format!("Departed from {}", loc)),
                        &body,
                    )?;
                }
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
                if let Some(npc) = npc_manager.get(*npc_id) {
                    let loc = loc_of(*location);
                    let body = format!("*{}*\n", activity);
                    append_journal_entry(
                        &self.npc_log_path(npc),
                        ts,
                        Some(&format!("Activity at {}", loc)),
                        &body,
                    )?;
                }
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
                if let Some(npc) = npc_manager.get(*source) {
                    let loc = loc_of(*location);
                    let body = format!("*{}*\n", content);
                    append_journal_entry(
                        &self.npc_log_path(npc),
                        ts,
                        Some(&format!("Gossip at {}", loc)),
                        &body,
                    )?;
                }
            }
            GameEvent::AddressedAbsentNpc { name, location, .. } => {
                let loc = loc_of(*location);
                let body = format!("*Addressed {} — they were not present.*\n", name);
                append_journal_entry(
                    &self.player_log_path(),
                    ts,
                    Some(&format!("Missed introduction at {}", loc)),
                    &body,
                )?;
            }
            GameEvent::PlayerMoved { from, to, .. } => {
                let to_n = loc_of(*to);
                let from_n = loc_of(*from);
                let body = format!("*From {} to {}*\n", from_n, to_n);
                let heading = match world.player_name.as_deref() {
                    Some(name) if !name.trim().is_empty() => format!("{} arrived", name),
                    _ => "Arrived".to_string(),
                };
                append_journal_entry(&self.player_log_path(), ts, Some(&heading), &body)?;
            }
            GameEvent::PlayerTaskAssigned { task, .. } => {
                let assigner = name_of(task.assigned_by);
                let location = loc_of(task.location);
                let body = format!("*{}*\n", task.description);
                let heading = format!("Task from {} at {}", assigner, location);
                append_journal_entry(&self.player_log_path(), ts, Some(&heading), &body)?;
            }
            GameEvent::PlayerTaskProgressed { task, action, .. } => {
                let location = loc_of(task.location);
                let body = format!("*{}*\n\nAction: {}\n", task.description, action);
                let heading = format!("Task in progress at {}", location);
                append_journal_entry(&self.player_log_path(), ts, Some(&heading), &body)?;
            }
            GameEvent::WeatherChanged { new_weather, .. } => {
                let body = format!("*Weather: {}*\n", new_weather);
                append_journal_entry(&self.player_log_path(), ts, Some("Weather"), &body)?;
                for npc in npc_manager.all_npcs() {
                    if let Err(e) =
                        append_journal_entry(&self.npc_log_path(npc), ts, Some("Weather"), &body)
                    {
                        tracing::warn!(npc_id = ?npc.id, "failed to write weather to npc diary: {e}");
                    }
                }
            }
            GameEvent::FestivalStarted { name, .. } => {
                let body = format!("*Festival begins: {}*\n", name);
                let heading = format!("Festival: {}", name);
                append_journal_entry(&self.player_log_path(), ts, Some(&heading), &body)?;
                for npc in npc_manager.all_npcs() {
                    if let Err(e) =
                        append_journal_entry(&self.npc_log_path(npc), ts, Some(&heading), &body)
                    {
                        tracing::warn!(npc_id = ?npc.id, "failed to write festival to npc diary: {e}");
                    }
                }
            }
            GameEvent::LifeEvent {
                npc_id,
                description,
                ..
            } => {
                if let Some(npc) = npc_manager.get(*npc_id) {
                    let body = format!("*{}*\n", description);
                    append_journal_entry(&self.npc_log_path(npc), ts, Some("Life event"), &body)?;
                }
            }
            GameEvent::NpcInteraction {
                participants,
                summary,
                ..
            } => {
                // Write one entry per participant — each gets the
                // summary in their own diary with the other names
                // formatted as "with X, Y". Self is excluded so the
                // "With X" header reads naturally.
                let trimmed = summary.trim();
                if trimmed.is_empty() {
                    return Ok(());
                }
                for pid in participants {
                    let Some(npc) = npc_manager.get(*pid) else {
                        continue;
                    };
                    let others: Vec<String> = participants
                        .iter()
                        .filter(|&p| p != pid)
                        .map(|p| name_of(*p))
                        .collect();
                    let body = if others.is_empty() {
                        format!("*{}*\n", trimmed)
                    } else {
                        format!("*With {}: {}*\n", others.join(", "), trimmed)
                    };
                    append_journal_entry(&self.npc_log_path(npc), ts, Some("Interaction"), &body)?;
                }
            }
        }
        Ok(())
    }
}

// ── Profile formatters ──────────────────────────────────────────────────────

/// Renders the full PROFILE markdown for an NPC. Includes everything that
/// goes between `PROFILE_START` and `PROFILE_END`, with one trailing
/// newline. The journal-section header is appended separately by
/// [`rewrite_profile_section`].
pub fn format_npc_profile(
    npc: &Npc,
    world: &WorldState,
    names_by_id: &HashMap<NpcId, String>,
) -> String {
    let home = npc
        .home
        .and_then(|id| world.graph.get(id).map(|d| d.name.clone()))
        .unwrap_or_else(|| "—".to_string());
    let mut out = String::new();
    out.push_str(&format!("# {} — Character Log\n", npc.name));
    out.push_str(&format!(
        "*{} · Age {} · Home: {}*\n\n",
        npc.occupation, npc.age, home
    ));

    out.push_str("## Personality\n\n");
    out.push_str(&format!("{}\n\n", npc.personality.trim()));

    out.push_str("## Intelligence\n\n");
    out.push_str(&format!(
        "- Verbal: {}/5\n- Analytical: {}/5\n- Emotional: {}/5\n- \
         Practical: {}/5\n- Wisdom: {}/5\n- Creative: {}/5\n\n",
        npc.intelligence.verbal,
        npc.intelligence.analytical,
        npc.intelligence.emotional,
        npc.intelligence.practical,
        npc.intelligence.wisdom,
        npc.intelligence.creative,
    ));

    out.push_str("## Backstory\n\n");
    if npc.knowledge.is_empty() {
        out.push_str("*(no backstory recorded)*\n\n");
    } else {
        for k in &npc.knowledge {
            out.push_str(&format!("- {}\n", k.trim()));
        }
        out.push('\n');
    }

    out.push_str("## Relationships\n\n");
    if npc.relationships.is_empty() {
        out.push_str("*(no recorded relationships)*\n\n");
    } else {
        let mut rels: Vec<_> = npc.relationships.iter().collect();
        rels.sort_by_key(|(id, _)| id.0);
        for (other_id, rel) in rels {
            let other_name = names_by_id
                .get(other_id)
                .cloned()
                .unwrap_or_else(|| format!("NPC({})", other_id.0));
            out.push_str(&format!(
                "- **{}** ({}) — strength {} `{}`\n",
                other_name,
                rel.kind,
                format_strength(rel.strength),
                strength_bar(rel.strength),
            ));
            let _ = RelationshipKind::Family; // keep import live for non-trivial use
        }
        out.push('\n');
    }

    out.push_str("## Schedule\n\n");
    if let Some(schedule) = npc.schedule() {
        out.push_str(&format_schedule(schedule, world));
    } else {
        out.push_str("*(no schedule recorded)*\n\n");
    }

    out
}

/// Renders the player's PROFILE markdown.
pub fn format_player_profile(world: &WorldState) -> String {
    let name = world
        .player_name
        .clone()
        .unwrap_or_else(|| "The Newcomer".to_string());
    let current = world
        .graph
        .get(world.player_location)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("location {}", world.player_location.0));
    let mut out = String::new();
    out.push_str(&format!("# {} — Player Log\n", name));
    out.push_str(&format!("*Currently at: {}*\n\n", current));
    out.push_str("## Visited locations\n\n");
    if world.visited_locations.is_empty() {
        out.push_str("*(none yet)*\n\n");
    } else {
        // Iterate first-visit order so the player's playthrough route
        // is visible. Older saves loaded before #1130 land with an
        // empty `visited_order`; fall back to sorted-by-id in that
        // case so the section is never empty when locations exist.
        let order: Vec<LocationId> = if world.visited_order.is_empty() {
            let mut v: Vec<LocationId> = world.visited_locations.iter().copied().collect();
            v.sort_by_key(|id| id.0);
            v
        } else {
            world.visited_order.clone()
        };
        for id in &order {
            let n = world
                .graph
                .get(*id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("location {}", id.0));
            out.push_str(&format!("- {}\n", n));
        }
        out.push('\n');
    }
    out
}

/// Renders an NPC's seasonal schedule as nested markdown lists.
pub fn format_schedule(
    schedule: &parish_npc::types::SeasonalSchedule,
    world: &WorldState,
) -> String {
    let mut out = String::new();
    for variant in &schedule.variants {
        let header = match (variant.season, variant.day_type) {
            (Some(s), Some(d)) => format!("{} · {}", season_label(s), day_label(d)),
            (Some(s), None) => season_label(s).to_string(),
            (None, Some(d)) => day_label(d).to_string(),
            (None, None) => "Default".to_string(),
        };
        out.push_str(&format!("### {}\n", header));
        if variant.entries.is_empty() {
            out.push_str("- *(empty)*\n");
        } else {
            for entry in &variant.entries {
                let loc = world
                    .graph
                    .get(entry.location)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| format!("location {}", entry.location.0));
                out.push_str(&format!(
                    "- {:02}:00–{:02}:00 @ {} — {}\n",
                    entry.start_hour, entry.end_hour, loc, entry.activity
                ));
            }
        }
        out.push('\n');
    }
    out
}

fn season_label(s: Season) -> &'static str {
    match s {
        Season::Spring => "Spring",
        Season::Summer => "Summer",
        Season::Autumn => "Autumn",
        Season::Winter => "Winter",
    }
}

fn day_label(d: DayType) -> &'static str {
    match d {
        DayType::Weekday => "Weekday",
        DayType::Sunday => "Sunday",
        DayType::MarketDay => "Market Day",
    }
}

fn format_strength(s: f64) -> String {
    format!("{:+.2}", s)
}

fn strength_bar(s: f64) -> String {
    // 11-cell bar; mid cell is zero. -1.0 → leftmost, +1.0 → rightmost.
    let cells = 11_i32;
    let mid = cells / 2;
    let pos = (s.clamp(-1.0, 1.0) * mid as f64).round() as i32 + mid;
    let mut bar = String::with_capacity(cells as usize);
    for i in 0..cells {
        bar.push(if i == pos { '●' } else { '·' });
    }
    bar
}

// ── File I/O ────────────────────────────────────────────────────────────────

/// How an NPC names the player in their own diary.
///
/// - If the NPC has been told the player's name (tracked by
///   `npc_manager.knows_player_name(npc_id)`) AND the world has it set,
///   return that name verbatim.
/// - Otherwise return `"A stranger"`. Keeps the diary entry in the
///   right POV — an NPC who hasn't been introduced wouldn't write
///   the player's name in their journal.
pub fn player_diary_label_for(
    world: &WorldState,
    npc_manager: &NpcManager,
    npc_id: NpcId,
) -> String {
    if npc_manager.knows_player_name(npc_id)
        && let Some(name) = world.player_name.as_deref()
        && !name.trim().is_empty()
    {
        return name.to_string();
    }
    "A stranger".to_string()
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

/// Replaces the contents of the PROFILE section in `path` with `profile_md`.
///
/// If `path` does not exist, creates a fresh file with the profile section
/// followed by an empty Journal stub. If the existing file has no
/// `PROFILE_END` marker (corruption / hand-edit), the journal contents
/// after the last seen `## Journal` heading are preserved verbatim; if
/// no journal heading is present either, the existing file is appended
/// as a recovery block instead of being discarded.
pub fn rewrite_profile_section(path: &Path, profile_md: &str) -> Result<()> {
    let existing = match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(e).with_context(|| format!("read {}", path.display()));
        }
    };

    // Try, in order of preservation strength:
    //   1. `<!-- PROFILE_END -->` marker — normal path.
    //   2. `## Journal` heading — covers hand-edited files where the
    //      profile markers were stripped but the journal heading
    //      survives.
    //   3. Entire existing contents under a recovery block — last
    //      resort so we never silently drop user-visible history.
    let journal_owned: Option<String> = existing.as_deref().and_then(|content| {
        if extract_after_profile_end(content).is_some() {
            None
        } else if let Some(idx) = content.find("\n## Journal") {
            Some(content[idx..].to_string())
        } else {
            Some(format!(
                "\n## Journal\n\n<!-- Recovered from a file with no PROFILE_END marker -->\n\n{}\n",
                content.trim_end(),
            ))
        }
    });
    let journal_section: &str = if let Some(s) = journal_owned.as_deref() {
        s
    } else if let Some(content) = existing.as_deref() {
        extract_after_profile_end(content).unwrap_or("\n## Journal\n\n")
    } else {
        "\n## Journal\n\n"
    };

    let mut out = String::with_capacity(profile_md.len() + journal_section.len() + 64);
    out.push_str(PROFILE_START);
    out.push('\n');
    out.push_str(profile_md);
    if !profile_md.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(PROFILE_END);
    out.push_str("\n\n---\n");
    if !journal_section.starts_with('\n') {
        out.push('\n');
    }
    out.push_str(journal_section);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all {}", parent.display()))?;
    }
    std::fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Returns the substring of `contents` that starts immediately after the
/// closing PROFILE marker. Returns `None` if the marker is absent.
fn extract_after_profile_end(contents: &str) -> Option<&str> {
    let idx = contents.find(PROFILE_END)?;
    let tail = &contents[idx + PROFILE_END.len()..];
    // Skip a single trailing newline so the rewritten output doesn't
    // accumulate blank lines on each session start.
    let tail = tail.strip_prefix('\n').unwrap_or(tail);
    // Skip the "---" separator we always write, plus the newlines around it.
    let tail = tail.strip_prefix("\n---\n").unwrap_or(tail);
    Some(tail)
}

/// Appends a journal entry to `path`. If `path` doesn't exist (no profile
/// has been written yet — unusual but possible during tests), seeds a
/// minimal journal stub first.
///
/// `heading_suffix` becomes the part of the `### …` line after a `— `
/// separator. Pass `None` for a plain date-time heading.
pub fn append_journal_entry(
    path: &Path,
    game_time: DateTime<Utc>,
    heading_suffix: Option<&str>,
    body: &str,
) -> Result<()> {
    let entry = JournalEntry::new(game_time, heading_suffix, body);
    append_prepared_entry(path, &entry)
}

/// A fully-rendered journal entry whose heading + block are built **once** and
/// can be appended to many files (e.g. a world-wide `WeatherChanged` event that
/// fans out to every location journal). Sharing the rendered strings across the
/// fan-out is the batching optimisation behind parish-core TD-031 — previously
/// every per-location append re-`format!`'d the identical heading and block.
#[derive(Debug, Clone)]
pub struct JournalEntry {
    /// `### <time>[ — <suffix>]\n`
    heading: String,
    /// The full block written on append: heading + body + trailing blank line.
    block: String,
    /// The idempotence needle (heading + body, without the leading `\n`),
    /// trimmed of trailing newlines, precomputed once.
    needle: String,
}

impl JournalEntry {
    /// Renders a journal entry once for reuse across one or more files.
    pub fn new(game_time: DateTime<Utc>, heading_suffix: Option<&str>, body: &str) -> Self {
        let heading = match heading_suffix {
            Some(suffix) if !suffix.is_empty() => {
                format!("### {} — {}\n", format_game_time(game_time), suffix)
            }
            _ => format!("### {}\n", format_game_time(game_time)),
        };

        let mut block = String::with_capacity(heading.len() + body.len() + 2);
        block.push_str(&heading);
        block.push_str(body);
        if !block.ends_with('\n') {
            block.push('\n');
        }
        block.push('\n');

        let needle = format!("{}{}", heading, body)
            .trim_end_matches('\n')
            .to_string();

        Self {
            heading,
            block,
            needle,
        }
    }
}

/// Appends a single pre-rendered [`JournalEntry`] to `path`, seeding a stub if
/// the file does not yet exist and skipping the write when the same entry is
/// already present (idempotence).
fn append_prepared_entry(path: &Path, entry: &JournalEntry) -> Result<()> {
    let _ = &entry.heading; // heading is embedded in block + needle.
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create_dir_all {}", parent.display()))?;
        }
        // Minimal stub so subsequent profile rewrites can find the marker.
        let stub = format!(
            "{start}\n# (profile pending)\n{end}\n\n---\n\n## Journal\n\n",
            start = PROFILE_START,
            end = PROFILE_END,
        );
        std::fs::write(path, stub).with_context(|| format!("seed stub {}", path.display()))?;
    }

    // Idempotence: if the existing file already contains an entry with
    // the same heading AND body, skip the append. Catches the
    // "replayed the same fixture from a save reset" case — same
    // in-fiction timestamp + same destination is by definition the
    // same journal event, so re-appending it would be Groundhog Day.
    if !entry.needle.is_empty()
        && let Ok(existing) = std::fs::read_to_string(path)
        && existing.contains(&entry.needle)
    {
        return Ok(());
    }

    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("open append {}", path.display()))?;
    f.write_all(entry.block.as_bytes())
        .with_context(|| format!("append {}", path.display()))?;
    Ok(())
}

/// Fans a single world-wide journal entry out to many location journals,
/// rendering the heading/body **once** and reusing it for every path.
///
/// This is the batched path behind parish-core TD-031: a `WeatherChanged` /
/// `FestivalStarted` event applies to every location, so the previous code
/// rebuilt the identical entry string for each of the ~22 Rundale locations.
/// Here the entry is constructed a single time and applied across the slice,
/// and the whole fan-out is a single call the event-bus subscriber makes —
/// giving one place to add frequency/backpressure gating in future.
///
/// Per-path failures are logged and skipped (a world-wide event should still
/// reach the other locations); the function returns the number of paths that
/// were written or already current.
pub fn append_journal_entry_batch<'a, I>(
    paths: I,
    game_time: DateTime<Utc>,
    heading_suffix: Option<&str>,
    body: &str,
) -> usize
where
    I: IntoIterator<Item = &'a Path>,
{
    let entry = JournalEntry::new(game_time, heading_suffix, body);
    let mut ok = 0usize;
    for path in paths {
        match append_prepared_entry(path, &entry) {
            Ok(()) => ok += 1,
            Err(e) => {
                tracing::warn!(path = %path.display(), "batched journal append failed: {e}");
            }
        }
    }
    ok
}

/// Formats an in-fiction timestamp as `Weekday DD Month YYYY, HH:MM`.
fn format_game_time(t: DateTime<Utc>) -> String {
    // Avoid depending on locale or chrono format specifiers that allocate
    // unnecessarily — we want stable, ASCII output for diff-friendly logs.
    let weekday = match t.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    };
    let month = match t.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "?",
    };
    format!(
        "{} {} {} {}, {:02}:{:02}",
        weekday,
        t.day(),
        month,
        t.year(),
        t.hour(),
        t.minute()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use parish_npc::manager::NpcManager;
    use parish_npc::types::{Intelligence, Relationship, RelationshipKind};
    use parish_world::WorldState;
    use std::collections::HashMap;

    fn test_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 3, 14, 30, 0).unwrap()
    }

    fn make_npc(id: u32, name: &str) -> Npc {
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(id);
        npc.name = name.to_string();
        npc.brief_description = format!("a person called {}", name);
        npc.age = 40;
        npc.occupation = "Publican".to_string();
        npc.personality = "Warm-hearted".to_string();
        npc.pronouns = "they/them".to_string();
        npc.intelligence = Intelligence::new(3, 3, 3, 3, 3, 3);
        npc.set_location(LocationId(1));
        npc.mood = "content".to_string();
        npc
    }

    #[test]
    fn slugify_basic_cases() {
        assert_eq!(slugify("Padraig Darcy"), "padraig-darcy");
        assert_eq!(slugify("Niamh O'Brien"), "niamh-o-brien");
        assert_eq!(slugify("---weird---"), "weird");
        assert_eq!(slugify(""), "unnamed");
    }

    #[test]
    fn profile_section_contains_name() {
        let npc = make_npc(7, "Padraig Darcy");
        let world = WorldState::new();
        let names = HashMap::from([(npc.id, npc.name.clone())]);
        let profile = format_npc_profile(&npc, &world, &names);
        assert!(profile.contains("Padraig Darcy"), "profile = {}", profile);
        assert!(profile.contains("Age 40"), "profile = {}", profile);
        assert!(profile.contains("## Intelligence"), "profile = {}", profile);
        assert!(profile.contains("## Schedule"), "profile = {}", profile);
    }

    #[test]
    fn player_profile_visited_locations_use_first_visit_order() {
        // #1130 / F14: visited locations render in the order
        // `mark_visited` recorded them, not by numeric LocationId.
        // WorldState::new()'s graph is empty, so the renderer falls
        // back to `format!("location {}", id.0)` for each id — the
        // ordering check is what's under test, not the rendered
        // names.
        let mut world = WorldState::new();
        // WorldState::new() seeds visited with LocationId(1). Visit
        // three more in a deliberately-out-of-order sequence: 5, 3, 2.
        world.mark_visited(LocationId(5));
        world.mark_visited(LocationId(3));
        world.mark_visited(LocationId(2));

        let profile = format_player_profile(&world);
        let visited_section = profile
            .split("## Visited locations\n\n")
            .nth(1)
            .expect("profile must have the Visited locations section");
        let p1 = visited_section
            .find("- location 1")
            .expect("LocationId(1) missing from visited section");
        let p5 = visited_section
            .find("- location 5")
            .expect("LocationId(5) missing from visited section");
        let p3 = visited_section
            .find("- location 3")
            .expect("LocationId(3) missing from visited section");
        let p2 = visited_section
            .find("- location 2")
            .expect("LocationId(2) missing from visited section");
        assert!(
            p1 < p5 && p5 < p3 && p3 < p2,
            "visited locations must render in first-visit order \
             (1, 5, 3, 2), got positions 1@{p1} 5@{p5} 3@{p3} 2@{p2}.\n\
             section:\n{visited_section}",
        );
    }

    #[test]
    fn profile_section_lists_relationships_with_names() {
        let mut npc = make_npc(7, "Padraig Darcy");
        npc.relationships
            .insert(NpcId(8), Relationship::new(RelationshipKind::Family, 0.5));
        let world = WorldState::new();
        let names = HashMap::from([
            (NpcId(7), "Padraig Darcy".to_string()),
            (NpcId(8), "Niamh Darcy".to_string()),
        ]);
        let profile = format_npc_profile(&npc, &world, &names);
        assert!(profile.contains("Niamh Darcy"));
        assert!(profile.contains("family"));
        assert!(profile.contains("+0.50"));
    }

    #[test]
    fn profile_rewrite_preserves_journal() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("npc-001-test.md");
        rewrite_profile_section(&path, "# First profile\n").unwrap();
        // Simulate an in-session journal append.
        append_journal_entry(
            &path,
            test_time(),
            Some("Arrived at Darcy's Pub"),
            "*From The Crossroads to Darcy's Pub*\n",
        )
        .unwrap();
        // Rewrite profile section — journal entries must survive.
        rewrite_profile_section(&path, "# Second profile\n").unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("# Second profile"),
            "new profile missing: {}",
            contents,
        );
        assert!(
            !contents.contains("# First profile"),
            "old profile still present: {}",
            contents,
        );
        assert!(
            contents.contains("Arrived at Darcy's Pub"),
            "journal entry lost across profile rewrite: {}",
            contents,
        );
    }

    #[test]
    fn player_moved_event_appends_to_player_log() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        let world = WorldState::new();
        let npcs = NpcManager::new();
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let player = std::fs::read_to_string(mgr.player_log_path()).unwrap();
        assert!(
            player.contains("Arrived"),
            "player log missing arrival heading: {}",
            player,
        );
        assert!(
            player.contains("1820"),
            "player log missing game-time year: {}",
            player,
        );
    }

    #[test]
    fn player_moved_uses_player_name_when_set() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        let mut world = WorldState::new();
        world.player_name = Some("Aiden".to_string());
        let npcs = NpcManager::new();
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let player = std::fs::read_to_string(mgr.player_log_path()).unwrap();
        assert!(
            player.contains("Aiden arrived"),
            "player log should use player name in arrival heading: {}",
            player,
        );
        assert!(
            !player.contains("Arrived at"),
            "player log should not contain 'Arrived at' location suffix: {}",
            player,
        );
    }

    #[test]
    fn player_moved_fallback_when_name_unset() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        let world = WorldState::new();
        let npcs = NpcManager::new();
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let player = std::fs::read_to_string(mgr.player_log_path()).unwrap();
        // When player_name is None, heading should be plain "Arrived"
        assert!(
            player.contains("Arrived\n"),
            "player log should contain plain 'Arrived' heading when name is unset: {}",
            player,
        );
        assert!(
            !player.contains("Arrived at"),
            "player log should not contain 'Arrived at' location suffix: {}",
            player,
        );
    }

    #[test]
    fn dialogue_event_writes_player_and_npc_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let mut npcs = NpcManager::new();
        let npc = make_npc(7, "Padraig Darcy");
        let npc_path_marker = npc.id;
        npcs.add_npc(npc);
        let world = WorldState::new();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::DialogueOccurred {
            npc_id: npc_path_marker,
            location: parish_world::LocationId(1),
            summary: "discussed weather".into(),
            player_said: Some("Good afternoon, Padraig.".into()),
            npc_said: Some("Ah, God bless ye.".into()),
            request_id: None,
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let npc_log =
            std::fs::read_to_string(mgr.npc_log_path(npcs.get(npc_path_marker).unwrap())).unwrap();
        // NPC's diary POV — player appears as "A stranger" (this NPC
        // hasn't been introduced yet); NPC refers to themselves as "I".
        assert!(
            npc_log.contains("**A stranger:** Good afternoon, Padraig."),
            "player line missing or wrong POV: {}",
            npc_log,
        );
        assert!(
            npc_log.contains("**I:** Ah, God bless ye."),
            "npc line missing or wrong POV: {}",
            npc_log,
        );
    }

    #[test]
    fn npc_activity_event_writes_authored_activity_to_npc_log() {
        let tmp = tempfile::tempdir().unwrap();
        let mut npcs = NpcManager::new();
        let npc = make_npc(7, "Padraig Darcy");
        let npc_id = npc.id;
        npcs.add_npc(npc);
        let world = WorldState::new();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::NpcActivity {
            npc_id,
            location: parish_world::LocationId(1),
            activity: "tending bar".to_string(),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let log = std::fs::read_to_string(mgr.npc_log_path(npcs.get(npc_id).unwrap())).unwrap();
        assert!(
            log.contains("Activity at"),
            "activity heading missing: {}",
            log,
        );
        assert!(
            log.contains("*tending bar*"),
            "activity body missing or wrong italic marker: {}",
            log,
        );
    }

    #[test]
    fn npc_activity_event_with_empty_text_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let mut npcs = NpcManager::new();
        let npc = make_npc(7, "Padraig Darcy");
        let npc_id = npc.id;
        npcs.add_npc(npc);
        let world = WorldState::new();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let path = mgr.npc_log_path(npcs.get(npc_id).unwrap());
        let before = std::fs::read_to_string(&path).unwrap();

        let event = GameEvent::NpcActivity {
            npc_id,
            location: parish_world::LocationId(1),
            activity: "   ".to_string(),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            before, after,
            "empty/whitespace activity must not append a journal entry"
        );
    }

    #[test]
    fn gossip_spread_event_writes_source_npc_log() {
        let tmp = tempfile::tempdir().unwrap();
        let mut npcs = NpcManager::new();
        let npc = make_npc(7, "Padraig Darcy");
        let source_id = npc.id;
        npcs.add_npc(npc);
        let world = WorldState::new();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::GossipSpread {
            source: source_id,
            location: parish_world::LocationId(1),
            content: "the landlord raised the rent again".to_string(),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let log = std::fs::read_to_string(mgr.npc_log_path(npcs.get(source_id).unwrap())).unwrap();
        assert!(log.contains("Gossip at"), "gossip heading missing: {}", log,);
        assert!(
            log.contains("*the landlord raised the rent again*"),
            "gossip body missing: {}",
            log,
        );
    }

    #[test]
    fn addressed_absent_npc_event_writes_player_log() {
        // F9 / #1135: player addresses an NPC who isn't here — event
        // lands in player.md so a post-session scan captures the
        // missed introduction.
        let tmp = tempfile::tempdir().unwrap();
        let npcs = NpcManager::new();
        let world = WorldState::new();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::AddressedAbsentNpc {
            name: "Mrs. Hannigan".to_string(),
            location: parish_world::LocationId(1),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        let log = std::fs::read_to_string(mgr.player_log_path()).unwrap();
        assert!(
            log.contains("Missed introduction at"),
            "heading missing from player log: {}",
            log,
        );
        assert!(
            log.contains("Mrs. Hannigan"),
            "absent npc name missing from player log: {}",
            log,
        );
    }

    #[test]
    fn disabled_manager_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), false);
        let world = WorldState::new();
        let npcs = NpcManager::new();
        mgr.write_all_profiles(&world, &npcs).unwrap();
        // No files should have been created — log_dir is empty PathBuf.
        let event = GameEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();
        assert!(!mgr.player_log_path().exists() || mgr.log_dir().as_os_str().is_empty());
    }

    #[test]
    fn weather_changed_fans_out_to_npc_journals() {
        let tmp = tempfile::tempdir().unwrap();
        let mut npcs = NpcManager::new();
        let npc1 = make_npc(7, "Padraig Darcy");
        let npc2 = make_npc(8, "Niamh Darcy");
        let id1 = npc1.id;
        let id2 = npc2.id;
        npcs.add_npc(npc1);
        npcs.add_npc(npc2);
        let world = WorldState::new();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::WeatherChanged {
            new_weather: "Heavy rain".to_string(),
            timestamp: test_time(),
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let log1 = std::fs::read_to_string(mgr.npc_log_path(npcs.get(id1).unwrap())).unwrap();
        let log2 = std::fs::read_to_string(mgr.npc_log_path(npcs.get(id2).unwrap())).unwrap();
        assert!(
            log1.contains("Weather") && log1.contains("Heavy rain"),
            "npc1 diary missing weather entry: {}",
            log1,
        );
        assert!(
            log2.contains("Weather") && log2.contains("Heavy rain"),
            "npc2 diary missing weather entry: {}",
            log2,
        );

        let player = std::fs::read_to_string(mgr.player_log_path()).unwrap();
        assert!(
            player.contains("Weather") && player.contains("Heavy rain"),
            "player diary missing weather entry: {}",
            player,
        );
    }

    #[test]
    fn festival_started_fans_out_to_npc_journals() {
        let tmp = tempfile::tempdir().unwrap();
        let mut npcs = NpcManager::new();
        let npc1 = make_npc(7, "Padraig Darcy");
        let npc2 = make_npc(8, "Niamh Darcy");
        let id1 = npc1.id;
        let id2 = npc2.id;
        npcs.add_npc(npc1);
        npcs.add_npc(npc2);
        let world = WorldState::new();
        let mgr = CharacterLogManager::new_at_dir(tmp.path().to_path_buf(), true);
        mgr.write_all_profiles(&world, &npcs).unwrap();

        let event = GameEvent::FestivalStarted {
            name: "Bealtaine".to_string(),
            timestamp: test_time(),
            location: None,
        };
        mgr.process_event(&event, &world, &npcs).unwrap();

        let log1 = std::fs::read_to_string(mgr.npc_log_path(npcs.get(id1).unwrap())).unwrap();
        let log2 = std::fs::read_to_string(mgr.npc_log_path(npcs.get(id2).unwrap())).unwrap();
        assert!(
            log1.contains("Festival") && log1.contains("Bealtaine"),
            "npc1 diary missing festival entry: {}",
            log1,
        );
        assert!(
            log2.contains("Festival") && log2.contains("Bealtaine"),
            "npc2 diary missing festival entry: {}",
            log2,
        );

        let player = std::fs::read_to_string(mgr.player_log_path()).unwrap();
        assert!(
            player.contains("Festival") && player.contains("Bealtaine"),
            "player diary missing festival entry: {}",
            player,
        );
    }

    // ── TD-031: batched world-wide journal fan-out ───────────────────────────

    // AC-7: a single batched fan-out writes the entry to EVERY target journal —
    // no path is dropped. Behaviour is preserved relative to the previous
    // per-location loop; only the rendering/IO is batched.
    #[test]
    fn batch_append_writes_entry_to_every_path() {
        let tmp = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = (1..=22)
            .map(|i| tmp.path().join(format!("loc-{:03}.md", i)))
            .collect();

        let ok = append_journal_entry_batch(
            paths.iter().map(PathBuf::as_path),
            test_time(),
            Some("Weather"),
            "*Weather: Storm*\n",
        );
        assert_eq!(ok, 22, "every location journal should be written");

        for p in &paths {
            let body = std::fs::read_to_string(p).unwrap();
            assert!(
                body.contains("Weather") && body.contains("Storm"),
                "journal {} missing weather entry:\n{}",
                p.display(),
                body
            );
        }
    }

    // AC-6: the batched helper renders the heading/body exactly once and reuses
    // it — re-running the same batch is idempotent (no duplicate entries), the
    // same guard the per-location path used to provide.
    #[test]
    fn batch_append_is_idempotent_per_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("loc-001.md");
        let slice = [path.as_path()];

        append_journal_entry_batch(slice, test_time(), Some("Weather"), "*Weather: Fog*\n");
        append_journal_entry_batch(slice, test_time(), Some("Weather"), "*Weather: Fog*\n");

        let body = std::fs::read_to_string(&path).unwrap();
        let occurrences = body.matches("Weather: Fog").count();
        assert_eq!(
            occurrences, 1,
            "duplicate world-wide event must not double-append:\n{}",
            body
        );
    }

    // A pre-rendered JournalEntry applied directly matches the single-path
    // convenience wrapper — proves the batch path and the legacy single path
    // produce byte-identical output.
    #[test]
    fn prepared_entry_matches_single_append() {
        let tmp = tempfile::tempdir().unwrap();
        let single = tmp.path().join("single.md");
        let batched = tmp.path().join("batched.md");

        append_journal_entry(&single, test_time(), Some("Weather"), "*Weather: Rain*\n").unwrap();
        append_journal_entry_batch(
            [batched.as_path()],
            test_time(),
            Some("Weather"),
            "*Weather: Rain*\n",
        );

        assert_eq!(
            std::fs::read_to_string(&single).unwrap(),
            std::fs::read_to_string(&batched).unwrap(),
            "batched fan-out output must match the single-append output byte-for-byte"
        );
    }
}
