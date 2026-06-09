//! Per-turn artifact writing. Heavy bytes (frame PNG/SVG, narrative lines) go
//! to disk under the run's artifact dir; the DB stores only the relative paths.

use std::path::{Path, PathBuf};

use crate::error::{HarnessError, Result};
use crate::frame::StateFrame;

/// Relative paths (under the run's artifact dir) of one turn's artifacts.
pub struct TurnArtifacts {
    pub frame_path: String,
    pub svg_path: String,
    pub lines_path: String,
}

/// Create (if needed) the per-turn directory `turns/NNN/` and return it.
fn turn_dir(run_dir: &Path, turn_index: u32) -> Result<PathBuf> {
    let dir = run_dir.join("turns").join(format!("{turn_index:03}"));
    std::fs::create_dir_all(&dir).map_err(|e| HarnessError::io(dir.display().to_string(), e))?;
    Ok(dir)
}

/// Write a turn's frame (PNG + SVG) and narrative lines JSON.
pub fn write_turn_artifacts(
    run_dir: &Path,
    turn_index: u32,
    frame: &StateFrame,
    lines_json: &str,
) -> Result<TurnArtifacts> {
    let dir = turn_dir(run_dir, turn_index)?;
    let png = dir.join("frame.png");
    let svg = dir.join("frame.svg");
    let lines = dir.join("lines.json");
    std::fs::write(&png, &frame.png).map_err(|e| HarnessError::io(png.display().to_string(), e))?;
    std::fs::write(&svg, frame.svg.as_bytes())
        .map_err(|e| HarnessError::io(svg.display().to_string(), e))?;
    std::fs::write(&lines, lines_json.as_bytes())
        .map_err(|e| HarnessError::io(lines.display().to_string(), e))?;
    let rel = |name: &str| format!("turns/{turn_index:03}/{name}");
    Ok(TurnArtifacts {
        frame_path: rel("frame.png"),
        svg_path: rel("frame.svg"),
        lines_path: rel("lines.json"),
    })
}

/// Write a top-level artifact file (e.g. `transcript.json`) under the run dir.
pub fn write_run_artifact(run_dir: &Path, name: &str, contents: &str) -> Result<()> {
    let path = run_dir.join(name);
    std::fs::write(&path, contents.as_bytes())
        .map_err(|e| HarnessError::io(path.display().to_string(), e))
}
