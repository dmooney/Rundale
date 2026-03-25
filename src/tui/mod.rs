//! Terminal UI rendering with Ratatui.
//!
//! Handles terminal setup/teardown, the main render loop,
//! and 24-bit true color palette shifts for time-of-day and weather.

pub mod debug_panel;

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use crate::config::InferenceCategory;
use crate::inference::InferenceQueue;
use crate::inference::openai_client::OpenAiClient;
use crate::loading::LoadingAnimation;
use crate::npc::IrishWordHint;
use crate::npc::manager::NpcManager;
use crate::persistence::AsyncDatabase;
use crate::world::WorldState;
use crate::world::palette::{RawPalette, compute_palette};
use crate::world::time::TimeOfDay;

/// Maximum number of entries in the debug activity log.
pub const DEBUG_LOG_CAPACITY: usize = 50;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::CrosstermBackend;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::io::{self, Stdout};
use std::time::Duration;

/// A color palette for the TUI based on time of day.
///
/// Drives the background, foreground, and accent colors for all
/// TUI elements, creating a visual day/night cycle.
#[derive(Debug, Clone, Copy)]
pub struct ColorPalette {
    /// Background color for all panels.
    pub bg: Color,
    /// Foreground (text) color.
    pub fg: Color,
    /// Accent color for the top bar.
    pub accent: Color,
}

/// Returns the color palette for the given time of day.
///
/// RGB values follow the Phase 1 spec:
/// - Dawn: warm pale yellow (255,220,180)
/// - Morning: warm gold (255,245,220)
/// - Midday: bright warm (255,255,240)
/// - Afternoon: deepening gold (240,220,170)
/// - Dusk: deep blue (60,70,110)
/// - Night: near-black cold (20,25,40)
/// - Midnight: darkest (10,12,20)
pub fn palette_for_time(tod: &TimeOfDay) -> ColorPalette {
    match tod {
        TimeOfDay::Dawn => ColorPalette {
            bg: Color::Rgb(255, 220, 180),
            fg: Color::Rgb(60, 40, 20),
            accent: Color::Rgb(200, 140, 60),
        },
        TimeOfDay::Morning => ColorPalette {
            bg: Color::Rgb(255, 245, 220),
            fg: Color::Rgb(50, 35, 15),
            accent: Color::Rgb(180, 130, 50),
        },
        TimeOfDay::Midday => ColorPalette {
            bg: Color::Rgb(255, 255, 240),
            fg: Color::Rgb(40, 30, 10),
            accent: Color::Rgb(160, 120, 40),
        },
        TimeOfDay::Afternoon => ColorPalette {
            bg: Color::Rgb(240, 220, 170),
            fg: Color::Rgb(50, 35, 15),
            accent: Color::Rgb(180, 130, 50),
        },
        TimeOfDay::Dusk => ColorPalette {
            bg: Color::Rgb(60, 70, 110),
            fg: Color::Rgb(220, 210, 190),
            accent: Color::Rgb(200, 160, 80),
        },
        TimeOfDay::Night => ColorPalette {
            bg: Color::Rgb(20, 25, 40),
            fg: Color::Rgb(180, 180, 190),
            accent: Color::Rgb(100, 110, 140),
        },
        TimeOfDay::Midnight => ColorPalette {
            bg: Color::Rgb(10, 12, 20),
            fg: Color::Rgb(150, 150, 165),
            accent: Color::Rgb(70, 75, 100),
        },
    }
}

impl From<RawPalette> for ColorPalette {
    fn from(raw: RawPalette) -> Self {
        ColorPalette {
            bg: Color::Rgb(raw.bg.r, raw.bg.g, raw.bg.b),
            fg: Color::Rgb(raw.fg.r, raw.fg.g, raw.fg.b),
            accent: Color::Rgb(raw.accent.r, raw.accent.g, raw.accent.b),
        }
    }
}

/// Returns a smoothly interpolated TUI palette for the given time, season, and weather.
///
/// Uses linear interpolation between time-of-day keyframes and applies
/// seasonal and weather color tinting.
pub fn palette_smooth(
    hour: u32,
    minute: u32,
    season: crate::world::time::Season,
    weather: crate::world::Weather,
) -> ColorPalette {
    compute_palette(hour, minute, season, weather).into()
}

/// Scroll state for the main text panel.
///
/// Tracks the scroll offset and whether auto-scroll (follow new output)
/// is active. When the user scrolls up, auto-scroll is disabled until
/// they press End or scroll back to the bottom.
#[derive(Debug, Clone)]
pub struct ScrollState {
    /// Current scroll offset in lines from the top.
    pub offset: u16,
    /// Whether to auto-scroll to the bottom on new content.
    pub auto_scroll: bool,
}

