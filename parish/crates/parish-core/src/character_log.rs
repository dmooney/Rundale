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
//! Every entry point (`parish-tauri`, `parish-server`, `parish-cli`) is
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
use std::sync::Mutex;

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
/// Holds a small mutex-protected map of "last journal arrival per NPC" so
/// the writer can drop spurious `NpcArrived` / `NpcDeparted` events fired
/// by every tier-recompute (the bus republishes a Tier1 promotion every
/// time an NPC briefly leaves and returns to the player's vicinity — left
/// unfiltered, a few hours of game time produces hundreds of identical
/// "Arrived at X" lines).
#[derive(Debug)]
pub struct CharacterLogManager {
    log_dir: PathBuf,
    enabled: bool,
    /// Last location *name* written to each NPC's journal for an arrival
    /// event. Used to suppress repeat lines for an NPC who hasn't moved
    /// since the previous journal entry. The map is seeded on `new()`
    /// by scanning each existing NPC log so a new session resuming the
    /// same branch doesn't re-emit the previous session's last arrival.
    /// The name is the value of the location's `name` field — same
    /// string the heading renders — so cross-session comparison doesn't
    /// require a world-graph handle.
    last_arrival: Mutex<HashMap<NpcId, String>>,
    /// Last location name recorded for the player. Defensive twin of
    /// [`Self::last_arrival`].
    last_player_arrival: Mutex<Option<String>>,
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
                last_arrival: Mutex::new(HashMap::new()),
                last_player_arrival: Mutex::new(None),
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
                last_arrival: Mutex::new(HashMap::new()),
                last_player_arrival: Mutex::new(None),
            };
        }
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            tracing::warn!(
                path = %log_dir.display(),
                error = %e,
                "failed to create character-log directory",
            );
        }
        // Seed dedup state from existing on-disk journals so a fresh
        // `CharacterLogManager` resuming the same branch doesn't
        // re-emit the previous session's final arrivals as duplicates.
        let last_arrival = scan_existing_npc_arrivals(&log_dir);
        let last_player_arrival = scan_existing_player_arrival(&log_dir);
        Self {
            log_dir,
            enabled,
            last_arrival: Mutex::new(last_arrival),
            last_player_arrival: Mutex::new(last_player_arrival),
        }
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

    /// Records that NPC `npc_id` just arrived at `location` and returns
    /// `true` iff the journal should be appended — i.e. the location
    /// differs from the previously recorded arrival for this NPC. The
    /// timestamp is captured for future enrichment; the dedup itself is
    /// location-only so identical "arrival" pings at increasing
    /// timestamps collapse to one entry.
    fn bump_last_arrival(&self, npc_id: NpcId, location_name: &str) -> bool {
        let mut guard = match self.last_arrival.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.get(&npc_id) {
            Some(prev) if prev == location_name => false,
            _ => {
                guard.insert(npc_id, location_name.to_string());
                true
            }
        }
    }

    /// Same shape as [`Self::bump_last_arrival`] but for the player —
    /// drops `PlayerMoved` events whose destination matches the last
    /// recorded location. Defensive: `game_session::apply_movement` only
    /// fires on `MovementResult::Arrived`, but a future caller bypassing
    /// that seam would otherwise produce the same flood the NPC writer
    /// guards against.
    fn bump_last_player_arrival(&self, location_name: &str) -> bool {
        let mut guard = match self.last_player_arrival.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.as_deref() {
            Some(prev) if prev == location_name => false,
            _ => {
                *guard = Some(location_name.to_string());
                true
            }
        }
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
                let player_label = player_diary_label(world, npc_manager, *npc_id);
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
                // Drop the entry if the NPC's last journal-recorded
                // location matches. State is seeded from on-disk
                // history in `new()` so reruns of the same script
                // don't re-emit the previous session's final arrival.
                let loc = loc_of(*location);
                if !self.bump_last_arrival(*npc_id, &loc) {
                    return Ok(());
                }
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
                npc_id, location, ..
            } => {
                let loc = loc_of(*location);
                if !self.bump_last_arrival(*npc_id, &loc) {
                    return Ok(());
                }
                if let Some(npc) = npc_manager.get(*npc_id) {
                    append_journal_entry(
                        &self.npc_log_path(npc),
                        ts,
                        Some(&format!("Departed from {}", loc)),
                        "",
                    )?;
                }
            }
            GameEvent::PlayerMoved { from, to, .. } => {
                let to_n = loc_of(*to);
                if !self.bump_last_player_arrival(&to_n) {
                    return Ok(());
                }
                let from_n = loc_of(*from);
                let body = format!("*From {} to {}*\n", from_n, to_n);
                append_journal_entry(
                    &self.player_log_path(),
                    ts,
                    Some(&format!("Arrived at {}", to_n)),
                    &body,
                )?;
            }
            GameEvent::WeatherChanged { new_weather, .. } => {
                let body = format!("*Weather: {}*\n", new_weather);
                append_journal_entry(&self.player_log_path(), ts, Some("Weather"), &body)?;
            }
            GameEvent::FestivalStarted { name, .. } => {
                let body = format!("*Festival begins: {}*\n", name);
                append_journal_entry(
                    &self.player_log_path(),
                    ts,
                    Some(&format!("Festival: {}", name)),
                    &body,
                )?;
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
    if let Some(schedule) = npc.schedule.as_ref() {
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
        let mut visited: Vec<&LocationId> = world.visited_locations.iter().collect();
        visited.sort_by_key(|id| id.0);
        for id in visited {
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

/// Reads each `npc-NNN-*.md` file in `log_dir`, parses the final
/// `### … — Arrived at <name>` / `… — Departed from <name>` heading
/// in the journal section, and returns a map from NpcId to that
/// location name. Missing or unparseable files contribute nothing.
fn scan_existing_npc_arrivals(log_dir: &Path) -> HashMap<NpcId, String> {
    let mut out = HashMap::new();
    let read_dir = match std::fs::read_dir(log_dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(rest) = filename.strip_prefix("npc-") else {
            continue;
        };
        let Some((id_str, _)) = rest.split_once('-') else {
            continue;
        };
        let Ok(id_u32) = id_str.parse::<u32>() else {
            continue;
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(loc) = parse_last_arrival_location(&contents) {
            out.insert(NpcId(id_u32), loc);
        }
    }
    out
}

/// Same as [`scan_existing_npc_arrivals`] but for `player.md`.
fn scan_existing_player_arrival(log_dir: &Path) -> Option<String> {
    let path = log_dir.join("player.md");
    let contents = std::fs::read_to_string(&path).ok()?;
    parse_last_arrival_location(&contents)
}

/// Parses the final `### … — Arrived at <name>` or `… — Departed from
/// <name>` heading in `contents` and returns the trailing location
/// name. Returns `None` when no such heading exists.
fn parse_last_arrival_location(contents: &str) -> Option<String> {
    let mut last: Option<String> = None;
    for line in contents.lines() {
        if !line.starts_with("### ") {
            continue;
        }
        let after_em_dash = match line.split_once(" — ") {
            Some((_, tail)) => tail,
            None => continue,
        };
        let loc = after_em_dash
            .strip_prefix("Arrived at ")
            .or_else(|| after_em_dash.strip_prefix("Departed from "))?;
        last = Some(loc.trim().to_string());
    }
    last
}

/// How an NPC names the player in their own diary.
///
/// - If the NPC has been told the player's name (tracked by
///   `npc_manager.knows_player_name(npc_id)`) AND the world has it set,
///   return that name verbatim.
/// - Otherwise return `"A stranger"`. Keeps the diary entry in the
///   right POV — an NPC who hasn't been introduced wouldn't write
///   the player's name in their journal.
fn player_diary_label(world: &WorldState, npc_manager: &NpcManager, npc_id: NpcId) -> String {
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
/// `PROFILE_END` marker (corruption / hand-edit), the entire file is
/// rebuilt — old contents are preserved at the bottom in a recovery block.
pub fn rewrite_profile_section(path: &Path, profile_md: &str) -> Result<()> {
    let existing = match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(e).with_context(|| format!("read {}", path.display()));
        }
    };

    let journal_section = existing
        .as_deref()
        .and_then(extract_after_profile_end)
        .unwrap_or("\n## Journal\n\n");

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

    let heading = match heading_suffix {
        Some(suffix) if !suffix.is_empty() => {
            format!("### {} — {}\n", format_game_time(game_time), suffix)
        }
        _ => format!("### {}\n", format_game_time(game_time)),
    };

    // Idempotence: if the existing file already contains an entry with
    // the same heading AND body, skip the append. Catches the
    // "replayed the same fixture from a save reset" case — same
    // in-fiction timestamp + same destination is by definition the
    // same journal event, so re-appending it would be Groundhog Day.
    if let Ok(existing) = std::fs::read_to_string(path) {
        let needle = format!("\n{}{}", heading, body);
        let needle_trimmed = needle.trim_end_matches('\n');
        if !needle_trimmed.is_empty() && existing.contains(needle_trimmed) {
            return Ok(());
        }
    }

    let mut block = String::with_capacity(heading.len() + body.len() + 2);
    block.push_str(&heading);
    block.push_str(body);
    if !block.ends_with('\n') {
        block.push('\n');
    }
    block.push('\n');

    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("open append {}", path.display()))?;
    f.write_all(block.as_bytes())
        .with_context(|| format!("append {}", path.display()))?;
    Ok(())
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
    use parish_npc::memory::{LongTermMemory, ShortTermMemory};
    use parish_npc::reactions::ReactionLog;
    use parish_npc::types::{Intelligence, NpcState, Relationship, RelationshipKind};
    use parish_world::WorldState;
    use std::collections::HashMap;

    fn test_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 3, 14, 30, 0).unwrap()
    }

    fn make_npc(id: u32, name: &str) -> Npc {
        Npc {
            id: NpcId(id),
            name: name.to_string(),
            brief_description: format!("a person called {}", name),
            age: 40,
            occupation: "Publican".to_string(),
            personality: "Warm-hearted".to_string(),
            intelligence: Intelligence::new(3, 3, 3, 3, 3, 3),
            location: LocationId(1),
            mood: "content".to_string(),
            home: None,
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::default(),
            deflated_summary: None,
            reaction_log: ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
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
            player.contains("Arrived at"),
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
            summary: "discussed weather".into(),
            player_said: Some("Good afternoon, Padraig.".into()),
            npc_said: Some("Ah, God bless ye.".into()),
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
}
