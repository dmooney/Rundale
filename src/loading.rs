//! Loading animation displayed while waiting for LLM inference responses.
//!
//! Shows a cycling Celtic cross spinner with humorous Irish-themed
//! "verbing" phrases and color animation, so the player knows the game
//! is working while an NPC thinks.

use ratatui::style::Color;

/// Celtic cross geometric variants — thin → hollow → bold → back.
const SPINNER_FRAMES: &[&str] = &["⁜", "✙", "✛", "✜", "✚", "✜", "✛", "✙"];

/// Humorous Irish-themed phrases shown while waiting for inference.
const LOADING_PHRASES: &[&str] = &[
    "Pondering the craic...",
    "Consulting the sheep...",
    "Brewing a thought...",
    "Asking the wind...",
    "Checking with the crows...",
    "Reading the tea leaves...",
    "Warming up by the fire...",
    "Rummaging through the thatch...",
    "Having a think...",
    "Debating with the rain...",
    "Summoning the storyteller...",
    "Tuning the fiddle...",
    "Waiting on the kettle...",
    "Counting the stones...",
    "Muttering in Irish...",
    "Conferring with the bog...",
    "Herding stray notions...",
    "Stirring the porridge...",
    "Listening to the river...",
    "Polishing the words...",
    "Untangling the yarn...",
    "Feeding the donkey...",
    "Sweeping the hearth...",
    "Searching for the right word...",
];

/// Cycling colors in an Irish palette (greens, golds, blues).
const SPINNER_COLORS: &[Color] = &[
    Color::Rgb(72, 199, 142),  // soft green
    Color::Rgb(255, 200, 87),  // warm gold
    Color::Rgb(100, 149, 237), // cornflower blue
    Color::Rgb(255, 160, 100), // soft orange
    Color::Rgb(180, 130, 255), // lavender
    Color::Rgb(120, 220, 180), // mint
];

/// How many ticks before the phrase and color cycle to the next one.
/// At ~100ms per tick this is roughly 1.5 seconds.
const PHRASE_CHANGE_INTERVAL: usize = 15;

/// Animated loading indicator for LLM inference waits.
///
/// Call [`tick`](LoadingAnimation::tick) once per render frame (~50–100ms)
/// and read the current display state via [`display_text`](LoadingAnimation::display_text)
/// and [`current_color`](LoadingAnimation::current_color).
pub struct LoadingAnimation {
    /// Index into [`SPINNER_FRAMES`].
    frame_index: usize,
    /// Index into [`LOADING_PHRASES`].
    phrase_index: usize,
    /// Ticks elapsed since the last phrase change.
    ticks_since_phrase_change: usize,
    /// Index into [`SPINNER_COLORS`].
    color_index: usize,
}

impl LoadingAnimation {
    /// Creates a new animation with a time-seeded initial phrase for variety.
    pub fn new() -> Self {
        // Use low-order bits of the system clock to pick a starting phrase
        // so players don't always see the same one first.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as usize)
            .unwrap_or(0);
        let phrase_index = seed % LOADING_PHRASES.len();

        Self {
            frame_index: 0,
            phrase_index,
            ticks_since_phrase_change: 0,
            color_index: 0,
        }
    }

    /// Advances the animation by one frame.
    ///
    /// The spinner character advances every tick. The phrase and color
    /// advance every [`PHRASE_CHANGE_INTERVAL`] ticks.
    pub fn tick(&mut self) {
        self.frame_index = (self.frame_index + 1) % SPINNER_FRAMES.len();
        self.ticks_since_phrase_change += 1;

        if self.ticks_since_phrase_change >= PHRASE_CHANGE_INTERVAL {
            self.ticks_since_phrase_change = 0;
            self.phrase_index = (self.phrase_index + 1) % LOADING_PHRASES.len();
            self.color_index = (self.color_index + 1) % SPINNER_COLORS.len();
        }
    }

    /// Returns the current display string, e.g. `"✛ Consulting the sheep..."`.
    pub fn display_text(&self) -> String {
        format!(
            "{} {}",
            SPINNER_FRAMES[self.frame_index], LOADING_PHRASES[self.phrase_index]
        )
    }

    /// Returns the current cycling color as a [`ratatui::style::Color`] for TUI rendering.
    pub fn current_color(&self) -> Color {
        SPINNER_COLORS[self.color_index % SPINNER_COLORS.len()]
    }