impl ScrollState {
    /// Creates a new scroll state with auto-scroll enabled.
    pub fn new() -> Self {
        Self {
            offset: 0,
            auto_scroll: true,
        }
    }

    /// Scrolls up by the given number of lines.
    pub fn scroll_up(&mut self, lines: u16) {
        self.offset = self.offset.saturating_add(lines);
        self.auto_scroll = false;
    }

    /// Scrolls down by the given number of lines.
    pub fn scroll_down(&mut self, lines: u16, max_offset: u16) {
        self.offset = self.offset.saturating_sub(lines);
        if self.offset == 0 {
            self.auto_scroll = true;
        }
        // Clamp — offset is distance from bottom, so 0 = bottom
        let _ = max_offset; // kept for API clarity; clamping happens in update()
    }

    /// Scrolls to the top of the text log.
    pub fn scroll_to_top(&mut self, max_offset: u16) {
        self.offset = max_offset;
        self.auto_scroll = false;
    }

    /// Scrolls to the bottom and re-enables auto-scroll.
    pub fn scroll_to_bottom(&mut self) {
        self.offset = 0;
        self.auto_scroll = true;
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

/// Main application state for the TUI.
///
/// Holds the game world state, input buffer, scroll state, and control flags.
/// Passed to the `draw` function each frame.
pub struct App {
    /// The game world state.
    pub world: WorldState,
    /// Current text in the input line.
    pub input_buffer: String,
    /// Set to true to exit the main loop.
    pub should_quit: bool,
    /// The inference queue for sending LLM requests (None if unavailable).
    pub inference_queue: Option<InferenceQueue>,
    /// Central NPC manager — owns all NPCs and handles tier assignment.
    pub npc_manager: NpcManager,
    /// Scroll state for the main text panel.
    pub scroll: ScrollState,
    /// Whether the Irish pronunciation sidebar is visible.
    pub sidebar_visible: bool,
    /// Pronunciation hints for Irish words from NPC responses.
    pub pronunciation_hints: Vec<IrishWordHint>,
    /// Whether improv craft mode is enabled for NPC dialogue.
    pub improv_enabled: bool,
    /// Whether the debug sidebar panel is visible.
    pub debug_sidebar_visible: bool,
    /// Rolling activity log for the debug panel.
    pub debug_log: VecDeque<String>,
    /// Counter for rotating idle messages.
    pub idle_counter: usize,
    /// The LLM client for inference requests.
    pub client: Option<OpenAiClient>,
    /// Current model name.
    pub model_name: String,
    /// Display name of the current provider.
    pub provider_name: String,
    /// Base URL for the current provider.
    pub base_url: String,
    /// API key for the current provider.
    pub api_key: Option<String>,
    /// Cloud provider name for dialogue (None = local only).
    pub cloud_provider_name: Option<String>,
    /// Cloud model name for dialogue.
    pub cloud_model_name: Option<String>,
    /// Cloud client for dialogue inference.
    pub cloud_client: Option<OpenAiClient>,
    /// Cloud API key.
    pub cloud_api_key: Option<String>,
    /// Cloud base URL.
    pub cloud_base_url: Option<String>,
    /// The model name used by the dialogue inference queue.
    pub dialogue_model: String,
    /// Loading animation state, active while waiting for LLM inference.
    pub loading_animation: Option<LoadingAnimation>,
    /// Async database handle for persistence (None if persistence is disabled).
    pub db: Option<Arc<AsyncDatabase>>,
    /// Active save branch id.
    pub active_branch_id: i64,
    /// Most recent snapshot id on the active branch.
    pub latest_snapshot_id: i64,
    /// Wall-clock time of the last autosave.
    pub last_autosave: Option<Instant>,
    /// The LLM client for intent parsing (may differ from base client).
    pub intent_client: Option<OpenAiClient>,
    /// The model name for intent parsing.
    pub intent_model: String,
    /// Provider name for intent category (None = inherits base).
    pub intent_provider_name: Option<String>,
    /// API key for intent category.
    pub intent_api_key: Option<String>,
    /// Base URL for intent category.
    pub intent_base_url: Option<String>,
    /// The LLM client for simulation (may differ from base client).
    pub simulation_client: Option<OpenAiClient>,
    /// The model name for simulation.
    pub simulation_model: String,
    /// Provider name for simulation category (None = inherits base).
    pub simulation_provider_name: Option<String>,
    /// API key for simulation category.
    pub simulation_api_key: Option<String>,
    /// Base URL for simulation category.
    pub simulation_base_url: Option<String>,
}

impl App {
    /// Creates a new App with default world state.
    pub fn new() -> Self {
        Self {
            world: WorldState::new(),
            input_buffer: String::new(),
            should_quit: false,
            inference_queue: None,
            npc_manager: NpcManager::new(),
            scroll: ScrollState::new(),
            sidebar_visible: false,
            pronunciation_hints: Vec::new(),
            improv_enabled: false,
            debug_sidebar_visible: false,
            debug_log: VecDeque::with_capacity(DEBUG_LOG_CAPACITY),
            idle_counter: 0,
            client: None,
            model_name: String::new(),
            provider_name: String::from("ollama"),
            base_url: String::new(),
            api_key: None,
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_client: None,
            cloud_api_key: None,
            cloud_base_url: None,
            dialogue_model: String::new(),
            loading_animation: None,
            db: None,
            active_branch_id: 1,
            latest_snapshot_id: 0,
            last_autosave: None,
            intent_client: None,
            intent_model: String::new(),
            intent_provider_name: None,
            intent_api_key: None,
            intent_base_url: None,
            simulation_client: None,
            simulation_model: String::new(),
            simulation_provider_name: None,
            simulation_api_key: None,
            simulation_base_url: None,
        }
    }

    /// Returns the provider name for a given inference category (or None if inheriting base).
    pub fn category_provider_name(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name.as_deref(),
            InferenceCategory::Simulation => self.simulation_provider_name.as_deref(),
            InferenceCategory::Intent => self.intent_provider_name.as_deref(),
        }
    }

    /// Returns the model name for a given inference category (empty string if inheriting base).
    pub fn category_model(&self, cat: InferenceCategory) -> &str {
        match cat {
            InferenceCategory::Dialogue => self.cloud_model_name.as_deref().unwrap_or(""),
            InferenceCategory::Simulation => &self.simulation_model,
            InferenceCategory::Intent => &self.intent_model,
        }
    }

    /// Returns the API key for a given inference category.
    pub fn category_api_key(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key.as_deref(),
            InferenceCategory::Simulation => self.simulation_api_key.as_deref(),
            InferenceCategory::Intent => self.intent_api_key.as_deref(),
        }
    }

