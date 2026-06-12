//! Image generation provider trait and implementations.
//!
//! Providers are only invoked when the corresponding API key is available.
//! Tests construct request bodies offline without making network calls.

pub mod fal;
pub mod google;
pub mod nvidia;
pub mod openai;
pub mod stability;

use anyhow::Result;
use std::path::Path;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Shared interface for image generation providers.
pub trait ImageProvider {
    /// Generate a new image from a text prompt.
    ///
    /// Returns a `Vec<Vec<u8>>` — one PNG byte buffer per candidate image.
    /// `n` is the number of candidates to generate.
    fn generate(
        &self,
        prompt: &str,
        n: u32,
        size: ImageSize,
        background: ImageBackground,
    ) -> Result<Vec<Vec<u8>>>;

    /// Generate an edited variant of an existing image (img2img / edit).
    ///
    /// `reference_paths` are local PNG files to pass as reference images.
    fn edit(
        &self,
        prompt: &str,
        reference_paths: &[&Path],
        size: ImageSize,
    ) -> Result<Vec<Vec<u8>>>;
}

/// Requested output image size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSize {
    /// 1536x1024 — large plate generation.
    Landscape1536x1024,
    /// 1024x1024 — sprite / square generation.
    Square1024x1024,
}

impl ImageSize {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageSize::Landscape1536x1024 => "1536x1024",
            ImageSize::Square1024x1024 => "1024x1024",
        }
    }

    /// Pixel dimensions `(width, height)` — for providers whose API takes
    /// explicit width/height (NVIDIA NIM, fal) rather than a size string.
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            ImageSize::Landscape1536x1024 => (1536, 1024),
            ImageSize::Square1024x1024 => (1024, 1024),
        }
    }

    /// Aspect-ratio label — for providers whose API takes an aspect-ratio
    /// string (Stability, Google). The final plate is 16:9 (480x270).
    pub fn aspect_ratio(self) -> &'static str {
        match self {
            ImageSize::Landscape1536x1024 => "16:9",
            ImageSize::Square1024x1024 => "1:1",
        }
    }
}

/// Whether the generated image should have a transparent background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageBackground {
    /// Solid background (for plates).
    Opaque,
    /// Transparent background (for sprites).
    Transparent,
}

impl ImageBackground {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageBackground::Opaque => "opaque",
            ImageBackground::Transparent => "transparent",
        }
    }
}

// ---------------------------------------------------------------------------
// Payload-size guard (AGENTS rule 16)
// ---------------------------------------------------------------------------

/// Truncate `prompt` so that the total returned string is <= `budget` chars,
/// prefixing any dropped content with a `[truncated N chars] ` marker.
///
/// The budget should be <= 90% of the provider's documented limit.
/// The marker itself counts against the budget so that the final string
/// length is always <= budget.
pub fn apply_prompt_budget(prompt: &str, budget: usize) -> String {
    if prompt.len() <= budget {
        return prompt.to_string();
    }
    let dropped = prompt.len() - budget;
    // Pre-compute the marker for the conservative dropped count.
    let marker = format!("[truncated {dropped} chars] ");
    // The actual content we can keep is budget minus the marker length.
    let content_budget = budget.saturating_sub(marker.len());
    // Floor to a valid char boundary.
    let content_budget = floor_char_boundary(prompt, content_budget);
    let truncated = &prompt[..content_budget];
    let actual_dropped = prompt.len() - content_budget;
    // Reformat with the precise dropped count.
    format!("[truncated {actual_dropped} chars] {truncated}")
}

/// Return the largest char boundary <= `index` in `s`.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
