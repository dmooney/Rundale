//! Asset-mod theme registry.
//!
//! Themes are loaded from `themes.toml` files inside [`ModKind::Asset`](crate::game_mod::ModKind)
//! mods. Each entry is a `(name, mode)` pair with a [`ThemePalette`] — the engine
//! ships zero hardcoded themes and consults this registry when handling `/theme`.
//!
//! ## Schema
//!
//! ```toml
//! # mods/zork-theme/themes.toml
//! [[themes]]
//! name = "zork"
//! mode = "c64"
//! label = "Zork (C64)"
//! message = "Theme set to Zork (C64)."
//! palette = { bg = "#1f1b96", fg = "#a8a8ff", accent = "#ffffff",
//!             panel_bg = "#1f1b96", input_bg = "#15116b",
//!             border = "#7878ff", muted = "#6c5eb5",
//!             font_body = "'Courier New', monospace",
//!             chat_align = "left", bubble_style = "flat",
//!             status_invert = true }
//!
//! # Optional: this name's default mode (used when /theme <name> has no mode arg)
//! [[mode_defaults]]
//! name = "zork"
//! default_mode = "c64"
//!
//! # Optional: a "synthetic" mode that resolves to another mode based on game time
//! [[mode_aliases]]
//! name = "solarized"
//! mode = "auto"
//! day_mode = "light"
//! night_mode = "dark"
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::game_mod::{DiscoveredMod, ModKind};
use crate::ipc::ThemePalette;
use parish_types::error::ParishError;

/// A single theme entry — one `(name, mode)` pair from an asset mod.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeEntry {
    /// Theme name, e.g. `"solarized"`. Multiple entries may share a name (one per mode).
    pub name: String,
    /// Mode within the theme, e.g. `"light"` / `"dark"` / `"c64"`. Empty string for mode-less themes.
    #[serde(default)]
    pub mode: String,
    /// Human-readable label for menus and confirmation messages.
    #[serde(default)]
    pub label: String,
    /// Optional flavour text shown when the user applies this theme.
    #[serde(default)]
    pub message: Option<String>,
    /// The palette to apply.
    pub palette: ThemePalette,
}

/// Default mode for a theme name (used when `/theme <name>` is invoked with no mode).
#[derive(Debug, Clone, Deserialize)]
pub struct ModeDefault {
    pub name: String,
    pub default_mode: String,
}

/// Synthetic mode that resolves to another mode based on game time.
///
/// The frontend selects `day_mode` when the in-game hour is daytime and
/// `night_mode` at night. Both must already exist as `[[themes]]` entries
/// with the same `name`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModeAlias {
    pub name: String,
    pub mode: String,
    pub day_mode: String,
    pub night_mode: String,
}

/// The on-disk shape of a `themes.toml` file.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ThemeManifest {
    #[serde(default)]
    pub themes: Vec<ThemeEntry>,
    #[serde(default)]
    pub mode_defaults: Vec<ModeDefault>,
    #[serde(default)]
    pub mode_aliases: Vec<ModeAlias>,
}

/// A summary of a theme registry entry, sent to the frontend so it can
/// list/preview available themes without re-parsing the mod files.
#[derive(Debug, Clone, Serialize)]
pub struct ThemeListing {
    pub name: String,
    pub mode: String,
    pub label: String,
}

/// Serializable wire shape of a [`ThemeRegistry`]. Sent to the frontend once
/// at startup (via `UiConfigSnapshot`) so themes can be applied client-side
/// without round-tripping through the backend.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ThemeRegistrySnapshot {
    pub themes: Vec<ThemeRegistryEntry>,
    pub mode_aliases: Vec<ModeAlias>,
    pub mode_defaults: BTreeMap<String, String>,
}

/// Wire shape of a single theme entry, including the full palette.
#[derive(Debug, Clone, Serialize)]
pub struct ThemeRegistryEntry {
    pub name: String,
    pub mode: String,
    pub label: String,
    pub palette: ThemePalette,
}

/// Merged registry of all asset-mod themes.
#[derive(Debug, Clone, Default)]
pub struct ThemeRegistry {
    /// Keyed by `(name, mode)`. BTreeMap so listings are deterministic.
    entries: BTreeMap<(String, String), ThemeEntry>,
    /// Default mode per name (e.g. `solarized -> auto`).
    defaults: BTreeMap<String, String>,
    /// Synthetic modes (e.g. `solarized auto -> {day=light, night=dark}`).
    aliases: BTreeMap<(String, String), ModeAlias>,
}