    /// Returns the base URL for a given inference category.
    pub fn category_base_url(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url.as_deref(),
            InferenceCategory::Simulation => self.simulation_base_url.as_deref(),
            InferenceCategory::Intent => self.intent_base_url.as_deref(),
        }
    }

    /// Returns the client for a given inference category.
    pub fn category_client(&self, cat: InferenceCategory) -> Option<&OpenAiClient> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client.as_ref(),
            InferenceCategory::Simulation => self.simulation_client.as_ref(),
            InferenceCategory::Intent => self.intent_client.as_ref(),
        }
    }

    /// Sets the provider name for a given inference category.
    pub fn set_category_provider_name(&mut self, cat: InferenceCategory, name: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name = Some(name),
            InferenceCategory::Simulation => self.simulation_provider_name = Some(name),
            InferenceCategory::Intent => self.intent_provider_name = Some(name),
        }
    }

    /// Sets the model name for a given inference category.
    pub fn set_category_model(&mut self, cat: InferenceCategory, model: String) {
        match cat {
            InferenceCategory::Dialogue => {
                self.cloud_model_name = Some(model.clone());
                self.dialogue_model = model;
            }
            InferenceCategory::Simulation => self.simulation_model = model,
            InferenceCategory::Intent => self.intent_model = model,
        }
    }

    /// Sets the API key for a given inference category.
    pub fn set_category_api_key(&mut self, cat: InferenceCategory, key: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key = Some(key),
            InferenceCategory::Simulation => self.simulation_api_key = Some(key),
            InferenceCategory::Intent => self.intent_api_key = Some(key),
        }
    }

    /// Sets the base URL for a given inference category.
    pub fn set_category_base_url(&mut self, cat: InferenceCategory, url: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url = Some(url),
            InferenceCategory::Simulation => self.simulation_base_url = Some(url),
            InferenceCategory::Intent => self.intent_base_url = Some(url),
        }
    }

    /// Sets the client for a given inference category.
    pub fn set_category_client(&mut self, cat: InferenceCategory, client: OpenAiClient) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client = Some(client),
            InferenceCategory::Simulation => self.simulation_client = Some(client),
            InferenceCategory::Intent => self.intent_client = Some(client),
        }
    }

    /// Pushes an entry to the debug activity log (ring buffer).
    pub fn debug_event(&mut self, msg: String) {
        if self.debug_log.len() >= DEBUG_LOG_CAPACITY {
            self.debug_log.pop_front();
        }
        self.debug_log.push_back(msg);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Initializes the terminal for TUI rendering.
///
/// Enables raw mode, enters the alternate screen, and installs a
/// panic hook that restores the terminal before printing the panic.
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    // Install panic hook that restores terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    Terminal::new(backend)
}

