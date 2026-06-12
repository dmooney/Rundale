//! Prompt builder — composes generation prompts from engine data + the style
//! bible.
//!
//! Every prompt is prefixed with the contents of `art/style-bible.md` (the
//! fixed visual rules for the project), followed by location- or NPC-specific
//! context derived from live engine data.

use anyhow::{Context, Result};
use parish_npc::Npc;
use parish_world::graph::LocationData;
use std::path::Path;

// ---------------------------------------------------------------------------
// Style bible
// ---------------------------------------------------------------------------

/// Load the style bible from `art/style-bible.md` relative to `project_root`.
///
/// The style bible is the committed text file that defines the fixed visual
/// framing, palette, and negative rules for the project.  Every generated
/// prompt is prefixed with its content.
pub fn load_style_bible(project_root: &Path) -> Result<String> {
    let path = project_root.join("art").join("style-bible.md");
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read style bible at {}", path.display()))
}

// ---------------------------------------------------------------------------
// Plate prompt builder
// ---------------------------------------------------------------------------

/// Build a full generation prompt for a background plate.
///
/// The prompt is prefixed with the style bible, followed by a description
/// synthesised from `LocationData` fields.
pub fn build_plate_prompt(style_bible: &str, loc: &LocationData) -> String {
    let perspective = if loc.indoor { "interior" } else { "exterior" };
    let myth = loc
        .mythological_significance
        .as_deref()
        .map(|s| format!("\n\nMythological significance: {s}"))
        .unwrap_or_default();

    format!(
        "{style_bible}

---

## Scene: {name} (location id {id}) — {perspective} plate

Generate a 480x270 pixel art background plate for the following location.

**Location name:** {name}
**View:** {perspective} scene, top-down 3/4 perspective
**Description:** {desc}{myth}

The plate must be a static background with no characters, no UI text, and no
readable labels. Muted earth palette. 16-bit pixel art fidelity. The scene
should feel like a specific, remembered place in rural Ireland, 1820.",
        style_bible = style_bible,
        name = loc.name,
        id = loc.id.0,
        perspective = perspective,
        desc = loc.description_template,
        myth = myth,
    )
}

// ---------------------------------------------------------------------------
// Sprite prompt builder
// ---------------------------------------------------------------------------

/// Build a full generation prompt for an NPC sprite.
///
/// The prompt is prefixed with the style bible, followed by a description
/// synthesised from `Npc` fields.
pub fn build_sprite_prompt(style_bible: &str, npc: &Npc) -> String {
    format!(
        "{style_bible}

---

## Character: {name} (NPC id {id}) — sprite

Generate a 48x72 pixel art character sprite for the following person.  The
sprite must have a transparent background so it can be composited over scene
plates.

**Name:** {name}
**Age:** {age}
**Occupation:** {occupation}
**Appearance:** {brief}

The sprite must be a single standing figure, full body, front-facing 3/4 view,
48x72 pixels native, transparent background. Muted earth palette. 16-bit pixel
art fidelity. 1820 rural Irish clothing appropriate to the occupation and social
class.",
        style_bible = style_bible,
        name = npc.name,
        id = npc.id.0,
        age = npc.age,
        occupation = npc.occupation,
        brief = npc.brief_description,
    )
}

// ---------------------------------------------------------------------------
// Variant prompt builder
// ---------------------------------------------------------------------------