impl ThemeRegistry {
    /// Returns the palette for a `(name, mode)` pair, if registered.
    pub fn get(&self, name: &str, mode: &str) -> Option<&ThemeEntry> {
        self.entries.get(&(name.to_string(), mode.to_string()))
    }

    /// Returns the default mode for a theme name, if any. Falls back to the
    /// empty string if the name is registered but has no declared default.
    pub fn default_mode_for(&self, name: &str) -> Option<&str> {
        self.defaults.get(name).map(String::as_str)
    }

    /// Returns the alias declaration for a synthetic mode, if any.
    pub fn alias(&self, name: &str, mode: &str) -> Option<&ModeAlias> {
        self.aliases.get(&(name.to_string(), mode.to_string()))
    }

    /// All distinct theme names in the registry, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.entries.keys().map(|(n, _)| n.clone()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// Modes registered for a given name, including synthetic aliases. Sorted.
    pub fn modes_for(&self, name: &str) -> Vec<String> {
        let mut modes: Vec<String> = self
            .entries
            .keys()
            .filter(|(n, _)| n == name)
            .map(|(_, m)| m.clone())
            .chain(
                self.aliases
                    .keys()
                    .filter(|(n, _)| n == name)
                    .map(|(_, m)| m.clone()),
            )
            .collect();
        modes.sort();
        modes.dedup();
        modes
    }

    /// Returns a serializable listing of all themes (without palettes), for
    /// shipping to the frontend at startup.
    pub fn listings(&self) -> Vec<ThemeListing> {
        let mut out: Vec<ThemeListing> = self
            .entries
            .values()
            .map(|e| ThemeListing {
                name: e.name.clone(),
                mode: e.mode.clone(),
                label: if e.label.is_empty() {
                    if e.mode.is_empty() {
                        e.name.clone()
                    } else {
                        format!("{} {}", e.name, e.mode)
                    }
                } else {
                    e.label.clone()
                },
            })
            .collect();
        // Plus synthetic mode aliases so the frontend can offer e.g. `solarized auto`.
        for (name, mode) in self.aliases.keys() {
            out.push(ThemeListing {
                name: name.clone(),
                mode: mode.clone(),
                label: format!("{name} {mode}"),
            });
        }
        out
    }

    /// Returns the full serializable palette map, for the frontend to apply
    /// any theme without round-tripping through the backend.
    pub fn palettes(&self) -> Vec<(String, String, ThemePalette)> {
        self.entries
            .values()
            .map(|e| (e.name.clone(), e.mode.clone(), e.palette.clone()))
            .collect()
    }

    /// All declared mode aliases, for shipping to the frontend.
    pub fn aliases_vec(&self) -> Vec<ModeAlias> {
        self.aliases.values().cloned().collect()
    }

    /// All declared mode defaults, for shipping to the frontend.
    pub fn defaults_map(&self) -> BTreeMap<String, String> {
        self.defaults.clone()
    }

    /// Returns a wire-shape snapshot for serialization to the frontend.
    pub fn snapshot(&self) -> ThemeRegistrySnapshot {
        ThemeRegistrySnapshot {
            themes: self
                .entries
                .values()
                .map(|e| ThemeRegistryEntry {
                    name: e.name.clone(),
                    mode: e.mode.clone(),
                    label: if e.label.is_empty() {
                        if e.mode.is_empty() {
                            e.name.clone()
                        } else {
                            format!("{} {}", e.name, e.mode)
                        }
                    } else {
                        e.label.clone()
                    },
                    palette: e.palette.clone(),
                })
                .collect(),
            mode_aliases: self.aliases.values().cloned().collect(),
            mode_defaults: self.defaults.clone(),
        }
    }

