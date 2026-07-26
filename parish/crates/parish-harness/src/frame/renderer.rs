//! State-frame renderer.
//!
//! Each turn the harness renders a "state-frame" from the authoritative
//! [`EngineState`] plus the turn's narrative. Two artifacts come out:
//!
//! - an **SVG** (text) — human/judge-readable: the clock, location, co-located
//!   NPCs with mood, active task progression, grapevine status, and the
//!   narrative. This exposes simulation data the *player* never sees (NPC
//!   moods, grapevine distortion), which is exactly the ground truth the judge
//!   needs.
//! - a **PNG** (raster) — a compact, deterministic visual encoding of the same
//!   facts as colored cells/bars, for the dashboard turn gallery. Built with
//!   the pure-Rust `png` crate (no font/system deps).
//!
//! Per rule #14 the renderer **validates content**: a blank/degenerate PNG is
//! an `Err`, never silently shipped, so a downstream consumer can't bundle an
//! empty frame.

use parish_core::ipc::EngineState;

use crate::error::{HarnessError, Result};

const W: u32 = 480;
const H: u32 = 240;
const SVG_TASK_Y: u32 = 146;
const SVG_NPC_FIRST_Y: u32 = 164;
const SVG_NPC_ROW_STEP: u32 = 14;
const SVG_MAX_NPC_ROWS: usize = 4;
const SVG_NARRATIVE_Y: u32 = 224;

/// A rendered state-frame: SVG text + PNG bytes.
#[derive(Debug, Clone)]
pub struct StateFrame {
    pub svg: String,
    pub png: Vec<u8>,
}

/// Render the state-frame for one turn.
pub fn render(state: &EngineState, narrative: &str, turn_index: u32) -> Result<StateFrame> {
    let svg = render_svg(state, narrative, turn_index);
    if svg.trim().is_empty() {
        return Err(HarnessError::BlankArtifact(
            "state-frame SVG was empty".into(),
        ));
    }
    let pixels = render_pixels(state);
    if is_blank(&pixels) {
        return Err(HarnessError::BlankArtifact(
            "state-frame PNG had no visual content (single flat color)".into(),
        ));
    }
    let png = encode_png(&pixels)?;
    Ok(StateFrame { svg, png })
}