/// Restores the terminal to its normal state.
///
/// Disables raw mode and leaves the alternate screen. Should be
/// called on both normal exit and error paths.
pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Draws one frame of the TUI.
///
/// Layout: top bar (3 lines with border), main text panel (fill), input line (3 lines with border).
/// Colors are driven by the smoothly interpolated time-of-day palette.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let now = app.world.clock.now();
    let palette = palette_smooth(
        chrono::Timelike::hour(&now),
        chrono::Timelike::minute(&now),
        app.world.clock.season(),
        app.world.weather,
    );
    let base_style = Style::default().fg(palette.fg).bg(palette.bg);
    let accent_style = Style::default().fg(palette.accent).bg(palette.bg);

    let chunks = Layout::vertical([
        Constraint::Length(3), // top bar
        Constraint::Min(1),    // main panel
        Constraint::Length(3), // input line
    ])
    .split(frame.area());

    // Top bar: location | time | weather | season
    let location = app.world.current_location();
    let time_of_day = app.world.clock.time_of_day();
    let season = app.world.clock.season();
    let festival_text = app
        .world
        .clock
        .check_festival()
        .map(|f| format!(" | {}", f))
        .unwrap_or_default();

    let top_text = format!(
        "{} | {} | {} | {}{}",
        location.name, time_of_day, app.world.weather, season, festival_text
    );
    let top_bar = Paragraph::new(Line::from(top_text))
        .style(accent_style)
        .block(Block::default().borders(Borders::BOTTOM).style(base_style));
    frame.render_widget(top_bar, chunks[0]);

    // Split main area horizontally if any sidebar is visible
    let show_sidebar = app.sidebar_visible || app.debug_sidebar_visible;
    let (main_area, sidebar_area) = if show_sidebar {
        let h_chunks = Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[1]);
        (h_chunks[0], Some(h_chunks[1]))
    } else {
        (chunks[1], None)
    };

    // Main panel: text log with word wrap and scroll support
    let panel_height = main_area.height;
    let panel_width = main_area.width;

    let mut log_lines: Vec<Line> = app
        .world
        .text_log
        .iter()
        .map(|s| Line::from(s.as_str()))
        .collect();

    // Append loading animation to the last line while waiting for inference
    if let Some(anim) = &app.loading_animation {
        let anim_span = ratatui::text::Span::styled(
            anim.display_text(),
            Style::default().fg(anim.current_color()),
        );
        if let Some(last) = log_lines.last_mut() {
            last.spans.push(anim_span);
        }
    }

    // Count wrapped lines to get accurate scroll math
    let total_lines = if panel_width > 0 {
        app.world
            .text_log
            .iter()
            .map(|s| {
                if s.is_empty() {
                    1u16
                } else {
                    // Ceiling division: how many visual lines does this text occupy?
                    ((s.len() as u16).saturating_sub(1) / panel_width) + 1
                }
            })
            .sum::<u16>()
    } else {
        app.world.text_log.len() as u16
    };

    let max_scroll = total_lines.saturating_sub(panel_height);

    // Compute scroll position — offset is distance from bottom
    let scroll_row = if app.scroll.auto_scroll {
        max_scroll
    } else {
        max_scroll.saturating_sub(app.scroll.offset)
    };

    let scroll_indicator = if total_lines > panel_height && !app.scroll.auto_scroll {
        if scroll_row == 0 {
            "[TOP]".to_string()
        } else {
            let pct = if max_scroll > 0 {
                (scroll_row as f32 / max_scroll as f32 * 100.0) as u16
            } else {
                100
            };
            format!("[{}%]", pct.min(100))
        }
    } else {
        String::new()
    };

    let block_title = if scroll_indicator.is_empty() {
        Block::default().style(base_style)
    } else {
        Block::default()
            .title_top(Line::from(scroll_indicator).right_aligned())
            .style(base_style)
    };

    let main_panel = Paragraph::new(Text::from(log_lines))
        .style(base_style)
        .wrap(Wrap { trim: false })
        .scroll((scroll_row, 0))
        .block(block_title);
    frame.render_widget(main_panel, main_area);

    // Sidebar: debug panel takes priority, then Irish pronunciation
    if let Some(sidebar) = sidebar_area {
        if app.debug_sidebar_visible {
            draw_debug_sidebar(frame, app, sidebar, &base_style, &accent_style);
        } else {
            draw_pronunciation_sidebar(frame, app, sidebar, &base_style, &accent_style);
        }
    }

    // Input line
    let input_text = format!("> {}", app.input_buffer);
    let input_line = Paragraph::new(Line::from(input_text))
        .style(base_style)
        .block(Block::default().borders(Borders::TOP).style(base_style));
    frame.render_widget(input_line, chunks[2]);
}

