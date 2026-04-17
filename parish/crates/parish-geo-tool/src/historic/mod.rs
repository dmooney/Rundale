//! Historic-map discovery pipeline — AI-assisted first-pass world generation.
//!
//! Reads tiled raster imagery from a historical Ordnance Survey source
//! (OS 6-inch First Edition, ca. 1829–42), feeds tile chunks to a
//! vision-capable LLM to identify man-made and named features, and emits
//! a `parish.json` of candidate locations with positions, names, and
//! connections already roughly correct for 1820s Ireland.
//!
//! See `docs/design/geo-tool.md` and `.claude/skills/rundale-geo-tool/`.

pub mod discover;
pub mod naming;
pub mod raster_cache;
pub mod tile_math;
pub mod tile_source;
pub mod vision_prompt;