/// Build the readable SVG.
fn render_svg(state: &EngineState, narrative: &str, turn_index: u32) -> String {
    let clock = &state.clock;
    let scene = &state.active_scene;
    let mut npc_lines = String::new();
    let visible_npc_count = if state.npcs.here.len() > SVG_MAX_NPC_ROWS {
        SVG_MAX_NPC_ROWS - 1
    } else {
        state.npcs.here.len()
    };
    for (row, npc) in state.npcs.here.iter().take(visible_npc_count).enumerate() {
        let y = SVG_NPC_FIRST_Y + row as u32 * SVG_NPC_ROW_STEP;
        npc_lines.push_str(&format!(
            "<text x=\"16\" y=\"{y}\" font-size=\"12\" fill=\"#cdd6f4\">- {} ({}){}</text>",
            xml_escape(&npc.display_name),
            xml_escape(&npc.mood),
            if npc.introduced { " [known]" } else { "" }
        ));
    }
    let hidden_npc_count = state.npcs.here.len().saturating_sub(visible_npc_count);
    if hidden_npc_count > 0 {
        let y = SVG_NPC_FIRST_Y + visible_npc_count as u32 * SVG_NPC_ROW_STEP;
        let noun = if hidden_npc_count == 1 { "NPC" } else { "NPCs" };
        npc_lines.push_str(&format!(
            "<text x=\"16\" y=\"{y}\" font-size=\"12\" fill=\"#cdd6f4\">… +{hidden_npc_count} more {noun}</text>"
        ));
    }
    let task_line = state
        .player
        .active_tasks
        .first()
        .map(|task| {
            let description: String = task.description.chars().take(72).collect();
            let more = state.player.active_tasks.len().saturating_sub(1);
            format!(
                "active task: #{} [{}] {}{}",
                task.id,
                task.status_label(),
                description,
                if more == 0 {
                    String::new()
                } else {
                    format!(" (+{more} more)")
                }
            )
        })
        .unwrap_or_else(|| "active tasks: none".to_string());
    let narrative_excerpt: String = narrative.chars().take(220).collect();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{W}\" height=\"{H}\" \
         viewBox=\"0 0 {W} {H}\">\
         <rect width=\"{W}\" height=\"{H}\" fill=\"#1e1e2e\"/>\
         <text x=\"16\" y=\"28\" font-size=\"16\" fill=\"#f5c2e7\">Turn {turn_index} — {loc}</text>\
         <text x=\"16\" y=\"52\" font-size=\"13\" fill=\"#a6adc8\">{hh:02}:{mm:02} {tod}, {dow} ({daytype}), {season}, {weather}</text>\
         <text x=\"16\" y=\"74\" font-size=\"12\" fill=\"#a6adc8\">player visited {visited} locations{paused}</text>\
         <text x=\"16\" y=\"104\" font-size=\"13\" fill=\"#94e2d5\">NPCs here: {here} / {total} roster ({introduced} known)</text>\
         <text x=\"16\" y=\"126\" font-size=\"12\" fill=\"#f9e2af\">grapevine: {items} items, {distorted} distorted (max level {maxd})</text>\
         <text x=\"16\" y=\"{SVG_TASK_Y}\" font-size=\"12\" fill=\"#cba6f7\">{task_line}</text>\
         {npc_lines}\
         <text x=\"16\" y=\"{SVG_NARRATIVE_Y}\" font-size=\"11\" fill=\"#bac2de\">{narrative}</text>\
         </svg>",
        loc = xml_escape(&scene.location_name),
        hh = clock.hour,
        mm = clock.minute,
        tod = xml_escape(&clock.time_label),
        dow = xml_escape(&clock.day_of_week),
        daytype = xml_escape(&clock.day_type),
        season = xml_escape(&clock.season),
        weather = xml_escape(&state.weather),
        visited = state.player.visited_count,
        paused = if clock.paused { ", PAUSED" } else { "" },
        here = state.npcs.here.len(),
        total = state.npcs.total,
        introduced = state.npcs.introduced,
        items = state.grapevine.item_count,
        distorted = state.grapevine.distorted_item_count,
        maxd = state.grapevine.max_distortion,
        task_line = xml_escape(&task_line),
        narrative = xml_escape(&narrative_excerpt),
    )
}

/// Build the RGB pixel buffer encoding key facts as colored regions.
fn render_pixels(state: &EngineState) -> Vec<u8> {
    let mut buf = vec![0u8; (W * H * 3) as usize];
    // Background.
    fill_rect(&mut buf, 0, 0, W, H, (30, 30, 46));
    // Header bar colored by location identity.
    let loc_color = color_for(&format!("loc:{}", state.active_scene.location_id));
    fill_rect(&mut buf, 0, 0, W, 40, loc_color);
    // Clock progress bar across the day (minutes since midnight / 1440).
    let minutes = u32::from(state.clock.hour) * 60 + u32::from(state.clock.minute);
    let clock_w = ((minutes as f64 / 1440.0) * f64::from(W)) as u32;
    fill_rect(&mut buf, 0, 44, clock_w.max(1), 14, (137, 220, 235));
    // One cell per co-located NPC, colored by mood.
    let mut x = 8;
    for npc in &state.npcs.here {
        let c = color_for(&format!("mood:{}", npc.mood));
        fill_rect(&mut buf, x, 70, 36, 36, c);
        x += 44;
        if x + 36 > W {
            break;
        }
    }
    // Grapevine intensity bar (distortion level scaled).
    let g_w = (u32::from(state.grapevine.max_distortion) * 40).min(W);
    fill_rect(&mut buf, 0, 120, g_w.max(1), 12, (249, 226, 175));
    // Roster fill bar (introduced / total).
    if state.npcs.total > 0 {
        let frac = state.npcs.introduced as f64 / state.npcs.total as f64;
        let r_w = (frac * f64::from(W)) as u32;
        fill_rect(&mut buf, 0, 140, r_w.max(1), 12, (166, 227, 161));
    }
    // One compact cell per active task, keyed by id + status so a lifecycle
    // transition changes the frame even when location/NPC facts stay static.
    let mut task_x = 8;
    for task in &state.player.active_tasks {
        let color = color_for(&format!("task:{}:{}", task.id, task.status_label()));
        fill_rect(&mut buf, task_x, 164, 28, 18, color);
        task_x += 34;
        if task_x + 28 > W {
            break;
        }
    }
    buf
}