/// Polls for and handles a single keyboard event.
///
/// Updates the app's input buffer as needed.
/// When Enter is pressed, returns the submitted input text.
/// Esc clears the input line.
pub fn handle_input(app: &mut App, timeout: Duration) -> io::Result<Option<String>> {
    if event::poll(timeout)?
        && let Event::Key(key) = event::read()?
    {
        if key.kind != KeyEventKind::Press {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char(c) => {
                app.input_buffer.push(c);
            }
            KeyCode::Backspace => {
                app.input_buffer.pop();
            }
            KeyCode::Enter => {
                if !app.input_buffer.is_empty() {
                    let input = app.input_buffer.drain(..).collect();
                    app.scroll.scroll_to_bottom();
                    return Ok(Some(input));
                }
            }
            KeyCode::Esc => {
                app.input_buffer.clear();
            }
            KeyCode::PageUp => {
                app.scroll.scroll_up(10);
            }
            KeyCode::PageDown => {
                let max = app.world.text_log.len() as u16;
                app.scroll.scroll_down(10, max);
            }
            KeyCode::Up => {
                app.scroll.scroll_up(1);
            }
            KeyCode::Down => {
                let max = app.world.text_log.len() as u16;
                app.scroll.scroll_down(1, max);
            }
            KeyCode::Home => {
                let max = app.world.text_log.len() as u16;
                app.scroll.scroll_to_top(max);
            }
            KeyCode::End => {
                app.scroll.scroll_to_bottom();
            }
            KeyCode::Tab => {
                app.sidebar_visible = !app.sidebar_visible;
            }
            KeyCode::F(12) => {
                app.debug_sidebar_visible = !app.debug_sidebar_visible;
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Draws the Irish pronunciation sidebar panel.
///
/// Shows recent Irish words from NPC dialogue with phonetic pronunciation
/// guides and English meanings. Toggled via Tab or `/irish`.
fn draw_pronunciation_sidebar(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    base_style: &Style,
    accent_style: &Style,
) {
    let mut lines: Vec<Line> = Vec::new();

    if app.pronunciation_hints.is_empty() {
        lines.push(Line::from("No Irish words yet.").style(*base_style));
        lines.push(Line::from(""));
        lines.push(Line::from("Chat with the locals").style(*base_style));
        lines.push(Line::from("and words will appear").style(*base_style));
        lines.push(Line::from("here with their").style(*base_style));
        lines.push(Line::from("pronunciation.").style(*base_style));
    } else {
        for hint in &app.pronunciation_hints {
            lines.push(Line::from(hint.word.as_str()).style(*accent_style));
            lines.push(Line::from(format!("  {}", hint.pronunciation)).style(*base_style));
            if let Some(meaning) = &hint.meaning {
                lines.push(Line::from(format!("  {}", meaning)).style(*base_style));
            }
            lines.push(Line::from(""));
        }
    }

    let sidebar = Paragraph::new(Text::from(lines))
        .style(*base_style)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .title(" Focail — Words ")
                .style(*base_style),
        );
    frame.render_widget(sidebar, area);
}

/// Draws the debug sidebar panel.
///
/// Shows live NPC state grouped by cognitive tier, with the game clock
/// at the top. Toggled via F12 or `/debug panel`.
fn draw_debug_sidebar(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    base_style: &Style,
    accent_style: &Style,
) {
    let lines = debug_panel::build_debug_lines(app, *accent_style, *base_style);

    let sidebar = Paragraph::new(Text::from(lines))
        .style(*base_style)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .title(" Debug ")
                .style(*base_style),
        );
    frame.render_widget(sidebar, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_dawn() {
        let p = palette_for_time(&TimeOfDay::Dawn);
        assert_eq!(p.bg, Color::Rgb(255, 220, 180));
    }

    #[test]
    fn test_palette_morning() {
        let p = palette_for_time(&TimeOfDay::Morning);
        assert_eq!(p.bg, Color::Rgb(255, 245, 220));
    }

    #[test]
    fn test_palette_midday() {
        let p = palette_for_time(&TimeOfDay::Midday);
        assert_eq!(p.bg, Color::Rgb(255, 255, 240));
    }

    #[test]
    fn test_palette_afternoon() {
        let p = palette_for_time(&TimeOfDay::Afternoon);
        assert_eq!(p.bg, Color::Rgb(240, 220, 170));
    }

    #[test]
    fn test_palette_dusk() {
        let p = palette_for_time(&TimeOfDay::Dusk);
        assert_eq!(p.bg, Color::Rgb(60, 70, 110));
        // Dusk has light text on dark background
        assert_eq!(p.fg, Color::Rgb(220, 210, 190));
    }

    #[test]
    fn test_palette_night() {
        let p = palette_for_time(&TimeOfDay::Night);
        assert_eq!(p.bg, Color::Rgb(20, 25, 40));
    }

    #[test]
    fn test_palette_midnight() {
        let p = palette_for_time(&TimeOfDay::Midnight);
        assert_eq!(p.bg, Color::Rgb(10, 12, 20));
    }

    #[test]
    fn test_app_new() {
        let app = App::new();
        assert!(!app.should_quit);
        assert!(app.input_buffer.is_empty());
        assert!(app.inference_queue.is_none());
        assert_eq!(app.npc_manager.npc_count(), 0);
        assert!(app.scroll.auto_scroll);
        assert_eq!(app.scroll.offset, 0);
        assert!(!app.sidebar_visible);
        assert!(!app.improv_enabled);
        assert!(app.pronunciation_hints.is_empty());
        assert_eq!(app.idle_counter, 0);
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert!(!app.should_quit);
        assert!(!app.sidebar_visible);
    }

    #[test]
    fn test_sidebar_toggle() {
        let mut app = App::new();
        assert!(!app.sidebar_visible);
        app.sidebar_visible = !app.sidebar_visible;
        assert!(app.sidebar_visible);
        app.sidebar_visible = !app.sidebar_visible;
        assert!(!app.sidebar_visible);
    }

    #[test]
    fn test_improv_toggle() {
        let mut app = App::new();
        assert!(!app.improv_enabled);
        app.improv_enabled = !app.improv_enabled;
        assert!(app.improv_enabled);
        app.improv_enabled = !app.improv_enabled;
        assert!(!app.improv_enabled);
    }

    #[test]
    fn test_pronunciation_hints_storage() {
        use crate::npc::IrishWordHint;
        let mut app = App::new();
        let hint = IrishWordHint {
            word: "sláinte".to_string(),
            pronunciation: "SLAWN-cha".to_string(),
            meaning: Some("Health/cheers".to_string()),
        };
        app.pronunciation_hints.push(hint.clone());
        assert_eq!(app.pronunciation_hints.len(), 1);
        assert_eq!(app.pronunciation_hints[0].word, "sláinte");
    }

    #[test]
    fn test_pronunciation_hints_truncation() {
        use crate::npc::IrishWordHint;
        let mut app = App::new();
        for i in 0..25 {
            app.pronunciation_hints.push(IrishWordHint {
                word: format!("word_{}", i),
                pronunciation: format!("pron_{}", i),
                meaning: None,
            });
        }
        app.pronunciation_hints.truncate(20);
        assert_eq!(app.pronunciation_hints.len(), 20);
    }

    #[test]
    fn test_scroll_state_new() {
        let scroll = ScrollState::new();
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_up_disables_auto() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(5);
        assert_eq!(scroll.offset, 5);
        assert!(!scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_down_reenables_auto_at_bottom() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(3);
        assert!(!scroll.auto_scroll);

        scroll.scroll_down(3, 100);
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_down_partial() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(10);
        scroll.scroll_down(3, 100);
        assert_eq!(scroll.offset, 7);
        assert!(!scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_down_clamps_at_zero() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(2);
        scroll.scroll_down(10, 100);
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_to_top() {
        let mut scroll = ScrollState::new();
        scroll.scroll_to_top(50);
        assert_eq!(scroll.offset, 50);
        assert!(!scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(20);
        scroll.scroll_to_bottom();
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }
}