    /// Merges another manifest's entries into this registry. Later mods override
    /// earlier ones on conflict — deterministic because callers feed mods in
    /// lexicographic-id order from [`crate::game_mod::discover_mods`].
    pub fn extend_from(&mut self, manifest: ThemeManifest) {
        for entry in manifest.themes {
            let key = (entry.name.clone(), entry.mode.clone());
            self.entries.insert(key, entry);
        }
        for def in manifest.mode_defaults {
            self.defaults.insert(def.name.clone(), def.default_mode);
        }
        for alias in manifest.mode_aliases {
            let key = (alias.name.clone(), alias.mode.clone());
            self.aliases.insert(key, alias);
        }
    }
}

/// Walks the auxiliary mods, parsing each `themes.toml` (if present) and
/// merging into a single [`ThemeRegistry`].
///
/// Per-mod parse failures are logged via `tracing` and skipped — a broken
/// asset mod must never prevent the game from starting.
pub fn load_asset_themes(auxiliary: &[DiscoveredMod]) -> ThemeRegistry {
    let mut registry = ThemeRegistry::default();
    for m in auxiliary {
        if m.kind != ModKind::Asset {
            continue;
        }
        match load_themes_from_dir(&m.path) {
            Ok(Some(manifest)) => registry.extend_from(manifest),
            Ok(None) => {} // No themes.toml in this asset mod — fine.
            Err(e) => tracing::warn!(
                mod_id = %m.id,
                error = %e,
                "Failed to load themes from asset mod; skipping."
            ),
        }
    }
    registry
}