/// A buffer is blank if every pixel is identical (no information rendered).
fn is_blank(pixels: &[u8]) -> bool {
    if pixels.len() < 3 {
        return true;
    }
    let first = &pixels[0..3];
    pixels.chunks_exact(3).all(|px| px == first)
}

/// Encode an RGB buffer as PNG bytes.
fn encode_png(pixels: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, W, H);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| HarnessError::Render(format!("png header: {e}")))?;
        writer
            .write_image_data(pixels)
            .map_err(|e| HarnessError::Render(format!("png data: {e}")))?;
    }
    Ok(out)
}

/// Fill a rectangle in the RGB buffer (clipped to bounds).
fn fill_rect(buf: &mut [u8], x: u32, y: u32, w: u32, h: u32, color: (u8, u8, u8)) {
    for py in y..(y + h).min(H) {
        for px in x..(x + w).min(W) {
            let idx = ((py * W + px) * 3) as usize;
            buf[idx] = color.0;
            buf[idx + 1] = color.1;
            buf[idx + 2] = color.2;
        }
    }
}

/// Deterministic color from a string (fnv-ish hash → RGB, kept bright enough to
/// be visible on the dark background).
fn color_for(s: &str) -> (u8, u8, u8) {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let r = 64 + (hash & 0x7f) as u8;
    let g = 64 + ((hash >> 8) & 0x7f) as u8;
    let b = 64 + ((hash >> 16) & 0x7f) as u8;
    (r, g, b)
}

