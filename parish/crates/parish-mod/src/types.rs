//! Runtime data types loaded from JSON / TOML files referenced by the manifest.

use serde::{Deserialize, Serialize};

use parish_types::AnachronismEntry;
use parish_types::LanguageHint;
use parish_types::ThemePalette;

/// Prompt templates loaded from text files.
#[derive(Debug, Clone)]
pub struct PromptTemplates {
    /// Tier-1 system prompt text.
    pub tier1_system: String,
    /// Tier-1 context prompt text.
    pub tier1_context: String,
    /// Tier-2 system prompt text.
    pub tier2_system: String,
}

/// Anachronism detection data loaded from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnachronismData {
    /// Prefix injected into the LLM context alert.
    pub context_alert_prefix: String,
    /// Suffix injected into the LLM context alert.
    pub context_alert_suffix: String,
    /// Known anachronistic terms.
    pub terms: Vec<AnachronismEntry>,
}

/// A festival or holy day definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FestivalDef {
    /// Festival name.
    pub name: String,
    /// Month (1–12).
    pub month: u32,
    /// Day of month (1–31).
    pub day: u32,
    /// Short description of the festival.
    pub description: String,
}

/// Encounter text table keyed by time-of-day label.
///
/// Uses [`BTreeMap`] so the JSON output is deterministic — the editor
/// relies on this for the empty-`git diff` round-trip invariant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterTable {
    /// Encounter flavour text keyed by time-of-day (e.g. "morning", "night").
    #[serde(flatten)]
    pub by_time: std::collections::BTreeMap<String, String>,
}

/// Loading-screen configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LoadingConfig {
    /// Unicode frames for the spinner animation.
    pub spinner_frames: Vec<String>,
    /// RGB colours cycled through during the spinner animation.
    pub spinner_colors: Vec<[u8; 3]>,
    /// Random phrases shown while loading.
    pub phrases: Vec<String>,
    /// Atmospheric flavour messages shown when NPC inference fails. Empty
    /// when the mod ships none — engine falls back to a single ellipsis.
    #[serde(default)]
    pub inference_failure_messages: Vec<String>,
    /// Atmospheric messages shown when no NPC is present and the player
    /// addresses no-one. Empty when the mod ships none — engine falls
    /// back to a blank line.
    #[serde(default)]
    pub idle_messages: Vec<String>,
}

/// Sidebar section of the UI configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SidebarConfig {
    /// Label for the language-hints panel.
    #[serde(default = "default_hints_label")]
    pub hints_label: String,
}

/// Theme palette configuration loaded from `ui.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemePaletteConfig {
    /// Main background colour.
    #[serde(default = "default_theme_bg")]
    pub bg: String,
    /// Primary text colour.
    #[serde(default = "default_theme_fg")]
    pub fg: String,
    /// Accent colour for highlights and status UI.
    #[serde(default = "default_theme_accent")]
    pub accent: String,
    /// Panel background colour.
    #[serde(default = "default_theme_panel_bg")]
    pub panel_bg: String,
    /// Input background colour.
    #[serde(default = "default_theme_input_bg")]
    pub input_bg: String,
    /// Border and separator colour.
    #[serde(default = "default_theme_border")]
    pub border: String,
    /// Secondary / muted text colour.
    #[serde(default = "default_theme_muted")]
    pub muted: String,
}

impl From<ThemePaletteConfig> for ThemePalette {
    fn from(config: ThemePaletteConfig) -> Self {
        ThemePalette {
            bg: config.bg,
            fg: config.fg,
            accent: config.accent,
            panel_bg: config.panel_bg,
            input_bg: config.input_bg,
            border: config.border,
            muted: config.muted,
        }
    }
}

impl Default for ThemePaletteConfig {
    fn default() -> Self {
        Self {
            bg: default_theme_bg(),
            fg: default_theme_fg(),
            accent: default_theme_accent(),
            panel_bg: default_theme_panel_bg(),
            input_bg: default_theme_input_bg(),
            border: default_theme_border(),
            muted: default_theme_muted(),
        }
    }
}

/// Theme section of the UI configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeConfig {
    /// Legacy accent override for older mods.
    #[serde(default)]
    pub default_accent: Option<String>,
    /// Fixed theme palette used by the frontend.
    #[serde(default)]
    pub palette: ThemePaletteConfig,
    /// Optional time-of-day keyframes. When present, the engine smoothly
    /// interpolates between them to compute the live palette; when empty,
    /// the static [`Self::palette`] is used directly.
    #[serde(default)]
    pub keyframes: Vec<ThemeKeyframeConfig>,
    /// Optional map overlay style (e.g. `"grid"` for blueprint graph-paper).
    #[serde(default)]
    pub map_overlay: Option<String>,
}

/// A single time-of-day palette anchor loaded from `ui.toml`'s
/// `[[theme.keyframes]]` array.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeKeyframeConfig {
    /// Anchor hour in [0.0, 24.0) — e.g. `8.5` for morning midpoint.
    pub hour: f32,
    /// Palette at this anchor.
    pub palette: ThemePaletteConfig,
}

/// UI configuration loaded from `ui.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiConfig {
    /// Mod branding assets, such as the app/window icon.
    #[serde(default)]
    pub branding: BrandingConfig,
    /// Sidebar panel settings.
    #[serde(default)]
    pub sidebar: SidebarConfig,
    /// Theme settings.
    #[serde(default)]
    pub theme: ThemeConfig,
}