fn load_themes_from_dir(mod_dir: &Path) -> Result<Option<ThemeManifest>, ParishError> {
    let path = mod_dir.join("themes.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| ParishError::Config(format!("read {}: {e}", path.display())))?;
    let manifest: ThemeManifest = toml::from_str(&raw)
        .map_err(|e| ParishError::Config(format!("parse {}: {e}", path.display())))?;
    Ok(Some(manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_mod::ModKind;
    use std::fs;
    use tempfile::TempDir;

    fn write_asset_mod(dir: &std::path::Path, id: &str, themes_toml: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("mod.toml"),
            format!(
                "[mod]\nname = \"{id}\"\nid = \"{id}\"\nversion = \"0.0.0\"\ndescription = \"x\"\nkind = \"asset\"\n"
            ),
        )
        .unwrap();
        fs::write(dir.join("themes.toml"), themes_toml).unwrap();
    }

    fn discover_asset(dir: std::path::PathBuf, id: &str) -> DiscoveredMod {
        DiscoveredMod {
            path: dir,
            kind: ModKind::Asset,
            id: id.to_string(),
        }
    }

    #[test]
    fn empty_auxiliary_returns_empty_registry() {
        let reg = load_asset_themes(&[]);
        assert!(reg.entries.is_empty());
        assert!(reg.names().is_empty());
    }

    #[test]
    fn loads_themes_from_asset_mod() {
        let tmp = TempDir::new().unwrap();
        let zork_dir = tmp.path().join("zork-theme");
        write_asset_mod(
            &zork_dir,
            "zork-theme",
            r##"
[[themes]]
name = "zork"
mode = "c64"
label = "Zork (C64)"
message = "Pitch black."
palette = { bg = "#1f1b96", fg = "#a8a8ff", accent = "#ffffff", panel_bg = "#1f1b96", input_bg = "#15116b", border = "#7878ff", muted = "#6c5eb5", chat_align = "left", status_invert = true }

[[themes]]
name = "zork"
mode = "dos"
label = "Zork (DOS)"
palette = { bg = "#000000", fg = "#c0c0c0", accent = "#ffff55", panel_bg = "#000000", input_bg = "#0a0a0a", border = "#808080", muted = "#808080", chat_align = "left", status_invert = true }

[[mode_defaults]]
name = "zork"
default_mode = "c64"
"##,
        );
        let reg = load_asset_themes(&[discover_asset(zork_dir, "zork-theme")]);
        assert_eq!(reg.names(), vec!["zork"]);
        assert_eq!(reg.modes_for("zork"), vec!["c64", "dos"]);
        assert_eq!(reg.default_mode_for("zork"), Some("c64"));
        let c64 = reg.get("zork", "c64").unwrap();
        assert_eq!(c64.palette.bg, "#1f1b96");
        assert_eq!(c64.palette.chat_align.as_deref(), Some("left"));
        assert_eq!(c64.palette.status_invert, Some(true));
    }

    #[test]
    fn ignores_setting_kind_mods() {
        let tmp = TempDir::new().unwrap();
        let setting_dir = tmp.path().join("rundale");
        fs::create_dir_all(&setting_dir).unwrap();
        fs::write(setting_dir.join("themes.toml"), "garbage").unwrap();
        let discovered = DiscoveredMod {
            path: setting_dir,
            kind: ModKind::Setting,
            id: "rundale".to_string(),
        };
        let reg = load_asset_themes(&[discovered]);
        assert!(reg.entries.is_empty());
    }

    #[test]
    fn skips_asset_mod_without_themes_file() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("noisy");
        fs::create_dir_all(&dir).unwrap();
        let reg = load_asset_themes(&[discover_asset(dir, "noisy")]);
        assert!(reg.entries.is_empty());
    }

    #[test]
    fn merges_multiple_asset_mods() {
        let tmp = TempDir::new().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        write_asset_mod(
            &a,
            "a",
            r##"
[[themes]]
name = "a-theme"
mode = ""
palette = { bg = "#111111", fg = "#222222", accent = "#333333", panel_bg = "#444444", input_bg = "#555555", border = "#666666", muted = "#777777" }
"##,
        );
        write_asset_mod(
            &b,
            "b",
            r##"
[[themes]]
name = "b-theme"
mode = ""
palette = { bg = "#aaaaaa", fg = "#bbbbbb", accent = "#cccccc", panel_bg = "#dddddd", input_bg = "#eeeeee", border = "#ffffff", muted = "#888888" }
"##,
        );
        let reg = load_asset_themes(&[discover_asset(a, "a"), discover_asset(b, "b")]);
        let mut names = reg.names();
        names.sort();
        assert_eq!(names, vec!["a-theme", "b-theme"]);
    }

    #[test]
    fn malformed_themes_toml_is_skipped_not_fatal() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("themes.toml"), "this is not valid toml [[").unwrap();
        let reg = load_asset_themes(&[discover_asset(dir, "broken")]);
        assert!(reg.entries.is_empty());
    }

    #[test]
    fn shipped_asset_mods_parse() {
        // Smoke test: every asset mod in the repo's `mods/` directory must
        // have a parseable `themes.toml` (or none). Catches schema drift.
        let mods_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods");
        if !mods_root.is_dir() {
            return; // workspace check; no mods checked out
        }
        for entry in std::fs::read_dir(&mods_root).unwrap().flatten() {
            let dir = entry.path();
            if !dir.is_dir() || !dir.join("mod.toml").is_file() {
                continue;
            }
            let manifest = std::fs::read_to_string(dir.join("mod.toml")).unwrap();
            if !manifest.contains("kind = \"asset\"") {
                continue;
            }
            // load_themes_from_dir must succeed for every asset mod
            let result = load_themes_from_dir(&dir);
            assert!(
                result.is_ok(),
                "Asset mod at {} has malformed themes.toml: {:?}",
                dir.display(),
                result
            );
        }
    }

    #[test]
    fn mode_aliases_loaded() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("solarized");
        write_asset_mod(
            &dir,
            "solarized",
            r##"
[[themes]]
name = "solarized"
mode = "light"
palette = { bg = "#fdf6e3", fg = "#586e75", accent = "#268bd2", panel_bg = "#eee8d5", input_bg = "#e6dfc5", border = "#93a1a1", muted = "#93a1a1" }

[[themes]]
name = "solarized"
mode = "dark"
palette = { bg = "#002b36", fg = "#839496", accent = "#268bd2", panel_bg = "#073642", input_bg = "#0d3f4f", border = "#586e75", muted = "#586e75" }

[[mode_aliases]]
name = "solarized"
mode = "auto"
day_mode = "light"
night_mode = "dark"

[[mode_defaults]]
name = "solarized"
default_mode = "auto"
"##,
        );
        let reg = load_asset_themes(&[discover_asset(dir, "solarized")]);
        let alias = reg.alias("solarized", "auto").unwrap();
        assert_eq!(alias.day_mode, "light");
        assert_eq!(alias.night_mode, "dark");
        assert_eq!(reg.default_mode_for("solarized"), Some("auto"));
        let modes = reg.modes_for("solarized");
        assert!(modes.contains(&"auto".to_string()));
        assert!(modes.contains(&"light".to_string()));
        assert!(modes.contains(&"dark".to_string()));
    }
}
