//! Terminal UI rendering with Ratatui.
//!
//! Handles terminal setup/teardown, the main render loop,
//! and 24-bit true color palette shifts for time-of-day and weather.

use crate::inference::InferenceQueue;
use crate::npc::Npc;
use crate::world::WorldState;
use crate::world::time::TimeOfDay;

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

/// Main application state for the TUI.
///
/// Holds the game world state, input buffer, and control flags.
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
    /// NPCs present in the world.
    pub npcs: Vec<Npc>,
}

impl App {
    /// Creates a new App with default world state.
    pub fn new() -> Self {
        Self {
            world: WorldState::new(),
            input_buffer: String::new(),
            should_quit: false,
            inference_queue: None,
            npcs: Vec::new(),
        }
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
/// Colors are driven by the current time-of-day palette.
pub fn draw(frame: &mut Frame, app: &App) {
    let palette = palette_for_time(&app.world.clock.time_of_day());
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

    // Main panel: text log with word wrap
    let log_lines: Vec<Line> = app
        .world
        .text_log
        .iter()
        .map(|s| Line::from(s.as_str()))
        .collect();
    let main_panel = Paragraph::new(Text::from(log_lines))
        .style(base_style)
        .wrap(Wrap { trim: false })
        .block(Block::default().style(base_style));
    frame.render_widget(main_panel, chunks[1]);

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
                    return Ok(Some(input));
                }
            }
            KeyCode::Esc => {
                app.input_buffer.clear();
            }
            _ => {}
        }
    }
    Ok(None)
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
        assert!(app.npcs.is_empty());
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert!(!app.should_quit);
    }
}
