//! Frontend theme palette type.

use serde::{Deserialize, Serialize};

/// CSS hex-string theme palette consumed by the frontend.
///
/// A 7-slot palette of `"#rrggbb"` colour strings. Lives in `parish-types`
/// (the zero-dependency leaf) so both the mod loader (`parish-mod`) and the
/// IPC layer (`parish-core::ipc`) can name it without a dependency cycle.
/// `parish-palette` owns the `From<RawPalette>` conversion that turns the
/// interpolated byte-RGB form into these hex strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemePalette {
    /// Main background colour (`"#rrggbb"`).
    pub bg: String,
    /// Foreground (text) colour.
    pub fg: String,
    /// Accent colour for highlights and the status bar.
    pub accent: String,
    /// Slightly offset panel background.
    pub panel_bg: String,
    /// Input field background.
    pub input_bg: String,
    /// Border/separator colour.
    pub border: String,
    /// Muted colour for secondary text.
    pub muted: String,
}
