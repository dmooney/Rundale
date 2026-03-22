//! Screenshot capture for the GUI.
//!
//! Provides automated screenshot generation via `--screenshot`. Renders
//! the GUI at multiple times of day and saves PNG files to a specified
//! directory. Designed to run under `xvfb-run` for headless capture.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::npc::IrishWordHint;
use crate::world::WorldState;
use crate::world::time::TimeOfDay;

/// Times of day to capture screenshots for.
const SCREENSHOT_TIMES: &[(TimeOfDay, &str, u32)] = &[
    (TimeOfDay::Morning, "morning", 8),
    (TimeOfDay::Midday, "midday", 12),
    (TimeOfDay::Dusk, "dusk", 17),
    (TimeOfDay::Night, "night", 21),
];

/// Configuration for screenshot capture mode.
#[derive(Debug)]
pub struct ScreenshotConfig {
    /// Output directory for screenshots.
    pub output_dir: PathBuf,
    /// Current frame counter (screenshots are requested after a few frames).
    pub frame_count: u32,
    /// Index into SCREENSHOT_TIMES for the current capture.
    pub current_index: usize,
    /// Whether a screenshot request is pending.
    pub pending_request: bool,
    /// Whether the current screenshot has been saved.
    pub saved: bool,
}

impl ScreenshotConfig {
    /// Creates a new screenshot config targeting the given output directory.
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            frame_count: 0,
            current_index: 0,
            pending_request: false,
            saved: false,
        }
    }

    /// Returns the filename for the current screenshot.
    pub fn current_filename(&self) -> String {
        if self.current_index < SCREENSHOT_TIMES.len() {
            format!("gui-{}.png", SCREENSHOT_TIMES[self.current_index].1)
        } else {
            "gui-unknown.png".to_string()
        }
    }

    /// Returns the current time-of-day to capture.
    pub fn current_time(&self) -> Option<&(TimeOfDay, &str, u32)> {
        SCREENSHOT_TIMES.get(self.current_index)
    }

    /// Returns whether all screenshots have been captured.
    pub fn is_done(&self) -> bool {
        self.current_index >= SCREENSHOT_TIMES.len()
    }
}

/// Populates the text log with representative sample content for screenshots.
///
/// Adds location descriptions, NPC dialogue, and movement narrations to
/// make the screenshots look populated and representative of gameplay.
pub fn populate_sample_content(world: &mut WorldState) -> Vec<IrishWordHint> {
    world.text_log.clear();

    world.log("— Kilteevan Village —".to_string());
    world.log(
        "The small village of Kilteevan — a handful of whitewashed cottages \
         clustered around a well and an old stone bridge over a shallow stream. \
         Smoke drifts from chimneys."
            .to_string(),
    );
    world.log("Seán Darcy is here.".to_string());
    world.log(
        "Exits: The Crossroads (north), Murphy's Farm (east), Lough Ree Shore (south)".to_string(),
    );
    world.log(String::new());

    world.log("> hello Seán".to_string());
    world.log(
        "Seán Darcy: (Looks up from sweeping the step) Dia dhuit, a chara! \
         A fine soft morning, so it is. Will ye have a cup of tae? The kettle's \
         only just boiled."
            .to_string(),
    );
    world.log(String::new());

    world.log("> tell me about the village".to_string());
    world.log(
        "Seán Darcy: (Leans on the broom) Ah, Cill Tíobháin — sure it's been \
         here since before anyone can remember. The church above was built in \
         the time of the Penal Laws, would ye believe. And that well there — \
         they say Brigid herself blessed it."
            .to_string(),
    );
    world.log(String::new());

    world.log("> go to crossroads".to_string());
    world.log(
        "You walk along the road north past low fields to the crossroads. (6 minutes)".to_string(),
    );
    world.log(String::new());

    world.log("— The Crossroads —".to_string());
    world.log(
        "A quiet crossroads where four narrow roads meet. A weathered stone wall \
         lines the eastern side, half-hidden by brambles. The air smells of turf \
         and wet grass."
            .to_string(),
    );
    world.log("Padraig O'Brien is here.".to_string());
    world.log(String::new());

    // Return sample Irish word hints
    vec![
        IrishWordHint {
            word: "Dia dhuit".to_string(),
            pronunciation: "DEE-ah gwit".to_string(),
            meaning: Some("Hello (lit. God to you)".to_string()),
        },
        IrishWordHint {
            word: "a chara".to_string(),
            pronunciation: "ah KHAR-ah".to_string(),
            meaning: Some("friend".to_string()),
        },
        IrishWordHint {
            word: "tae".to_string(),
            pronunciation: "tay".to_string(),
            meaning: Some("tea".to_string()),
        },
        IrishWordHint {
            word: "Cill Tíobháin".to_string(),
            pronunciation: "kill tee-VAWN".to_string(),
            meaning: Some("Kilteevan (church of St. Tíobhán)".to_string()),
        },
    ]
}

