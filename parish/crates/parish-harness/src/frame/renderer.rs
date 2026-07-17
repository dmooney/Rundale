//! State-frame renderer.
//!
//! Each turn the harness renders a "state-frame" from the authoritative
//! [`EngineState`] plus the turn's narrative. Two artifacts come out:
//!
//! - an **SVG** (text) — human/judge-readable: the clock, location, co-located
//!   NPCs with mood, grapevine status, and the narrative. This exposes
//!   simulation data the *player* never sees (NPC moods, grapevine distortion),
//!   which is exactly the ground truth the judge needs.
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
    let mut y = 150;
    for npc in &state.npcs.here {
        npc_lines.push_str(&format!(
            "<text x=\"16\" y=\"{y}\" font-size=\"12\" fill=\"#cdd6f4\">- {} ({}){}</text>",
            xml_escape(&npc.display_name),
            xml_escape(&npc.mood),
            if npc.introduced { " [known]" } else { "" }
        ));
        y += 16;
    }
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
         {npc_lines}\
         <text x=\"16\" y=\"220\" font-size=\"11\" fill=\"#bac2de\">{narrative}</text>\
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
    use parish_core::ipc::engine_state::{
        ActiveScene, EngineClock, GrapevineStatus, NpcStateSummary, NpcStatus, PlayerState,
    };
    use std::io::Cursor;

    fn sample_state() -> EngineState {
        EngineState {
            schema_version: 1,
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