/// Minimal XML escaping for SVG text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::ipc::build_engine_state;
    use parish_core::ipc::engine_state::{
        ActiveScene, EngineClock, GrapevineStatus, NpcStateSummary, NpcStatus, PlayerState,
    };
    use parish_core::npc::{NpcId, manager::NpcManager};
    use parish_core::world::WorldState;
    use std::io::Cursor;

    fn sample_state() -> EngineState {
        let active_tasks = {
            let mut world = WorldState::new();
            let location = world.player_location;
            let assigned_at = world.clock.now();
            let task_id = world
                .player_progress
                .assign_task("weed the potato patch", NpcId(11), location, assigned_at)
                .unwrap();
            assert_eq!(
                world.player_progress.advance_assigned_task(
                    "I weed the potato patch",
                    location,
                    assigned_at
                ),
                Some(task_id)
            );
            build_engine_state(&world, &NpcManager::new())
                .player
                .active_tasks
        };
        EngineState {
            schema_version: 2,
            active_scene: ActiveScene {
                location_id: 3,
                location_name: "Darcy's Pub".into(),
                indoor: true,
            },
            clock: EngineClock {
                hour: 14,
                minute: 30,
                time_label: "Afternoon".into(),
                day_of_week: "Monday".into(),
                day_type: "Weekday".into(),
                season: "Spring".into(),
                festival: None,
                paused: false,
                inference_paused: false,
            },
            weather: "Light rain".into(),
            player: PlayerState {
                location_id: 3,
                visited_count: 4,
                name: Some("Sean".into()),
                active_tasks,
            },
            npcs: NpcStateSummary {
                here: vec![NpcStatus {
                    id: 1,
                    real_name: "Maggie Byrne".into(),
                    display_name: "Maggie Byrne".into(),
                    mood: "cheerful".into(),
                    introduced: true,
                }],
                total: 12,
                introduced: 3,
            },
            grapevine: GrapevineStatus {
                item_count: 5,
                max_distortion: 2,
                distorted_item_count: 1,
            },
        }
    }

    fn state_with_npcs(count: usize) -> EngineState {
        let mut state = sample_state();
        state.npcs.here = (1..=count)
            .map(|index| NpcStatus {
                id: index as u32,
                real_name: format!("NPC {index}"),
                display_name: format!("NPC {index}"),
                mood: "steady".into(),
                introduced: false,
            })
            .collect();
        state.npcs.total = count;
        state.npcs.introduced = 0;
        state
    }

    fn assert_npc_line(svg: &str, name: &str, y: u32) {
        let expected = format!(
            "<text x=\"16\" y=\"{y}\" font-size=\"12\" fill=\"#cdd6f4\">- {name} (steady)</text>"
        );
        assert!(
            svg.contains(&expected),
            "expected NPC line `{expected}` in SVG: {svg}"
        );
    }

    #[test]
    fn frame_nonblank_produces_decodable_png() {
        let state = sample_state();
        let frame = render(&state, "You step into the smoky pub.", 7).unwrap();
        let repeated = render(&state, "You step into the smoky pub.", 7).unwrap();
        assert!(!frame.png.is_empty());
        assert_eq!(
            frame.png, repeated.png,
            "PNG encoding must be deterministic"
        );
        // PNG magic bytes.
        assert_eq!(&frame.png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        // Fully decodes back to the exact RGB pixels that were rendered.
        let decoder = png::Decoder::new(Cursor::new(frame.png.as_slice()));
        let mut reader = decoder.read_info().unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!(info.width, W);
        assert_eq!(info.height, H);
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        decoded.truncate(info.buffer_size());
        assert_eq!(decoded, render_pixels(&state));
        // SVG carries the readable ground truth.
        assert!(frame.svg.contains("Darcy&#39;s Pub") || frame.svg.contains("Darcy's Pub"));
        assert!(frame.svg.contains("grapevine"));
        assert!(
            frame
                .svg
                .contains("active task: #1 [in_progress] weed the potato patch")
        );
    }

    #[test]
    fn svg_layout_with_no_npcs_retains_task_and_narrative_rows() {
        let svg = render_svg(&state_with_npcs(0), "The room is empty.", 8);

        assert!(svg.contains(
            "<text x=\"16\" y=\"146\" font-size=\"12\" fill=\"#cba6f7\">active task: #1 [in_progress] weed the potato patch</text>"
        ));
        assert!(svg.contains(
            "<text x=\"16\" y=\"224\" font-size=\"11\" fill=\"#bac2de\">The room is empty.</text>"
        ));
        assert!(!svg.contains("fill=\"#cdd6f4\""));
    }

    #[test]
    fn svg_layout_fits_four_npcs_above_narrative() {
        let svg = render_svg(&state_with_npcs(4), "Four neighbours are gathered.", 9);

        assert_npc_line(&svg, "NPC 1", 164);
        assert_npc_line(&svg, "NPC 2", 178);
        assert_npc_line(&svg, "NPC 3", 192);
        assert_npc_line(&svg, "NPC 4", 206);
        assert!(svg.contains(
            "<text x=\"16\" y=\"224\" font-size=\"11\" fill=\"#bac2de\">Four neighbours are gathered.</text>"
        ));
        assert!(!svg.contains("more NPC"));
        assert_eq!(
            SVG_NARRATIVE_Y - (SVG_NPC_FIRST_Y + 3 * SVG_NPC_ROW_STEP),
            18
        );
    }

    #[test]
    fn svg_layout_caps_large_npc_groups_with_summary_row() {
        let svg = render_svg(&state_with_npcs(7), "The gathering fills the room.", 10);

        assert_npc_line(&svg, "NPC 1", 164);
        assert_npc_line(&svg, "NPC 2", 178);
        assert_npc_line(&svg, "NPC 3", 192);
        assert!(svg.contains(
            "<text x=\"16\" y=\"206\" font-size=\"12\" fill=\"#cdd6f4\">… +4 more NPCs</text>"
        ));
        assert!(!svg.contains("- NPC 4 (steady)"));
        assert!(!svg.contains("- NPC 7 (steady)"));
        assert_eq!(svg.matches("fill=\"#cdd6f4\"").count(), 4);
        assert!(svg.contains(
            "<text x=\"16\" y=\"224\" font-size=\"11\" fill=\"#bac2de\">The gathering fills the room.</text>"
        ));
    }

    #[test]
    fn frame_blank_buffer_is_rejected() {
        // An all-zero buffer is blank and must be flagged.
        let blank = vec![0u8; (W * H * 3) as usize];
        assert!(is_blank(&blank));
        // A real render is not blank.
        assert!(!is_blank(&render_pixels(&sample_state())));
    }

    #[test]
    fn frame_color_for_is_deterministic() {
        assert_eq!(color_for("mood:cheerful"), color_for("mood:cheerful"));
        assert_ne!(color_for("mood:cheerful"), color_for("mood:sullen"));
    }
}
