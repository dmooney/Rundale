//! Loading animation displayed while waiting for LLM inference responses.
//!
//! Shows a cycling Celtic cross spinner with humorous Irish-themed
//! "verbing" phrases and color animation, so the player knows the game
//! is working while an NPC thinks.

use std::time::{Duration, Instant};

/// Celtic cross geometric variants — thin → hollow → bold → back.
const SPINNER_FRAMES: &[&str] = &["✢", "✙", "✛", "✜", "✚", "✜", "✛", "✙"];

/// How long each spinner frame is displayed before advancing.
const SPINNER_FRAME_DURATION: Duration = Duration::from_millis(300);

/// How long each loading phrase is displayed before cycling.
const PHRASE_DURATION: Duration = Duration::from_millis(3000);

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

/// Cycling colors in an Irish palette (greens, golds, blues) as `(R, G, B)` tuples.
const SPINNER_COLORS: &[(u8, u8, u8)] = &[
    (72, 199, 142),  // soft green
    (255, 200, 87),  // warm gold
    (100, 149, 237), // cornflower blue
    (255, 160, 100), // soft orange
    (180, 130, 255), // lavender
    (120, 220, 180), // mint
];

/// Animated loading indicator for LLM inference waits.
///
/// Uses wall-clock time so the animation speed is consistent regardless
/// of render frame rate. Call [`tick`](LoadingAnimation::tick) each frame
/// and read the current display state via [`display_text`](LoadingAnimation::display_text)
/// and [`current_color`](LoadingAnimation::current_color).
pub struct LoadingAnimation {
    /// Index into [`SPINNER_FRAMES`].
    frame_index: usize,
    /// Index into [`LOADING_PHRASES`].
    phrase_index: usize,
    /// Index into [`SPINNER_COLORS`].
    color_index: usize,
    /// When the current spinner frame started.
    last_frame_change: Instant,
    /// When the current phrase started.
    last_phrase_change: Instant,
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
        let now = Instant::now();

        Self {
            frame_index: 0,
            phrase_index,
            color_index: 0,
            last_frame_change: now,
            last_phrase_change: now,
        }
    }

    /// Advances the animation based on elapsed wall-clock time.
    ///
    /// The spinner character advances every [`SPINNER_FRAME_DURATION`].
    /// The phrase and color advance every [`PHRASE_DURATION`].
    pub fn tick(&mut self) {
        let now = Instant::now();

        if now.duration_since(self.last_frame_change) >= SPINNER_FRAME_DURATION {
            self.frame_index = (self.frame_index + 1) % SPINNER_FRAMES.len();
            self.last_frame_change = now;
        }

        if now.duration_since(self.last_phrase_change) >= PHRASE_DURATION {
            self.phrase_index = (self.phrase_index + 1) % LOADING_PHRASES.len();
            self.color_index = (self.color_index + 1) % SPINNER_COLORS.len();
            self.last_phrase_change = now;
        }
    }

    /// Returns the current display string, e.g. `"✛ Consulting the sheep..."`.
    pub fn display_text(&self) -> String {
        format!(
            "{} {}",
            SPINNER_FRAMES[self.frame_index], LOADING_PHRASES[self.phrase_index]
        )
    }

    /// Returns the current spinner character, e.g. `"✛"`.
    pub fn spinner_char(&self) -> &'static str {
        SPINNER_FRAMES[self.frame_index]
    }

    /// Returns the current loading phrase, e.g. `"Consulting the sheep..."`.
    pub fn phrase(&self) -> &'static str {
        LOADING_PHRASES[self.phrase_index]
    }

    /// Returns the current cycling color as an `(R, G, B)` tuple.
    pub fn current_color_rgb(&self) -> (u8, u8, u8) {
        SPINNER_COLORS[self.color_index % SPINNER_COLORS.len()]
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
        assert_eq!(anim.color_index, 0);
    }

    #[test]
    fn test_tick_no_change_when_called_immediately() {
        let mut anim = LoadingAnimation::new();
        let initial_frame = anim.frame_index;
        let initial_phrase = anim.phrase_index;
        // Tick immediately — not enough time has passed
        anim.tick();
        assert_eq!(anim.frame_index, initial_frame);
        assert_eq!(anim.phrase_index, initial_phrase);
    }

    #[test]
    fn test_frame_advances_after_duration() {
        let mut anim = LoadingAnimation::new();
        let initial_frame = anim.frame_index;
        // Simulate time passing by backdating the last change
        anim.last_frame_change = Instant::now() - SPINNER_FRAME_DURATION;
        anim.tick();
        assert_eq!(anim.frame_index, (initial_frame + 1) % SPINNER_FRAMES.len());
    }

    #[test]
    fn test_phrase_advances_after_duration() {
        let mut anim = LoadingAnimation::new();
        let initial_phrase = anim.phrase_index;
        // Simulate time passing
        anim.last_phrase_change = Instant::now() - PHRASE_DURATION;
        anim.tick();
        let expected = (initial_phrase + 1) % LOADING_PHRASES.len();
        assert_eq!(anim.phrase_index, expected);
    }

    #[test]
    fn test_color_changes_with_phrase() {
        let mut anim = LoadingAnimation::new();
        assert_eq!(anim.color_index, 0);
        anim.last_phrase_change = Instant::now() - PHRASE_DURATION;
        anim.tick();
        assert_eq!(anim.color_index, 1);
    }

    #[test]
    fn test_frame_wraps_around() {
        let mut anim = LoadingAnimation::new();
        for _ in 0..SPINNER_FRAMES.len() {
            anim.last_frame_change = Instant::now() - SPINNER_FRAME_DURATION;
            anim.tick();
        }
        assert!(anim.frame_index < SPINNER_FRAMES.len());
    }

    #[test]
    fn test_phrase_wraps_around() {
        let mut anim = LoadingAnimation::new();
        for _ in 0..LOADING_PHRASES.len() {
            anim.last_phrase_change = Instant::now() - PHRASE_DURATION;
            anim.tick();
        }
        assert!(anim.phrase_index < LOADING_PHRASES.len());
    }

    #[test]
    fn test_display_text_contains_spinner_and_phrase() {
        let anim = LoadingAnimation::new();
        let text = anim.display_text();
        assert!(SPINNER_FRAMES.iter().any(|f| text.contains(f)));
        assert!(text.contains(LOADING_PHRASES[anim.phrase_index]));
    }

    #[test]
    fn test_display_text_format() {
        let anim = LoadingAnimation::new();
        let text = anim.display_text();
        assert!(text.contains(' '));
        assert!(text.ends_with("..."));
    }

    #[test]
    fn test_current_color_rgb_returns_tuple() {
        let anim = LoadingAnimation::new();
        let (r, g, b) = anim.current_color_rgb();
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
        assert_eq!(a.frame_index, b.frame_index);
        assert_eq!(a.color_index, b.color_index);
    }
}