/// Branding assets loaded from `ui.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BrandingConfig {
    /// Primary app/window icon relative to the mod directory.
    #[serde(default)]
    pub app_icon: Option<String>,
    /// Small browser favicon relative to the mod directory.
    #[serde(default)]
    pub favicon: Option<String>,
}

fn default_hints_label() -> String {
    "Language Hints".to_string()
}

// Neutral charcoal-grey defaults used only when no base mod is loaded — the
// engine itself ships no aesthetic. Mods always override these via `ui.toml`.
fn default_theme_bg() -> String {
    "#18181a".to_string()
}

fn default_theme_fg() -> String {
    "#dcdce0".to_string()
}

fn default_theme_accent() -> String {
    "#8c8c96".to_string()
}

fn default_theme_panel_bg() -> String {
    "#202024".to_string()
}

fn default_theme_input_bg() -> String {
    "#28282c".to_string()
}

fn default_theme_border() -> String {
    "#484850".to_string()
}

fn default_theme_muted() -> String {
    "#96969e".to_string()
}

/// Returns the built-in fixed theme palette used when a mod does not provide one.
pub fn default_theme_palette() -> ThemePalette {
    ThemePaletteConfig::default().into()
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            hints_label: default_hints_label(),
        }
    }
}

impl ThemeConfig {
    /// Returns the fully resolved theme palette, applying any legacy overrides.
    pub fn resolved_palette(&self) -> ThemePalette {
        let mut palette = ThemePalette::from(self.palette.clone());
        if let Some(ref accent) = self.default_accent {
            palette.accent = accent.clone();
        }
        palette
    }

    /// Converts the mod-provided keyframes into the runtime [`parish_palette::Keyframe`]
    /// form consumed by `compute_palette_with_keyframes`. Returns an empty vec
    /// when the mod ships only a static palette.
    pub fn resolved_keyframes(&self) -> Vec<parish_palette::Keyframe> {
        self.keyframes
            .iter()
            .map(|kf| parish_palette::Keyframe {
                hour: kf.hour,
                palette: theme_palette_config_to_raw(&kf.palette),
            })
            .collect()
    }

    /// Returns the static palette as a [`parish_palette::RawPalette`] for use
    /// when no keyframes are provided.
    pub fn static_raw_palette(&self) -> parish_palette::RawPalette {
        theme_palette_config_to_raw(&self.palette)
    }
}

/// Converts a hex-string [`ThemePaletteConfig`] into the byte-RGB
/// [`parish_palette::RawPalette`] form used by interpolation. Malformed hex
/// values silently fall back to black so a typo in a single channel can't
/// crash startup; the loader logs a warning when this is wrong enough to
/// notice.
fn theme_palette_config_to_raw(p: &ThemePaletteConfig) -> parish_palette::RawPalette {
    let parse = |s: &str| {
        parish_palette::parse_hex_color(s).unwrap_or(parish_palette::RawColor::new(0, 0, 0))
    };
    parish_palette::RawPalette {
        bg: parse(&p.bg),
        fg: parse(&p.fg),
        accent: parse(&p.accent),
        panel_bg: parse(&p.panel_bg),
        input_bg: parse(&p.input_bg),
        border: parse(&p.border),
        muted: parse(&p.muted),
    }
}

/// A single pronunciation entry from the mod's `pronunciations.json`.
///
/// Extends [`LanguageHint`] with a list of match strings used to associate
/// the pronunciation with NPC or location names (case-insensitive).
#[derive(Debug, Clone, Deserialize)]
pub struct PronunciationEntry {
    /// The word displayed in the sidebar (may include fada/diacritics).
    pub word: String,
    /// Phonetic pronunciation guide.
    pub pronunciation: String,
    /// English meaning or gloss.
    #[serde(default)]
    pub meaning: Option<String>,
    /// Strings to match against NPC/location names (case-insensitive substring).
    #[serde(default)]
    pub matches: Vec<String>,
}

impl PronunciationEntry {
    /// Convert to a [`LanguageHint`] for frontend display.
    pub fn to_hint(&self) -> LanguageHint {
        LanguageHint {
            word: self.word.clone(),
            pronunciation: self.pronunciation.clone(),
            meaning: self.meaning.clone(),
        }
    }

    /// Check whether this entry matches any of the given names.
    pub fn matches_any(&self, names: &[&str]) -> bool {
        // Pre-compute lowercased match strings and word once, rather than
        // re-allocating them for every name in the outer loop.
        // Reduces allocations from O(names × matches) to O(matches + 1).
        let matches_lower: Vec<String> = self.matches.iter().map(|m| m.to_lowercase()).collect();
        let word_lower = self.word.to_lowercase();

        for name in names {
            let name_lower = name.to_lowercase();
            // Check the match strings first
            for m in &matches_lower {
                if name_lower.contains(m.as_str()) {
                    return true;
                }
            }
            // Fall back to matching the word itself
            if name_lower.contains(word_lower.as_str()) {
                return true;
            }
        }
        false
    }
}

/// Pronunciation data loaded from the mod's `pronunciations.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PronunciationData {
    /// Name pronunciation entries.
    pub names: Vec<PronunciationEntry>,
}