/// Handles screenshot capture logic for a single frame.
///
/// Called from `GuiApp::update()` when screenshot mode is active. Manages
/// the multi-frame capture sequence: wait for rendering to settle, request
/// screenshot, receive image data, save PNG, advance to next time of day.
pub fn handle_screenshot_frame(
    ctx: &egui::Context,
    config: &mut ScreenshotConfig,
    world: &mut WorldState,
    should_quit: &mut bool,
) {
    config.frame_count += 1;

    // Check if all screenshots are done
    if config.is_done() {
        *should_quit = true;
        return;
    }

    // Set game clock to the target time on the first frame for this capture
    if config.frame_count == 1 || config.saved {
        if config.saved {
            config.current_index += 1;
            config.pending_request = false;
            config.saved = false;
            config.frame_count = 1;

            if config.is_done() {
                *should_quit = true;
                return;
            }
        }

        // Advance clock to target hour
        if let Some((_tod, _name, hour)) = config.current_time() {
            let target_hour = *hour as i64;
            let current_hour = world
                .clock
                .now()
                .format("%H")
                .to_string()
                .parse::<i64>()
                .unwrap_or(8);
            let advance = if target_hour >= current_hour {
                (target_hour - current_hour) * 60
            } else {
                (24 - current_hour + target_hour) * 60
            };
            if advance > 0 {
                world.clock.advance(advance);
            }
        }
    }

    // Wait a few frames for the UI to settle, then request screenshot
    // Frame 3 = request, frame 4+ = check for result
    if config.frame_count == 3 && !config.pending_request {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        config.pending_request = true;
        ctx.request_repaint();
        return;
    }

    // Check for screenshot result in events
    if config.pending_request {
        let mut captured_image = None;
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Screenshot { image, .. } = event {
                    captured_image = Some(image.clone());
                }
            }
        });

        if let Some(image) = captured_image {
            let filename = config.current_filename();
            let path = config.output_dir.join(&filename);
            match save_screenshot(&image, &path) {
                Ok(()) => {
                    tracing::info!("Saved screenshot: {}", path.display());
                    eprintln!("Saved screenshot: {}", path.display());
                }
                Err(e) => {
                    tracing::error!("Failed to save screenshot {}: {}", path.display(), e);
                    eprintln!("Failed to save screenshot {}: {}", path.display(), e);
                }
            }
            config.saved = true;
            ctx.request_repaint();
        } else {
            // Keep requesting repaint until we get the screenshot
            ctx.request_repaint();
        }
    }
}

/// Saves an egui `ColorImage` as a PNG file.
///
/// Converts the RGBA pixel data to an `image::RgbaImage` and encodes
/// as PNG to the specified path.
pub fn save_screenshot(image: &egui::ColorImage, path: &Path) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }

    let pixels: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|c| [c.r(), c.g(), c.b(), c.a()])
        .collect();

    let img = image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels)
        .ok_or_else(|| "failed to create image buffer".to_string())?;

    img.save(path).map_err(|e| format!("save PNG: {e}"))?;

    Ok(())
}

/// Launches the GUI in screenshot mode.
///
/// Opens the window, renders at multiple times of day, captures PNG
/// screenshots, and exits. Intended to be run via `xvfb-run` for
/// headless capture. No LLM client needed.
pub fn run_screenshots(output_dir: &Path) -> Result<(), anyhow::Error> {
    let rt_handle = tokio::runtime::Handle::current();

    let mut app = super::GuiApp::new(rt_handle);

    // Load world data
    let parish_path = Path::new("data/parish.json");
    if parish_path.exists() {
        match WorldState::from_parish_file(parish_path, crate::world::LocationId(15)) {
            Ok(world) => app.world = world,
            Err(e) => tracing::warn!("Failed to load parish data: {}", e),
        }
    }

    // Load NPCs
    let npcs_path = Path::new("data/npcs.json");
    if npcs_path.exists() {
        match crate::npc::manager::NpcManager::load_from_file(npcs_path) {
            Ok(mgr) => app.npc_manager = mgr,
            Err(e) => tracing::warn!("Failed to load NPC data: {}", e),
        }
    }

    // Populate with sample content and Irish word hints
    app.pronunciation_hints = populate_sample_content(&mut app.world);

    // Pause clock so it doesn't drift during capture
    app.world.clock.pause();

    // Configure screenshot mode
    app.screenshot_config = Some(ScreenshotConfig::new(output_dir.to_path_buf()));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Parish — Screenshot Capture")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([1200.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Parish Screenshots",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screenshot_config_new() {
        let config = ScreenshotConfig::new(PathBuf::from("/tmp/screenshots"));
        assert_eq!(config.frame_count, 0);
        assert_eq!(config.current_index, 0);
        assert!(!config.pending_request);
        assert!(!config.saved);
        assert!(!config.is_done());
    }

    #[test]
    fn test_screenshot_config_filename() {
        let config = ScreenshotConfig::new(PathBuf::from("/tmp"));
        assert_eq!(config.current_filename(), "gui-morning.png");
    }

    #[test]
    fn test_screenshot_config_done() {
        let mut config = ScreenshotConfig::new(PathBuf::from("/tmp"));
        assert!(!config.is_done());
        config.current_index = SCREENSHOT_TIMES.len();
        assert!(config.is_done());
    }

    #[test]
    fn test_screenshot_times_coverage() {
        assert_eq!(SCREENSHOT_TIMES.len(), 4);
        // Verify all have valid hours
        for (_tod, name, hour) in SCREENSHOT_TIMES {
            assert!(*hour < 24, "invalid hour for {name}");
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_populate_sample_content() {
        let mut world = WorldState::new();
        let hints = populate_sample_content(&mut world);
        assert!(!world.text_log.is_empty(), "text_log should be populated");
        assert!(!hints.is_empty(), "hints should be populated");
        // Verify Irish words have all fields
        for hint in &hints {
            assert!(!hint.word.is_empty());
            assert!(!hint.pronunciation.is_empty());
            assert!(hint.meaning.is_some());
        }
    }

    #[test]
    fn test_populate_sample_content_has_dialogue() {
        let mut world = WorldState::new();
        populate_sample_content(&mut world);
        let log_text = world.text_log.join("\n");
        assert!(
            log_text.contains("Seán Darcy"),
            "should contain NPC dialogue"
        );
        assert!(log_text.contains("Dia dhuit"), "should contain Irish words");
        assert!(
            log_text.contains("Crossroads"),
            "should contain location names"
        );
    }
}