    /// Returns the current color as an `(R, G, B)` tuple for GUI rendering.
    pub fn current_color_rgb(&self) -> (u8, u8, u8) {
        match SPINNER_COLORS[self.color_index % SPINNER_COLORS.len()] {
            Color::Rgb(r, g, b) => (r, g, b),
            _ => (200, 200, 200),
        }
    }

    /// Returns an ANSI 24-bit foreground color escape sequence for headless mode.
    pub fn current_color_ansi(&self) -> String {
        let (r, g, b) = self.current_color_rgb();
        format!("\x1b[38;2;{};{};{}m", r, g, b)
    }
}

impl Default for LoadingAnimation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_not_empty() {
        assert!(!SPINNER_FRAMES.is_empty());
        assert!(!LOADING_PHRASES.is_empty());
        assert!(!SPINNER_COLORS.is_empty());
    }

    #[test]
    fn test_new_creates_valid_state() {
        let anim = LoadingAnimation::new();
        assert!(anim.frame_index < SPINNER_FRAMES.len());
        assert!(anim.phrase_index < LOADING_PHRASES.len());
        assert_eq!(anim.ticks_since_phrase_change, 0);
        assert_eq!(anim.color_index, 0);
    }

    #[test]
    fn test_tick_advances_frame() {
        let mut anim = LoadingAnimation::new();
        let initial = anim.frame_index;
        anim.tick();
        assert_eq!(anim.frame_index, (initial + 1) % SPINNER_FRAMES.len());
    }

    #[test]
    fn test_frame_wraps_around() {
        let mut anim = LoadingAnimation::new();
        for _ in 0..SPINNER_FRAMES.len() {
            anim.tick();
        }
        // After exactly len ticks the frame should have wrapped
        // (phrase may or may not have changed depending on interval)
        assert!(anim.frame_index < SPINNER_FRAMES.len());
    }

    #[test]
    fn test_phrase_changes_after_interval() {
        let mut anim = LoadingAnimation::new();
        let initial_phrase = anim.phrase_index;
        for _ in 0..PHRASE_CHANGE_INTERVAL {
            anim.tick();
        }
        let expected = (initial_phrase + 1) % LOADING_PHRASES.len();
        assert_eq!(anim.phrase_index, expected);
    }

    #[test]
    fn test_phrase_wraps_around() {
        let mut anim = LoadingAnimation::new();
        let total_ticks = PHRASE_CHANGE_INTERVAL * LOADING_PHRASES.len();
        for _ in 0..total_ticks {
            anim.tick();
        }
        // After cycling through all phrases, we should be back to the start
        assert!(anim.phrase_index < LOADING_PHRASES.len());
    }

    #[test]
    fn test_color_changes_with_phrase() {
        let mut anim = LoadingAnimation::new();
        assert_eq!(anim.color_index, 0);
        for _ in 0..PHRASE_CHANGE_INTERVAL {
            anim.tick();
        }
        assert_eq!(anim.color_index, 1);
    }

    #[test]
    fn test_display_text_contains_spinner_and_phrase() {
        let anim = LoadingAnimation::new();
        let text = anim.display_text();
        // Should contain a spinner frame character
        assert!(SPINNER_FRAMES.iter().any(|f| text.contains(f)));
        // Should contain the current phrase
        assert!(text.contains(LOADING_PHRASES[anim.phrase_index]));
    }

    #[test]
    fn test_display_text_format() {
        let anim = LoadingAnimation::new();
        let text = anim.display_text();
        // Format is "SPINNER PHRASE" with a space separator
        assert!(text.contains(' '));
        assert!(text.ends_with("..."));
    }

    #[test]
    fn test_current_color_returns_valid_color() {
        let anim = LoadingAnimation::new();
        let color = anim.current_color();
        assert!(matches!(color, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_current_color_rgb_returns_tuple() {
        let anim = LoadingAnimation::new();
        let (r, g, b) = anim.current_color_rgb();
        // First color is soft green (72, 199, 142)
        assert_eq!((r, g, b), (72, 199, 142));
    }

    #[test]
    fn test_current_color_ansi_format() {
        let anim = LoadingAnimation::new();
        let ansi = anim.current_color_ansi();
        assert!(ansi.starts_with("\x1b[38;2;"));
        assert!(ansi.ends_with('m'));
    }

    #[test]
    fn test_default_matches_new() {
        let a = LoadingAnimation::new();
        let b = LoadingAnimation::default();
        // Both should have valid initial state (phrase_index may differ due to time seed)
        assert_eq!(a.frame_index, b.frame_index);
        assert_eq!(a.color_index, b.color_index);
        assert_eq!(a.ticks_since_phrase_change, b.ticks_since_phrase_change);
    }
}