/// Build a prompt for a night (or other) variant of an existing plate.
///
/// Accepts the day plate's accepted output path as a reference image hint.
pub fn build_variant_prompt(
    style_bible: &str,
    loc: &LocationData,
    variant: &str,
    ref_description: Option<&str>,
) -> String {
    let perspective = if loc.indoor { "interior" } else { "exterior" };
    let ref_note = ref_description
        .map(|s| format!("\n\nReference: {s}"))
        .unwrap_or_default();

    format!(
        "{style_bible}

---

## Scene variant: {name} — {variant}

Generate a 480x270 pixel art background plate for the **{variant}** variant of
the following location. This must be visually consistent with the day plate
for the same scene.

**Location name:** {name}
**View:** {perspective} scene, top-down 3/4 perspective
**Description:** {desc}
**Variant:** {variant} — adjust lighting, window glow, and atmosphere
accordingly.{ref_note}

No characters, no UI text, no readable labels. Muted palette with appropriate
{variant} lighting. 16-bit pixel art fidelity.",
        style_bible = style_bible,
        name = loc.name,
        perspective = perspective,
        desc = loc.description_template,
        variant = variant,
        ref_note = ref_note,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use parish_world::LocationId;
    use parish_world::graph::{Connection, GeoKind, LocationData};

    fn fake_style_bible() -> String {
        "STYLE BIBLE CONTENT".to_string()
    }

    fn pub_location() -> LocationData {
        LocationData {
            id: LocationId(2),
            name: "Darcy's Pub".to_string(),
            description_template: "The warm interior of Darcy's Pub. Turf smoke hangs in the air."
                .to_string(),
            indoor: true,
            public: true,
            connections: vec![Connection {
                target: LocationId(1),
                path_description: "a short lane".to_string(),
                hazard: Default::default(),
            }],
            lat: 0.0,
            lon: 0.0,
            associated_npcs: vec![],
            mythological_significance: None,
            aliases: vec![],
            geo_kind: GeoKind::Fictional,
            relative_to: None,
            geo_source: None,
        }
    }

    fn padraig_npc() -> Npc {
        // Npc::new_test_npc() is a 58-year-old Publican named Padraig O'Brien
        // with brief_description "an older man behind the bar" — the exact
        // fields the spec calls golden for Padraig Darcy.  We override the
        // name to match the Rundale NPC.
        let mut npc = Npc::new_test_npc();
        npc.name = "Padraig Darcy".to_string();
        npc
    }

    // --- plate prompt golden tests ---

    #[test]
    fn plate_prompt_contains_style_bible() {
        let loc = pub_location();
        let prompt = build_plate_prompt(&fake_style_bible(), &loc);
        assert!(
            prompt.starts_with("STYLE BIBLE CONTENT"),
            "prompt must start with style bible"
        );
    }

    #[test]
    fn plate_prompt_contains_location_name() {
        let loc = pub_location();
        let prompt = build_plate_prompt(&fake_style_bible(), &loc);
        assert!(prompt.contains("Darcy's Pub"), "must contain location name");
    }

    #[test]
    fn plate_prompt_contains_interior_keyword() {
        let loc = pub_location();
        let prompt = build_plate_prompt(&fake_style_bible(), &loc);
        assert!(
            prompt.contains("interior"),
            "indoor location must mention interior"
        );
    }

    #[test]
    fn plate_prompt_contains_description_template() {
        let loc = pub_location();
        let prompt = build_plate_prompt(&fake_style_bible(), &loc);
        assert!(
            prompt.contains("Turf smoke"),
            "must include description template content"
        );
    }

    #[test]
    fn plate_prompt_contains_dimensions() {
        let loc = pub_location();
        let prompt = build_plate_prompt(&fake_style_bible(), &loc);
        assert!(prompt.contains("480x270"), "must specify plate dimensions");
    }

    #[test]
    fn plate_prompt_outdoor_says_exterior() {
        let mut loc = pub_location();
        loc.indoor = false;
        let prompt = build_plate_prompt(&fake_style_bible(), &loc);
        assert!(
            prompt.contains("exterior"),
            "outdoor location must mention exterior"
        );
    }

    #[test]
    fn plate_prompt_includes_mythological_significance() {
        let mut loc = pub_location();
        loc.mythological_significance = Some("A rath said to be home to the sídhe.".to_string());
        let prompt = build_plate_prompt(&fake_style_bible(), &loc);
        assert!(
            prompt.contains("sídhe"),
            "must include mythological significance"
        );
    }

    // --- sprite prompt golden tests ---

    #[test]
    fn sprite_prompt_contains_style_bible() {
        let npc = padraig_npc();
        let prompt = build_sprite_prompt(&fake_style_bible(), &npc);
        assert!(
            prompt.starts_with("STYLE BIBLE CONTENT"),
            "prompt must start with style bible"
        );
    }

    #[test]
    fn sprite_prompt_contains_npc_name() {
        let npc = padraig_npc();
        let prompt = build_sprite_prompt(&fake_style_bible(), &npc);
        assert!(prompt.contains("Padraig Darcy"), "must contain NPC name");
    }

    #[test]
    fn sprite_prompt_contains_age() {
        let npc = padraig_npc();
        let prompt = build_sprite_prompt(&fake_style_bible(), &npc);
        assert!(prompt.contains("58"), "must contain NPC age");
    }

    #[test]
    fn sprite_prompt_contains_occupation() {
        let npc = padraig_npc();
        let prompt = build_sprite_prompt(&fake_style_bible(), &npc);
        assert!(prompt.contains("Publican"), "must contain occupation");
    }

    #[test]
    fn sprite_prompt_contains_brief_description() {
        let npc = padraig_npc();
        let prompt = build_sprite_prompt(&fake_style_bible(), &npc);
        assert!(
            prompt.contains("an older man behind the bar"),
            "must contain brief description"
        );
    }

    #[test]
    fn sprite_prompt_contains_dimensions() {
        let npc = padraig_npc();
        let prompt = build_sprite_prompt(&fake_style_bible(), &npc);
        assert!(prompt.contains("48x72"), "must specify sprite dimensions");
    }
}
