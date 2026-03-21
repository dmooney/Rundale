//! Description template generation for game locations.
//!
//! Generates evocative 1820s-style description templates from OSM feature
//! types and tags. Supports three tiers: `curated` (hand-authored, never
//! overwritten), `template` (rule-generated), and `llm` (to be populated
//! by a future LLM enrichment pass).

use super::osm_model::{GeoFeature, LocationType};

/// Tracks how a description was generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DescriptionSource {
    /// Hand-authored by a human — never overwritten.
    Curated,
    /// Auto-generated from OSM tags and location type templates.
    Template,
    /// Placeholder awaiting LLM enrichment.
    LlmPending,
}

/// Generates a description template for a feature based on its type and tags.
///
/// Returns the template string and the generation source tier.
/// Templates include `{time}`, `{weather}`, and `{npcs_present}` placeholders.
pub fn generate_description(feature: &GeoFeature) -> (String, DescriptionSource) {
    if feature.curated {
        // This shouldn't be called for curated features, but guard anyway
        return (String::new(), DescriptionSource::Curated);
    }

    let template = match feature.location_type {
        LocationType::Pub => generate_pub_description(feature),
        LocationType::Church => generate_church_description(feature),
        LocationType::Shop => generate_shop_description(feature),
        LocationType::School => generate_school_description(feature),
        LocationType::PostOffice => generate_post_office_description(feature),
        LocationType::Farm => generate_farm_description(feature),
        LocationType::Crossroads => generate_crossroads_description(feature),
        LocationType::Bridge => generate_bridge_description(feature),
        LocationType::Well => generate_well_description(feature),
        LocationType::Waterside => generate_waterside_description(feature),
        LocationType::Bog => generate_bog_description(feature),
        LocationType::Woodland => generate_woodland_description(feature),
        LocationType::RingFort => generate_ring_fort_description(feature),
        LocationType::StandingStone => generate_standing_stone_description(feature),
        LocationType::Graveyard => generate_graveyard_description(feature),
        LocationType::Mill => generate_mill_description(feature),
        LocationType::Forge => generate_forge_description(feature),
        LocationType::LimeKiln => generate_lime_kiln_description(feature),
        LocationType::Square => generate_square_description(feature),
        LocationType::Harbour => generate_harbour_description(feature),
        LocationType::Hill => generate_hill_description(feature),
        LocationType::Ruin => generate_ruin_description(feature),
        LocationType::NamedPlace => generate_named_place_description(feature),
        LocationType::Road | LocationType::Other => generate_generic_description(feature),
    };

    (template, DescriptionSource::Template)
}

/// Generates mythological significance text for applicable location types.
///
/// Returns `Some` for location types with traditional Irish folklore
/// connections (ring forts, holy wells, crossroads, bogs, etc.).
pub fn generate_mythological_significance(feature: &GeoFeature) -> Option<String> {
    match feature.location_type {
        LocationType::RingFort => Some(
            "A rath — an ancient ring fort said to be home to the sídhe, the fairy folk. \
             No farmer dares disturb it, for those who plough fairy forts are cursed with misfortune."
                .to_string(),
        ),
        LocationType::Well => Some(
            "A holy well, blessed by a saint or perhaps older still — a place where the boundary \
             between this world and the other is thin. Rags tied to the nearby hawthorn flutter \
             like prayers."
                .to_string(),
        ),
        LocationType::Crossroads => Some(
            "Crossroads hold power in Irish folklore — a place between places, where the veil \
             is thin and deals can be struck with things best left unnamed."
                .to_string(),
        ),
        LocationType::Bog => Some(
            "Bogs preserve everything — bodies, butter, memories. People say you can hear \
             voices in the wind here on certain nights."
                .to_string(),
        ),
        LocationType::StandingStone => Some(
            "An ancient stone, raised by hands long forgotten. Some say it marks a grave, \
             others a boundary between kingdoms mortal and fey."
                .to_string(),
        ),
        LocationType::Waterside => {
            // Only significant for named lakes/rivers
            if feature.name.to_lowercase().contains("lough")
                || feature.name.to_lowercase().contains("lake")
            {
                Some(format!(
                    "{} is said to hold secrets beneath its waters — drowned churches, \
                     sunken villages, and creatures older than memory.",
                    feature.name
                ))
            } else {
                None
            }
        }
        LocationType::Graveyard => Some(
            "The dead rest here, but not always peacefully. On certain nights, \
             it is said the churchyard gate swings open of its own accord."
                .to_string(),
        ),
        _ => None,
    }
}

fn generate_pub_description(feature: &GeoFeature) -> String {
    format!(
        "The warm interior of {}. Turf smoke hangs in the air and the smell of porter \
         fills the low-ceilinged room. A fire burns in the hearth. It is {{time}} and \
         the weather outside is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_church_description(feature: &GeoFeature) -> String {
    let denomination = feature
        .tags
        .get("denomination")
        .map(|s| s.as_str())
        .unwrap_or("Catholic");
    format!(
        "A stone {denomination} church stands here — {}. The graveyard behind is thick \
         with Celtic crosses and lichen-covered headstones. The sky is {{weather}}. \
         It is {{time}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_shop_description(feature: &GeoFeature) -> String {
    let shop_type = feature
        .tags
        .get("shop")
        .map(|s| s.as_str())
        .unwrap_or("general");
    let shop_desc = match shop_type {
        "general" | "convenience" => {
            "A small general shop crammed with everything from spades and tallow candles \
             to bolts of cloth and bags of meal."
        }
        "butcher" => "A butcher's shop with cuts of meat hanging in the window.",
        "bakery" => "A bakery with the warm smell of fresh bread drifting through the door.",
        _ => "A small shop serving the needs of the parish.",
    };
    format!(
        "{shop_desc} The bell above the door jangles as you enter {}. \
         It is {{time}}. Outside, the weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_school_description(feature: &GeoFeature) -> String {
    format!(
        "A low thatched cabin where the schoolmaster teaches Latin, Irish, and arithmetic \
         to the children of the parish — {}. A rough bench sits outside where classes \
         are held in fine weather. It is {{time}}. The sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_post_office_description(feature: &GeoFeature) -> String {
    format!(
        "A small stone building where the mail coach leaves correspondence for the parish — \
         {}. A hand-lettered sign reads 'Letters' above the door. It is {{time}}. \
         The weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_farm_description(feature: &GeoFeature) -> String {
    format!(
        "A working farm with a whitewashed house and stone outbuildings roofed in thatch — \
         {}. Cattle graze in the near field. A sheepdog watches from the gate. \
         It is {{time}}. The sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_crossroads_description(_feature: &GeoFeature) -> String {
    "A quiet crossroads where narrow roads meet. A weathered stone wall lines one side, \
     half-hidden by brambles. The {weather} sky stretches over the flat midlands. \
     It is {time}. {npcs_present}"
        .to_string()
}

fn generate_bridge_description(feature: &GeoFeature) -> String {
    format!(
        "A stone bridge arches over the water — {}. The stream rushes beneath. \
         Moss and fern cling to the old stonework. It is {{time}}. The weather is {{weather}}. \
         {{npcs_present}}",
        feature.name
    )
}

fn generate_well_description(feature: &GeoFeature) -> String {
    format!(
        "A holy well surrounded by hawthorn and elder — {}. Rags and ribbons flutter from \
         the branches, left by those who came to pray. The water is cold and clear. \
         It is {{time}}. The sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_waterside_description(feature: &GeoFeature) -> String {
    let is_lake = feature.name.to_lowercase().contains("lough")
        || feature.name.to_lowercase().contains("lake");

    if is_lake {
        format!(
            "The stony shore of {} stretches out before you. The great lake shimmers \
             under the {{weather}} sky. Small islands dot the water in the distance. \
             It is {{time}}. {{npcs_present}}",
            feature.name
        )
    } else {
        format!(
            "The banks of {} — the water runs clear over smooth stones. \
             Willows trail their branches in the current. It is {{time}}. \
             The sky is {{weather}}. {{npcs_present}}",
            feature.name
        )
    }
}

fn generate_bog_description(feature: &GeoFeature) -> String {
    format!(
        "A rough track cutting through blanket bog — {}. Turf banks stand in neat rows, \
         drying in the wind. The {{weather}} sky presses down. Pools of dark water \
         gleam between the heather. It is {{time}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_woodland_description(feature: &GeoFeature) -> String {
    format!(
        "A stand of old trees — {}. Dappled light filters through the canopy. \
         The ground is soft with leaf mould and fallen branches. Birdsong fills the air. \
         It is {{time}}. The weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_ring_fort_description(feature: &GeoFeature) -> String {
    format!(
        "An ancient ring fort — {} — a raised circular mound ringed by hawthorn and elder. \
         The air feels different here, heavy and watchful. It is {{time}}. \
         The sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_standing_stone_description(feature: &GeoFeature) -> String {
    format!(
        "A tall standing stone rises from the earth — {}. Its surface is rough and \
         weathered by centuries of rain and wind. Strange markings may once have been \
         carved upon it. It is {{time}}. The weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_graveyard_description(feature: &GeoFeature) -> String {
    format!(
        "A churchyard cemetery thick with headstones and Celtic crosses — {}. \
         Some stones are so old the names have worn away entirely. Ivy creeps over \
         the low wall. It is {{time}}. The sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_mill_description(feature: &GeoFeature) -> String {
    format!(
        "A working mill by the water — {}. The great wheel turns slowly. \
         The sound of grinding stone fills the air. Sacks of grain are stacked \
         by the entrance. It is {{time}}. The weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_forge_description(_feature: &GeoFeature) -> String {
    "The forge glows red in the dim interior. The blacksmith's hammer rings on the anvil. \
     The smell of hot iron and coal smoke fills the air. It is {time}. \
     The weather outside is {weather}. {npcs_present}"
        .to_string()
}

fn generate_lime_kiln_description(feature: &GeoFeature) -> String {
    format!(
        "A squat stone lime kiln — {}. On burning days the smoke rises thick and white. \
         A flat patch of ground beside it serves as a gathering place. It is {{time}}. \
         The weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_square_description(feature: &GeoFeature) -> String {
    format!(
        "An open square at the heart of the settlement — {}. Market stalls appear \
         on fair days. The cobbles are worn smooth by generations of feet. \
         It is {{time}}. The weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_harbour_description(feature: &GeoFeature) -> String {
    format!(
        "A sheltered harbour where boats bob at their moorings — {}. \
         The smell of salt and tar hangs in the air. Fishermen mend their nets \
         on the quayside. It is {{time}} and the sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_hill_description(feature: &GeoFeature) -> String {
    let elevation = feature
        .tags
        .get("ele")
        .and_then(|e| e.parse::<f64>().ok())
        .map(|e| format!(" ({e:.0}m)"))
        .unwrap_or_default();
    format!(
        "The summit of {}{elevation}. From here you can see for miles across the \
         patchwork of fields and bogs. The wind is constant. It is {{time}}. \
         The sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_ruin_description(feature: &GeoFeature) -> String {
    let ruin_type = feature
        .tags
        .get("historic")
        .map(|s| s.as_str())
        .unwrap_or("ruins");
    let desc = match ruin_type {
        "castle" => "The crumbling walls of an old castle",
        "ruins" => "Crumbling stone walls",
        "monument" => "A weathered monument",
        _ => "Ancient ruins",
    };
    format!(
        "{desc} stand here — {}. Ivy and bramble have reclaimed much of the stonework. \
         It is {{time}}. The sky is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

fn generate_named_place_description(feature: &GeoFeature) -> String {
    let place_type = feature
        .tags
        .get("place")
        .map(|s| s.as_str())
        .unwrap_or("place");
    match place_type {
        "village" | "town" => format!(
            "The heart of {}. Cottages and cabins line the road. Smoke rises from \
             chimneys. It is {{time}}. The weather is {{weather}}. {{npcs_present}}",
            feature.name
        ),
        "hamlet" => format!(
            "A small cluster of cottages and cabins — {}. A dog barks somewhere. \
             It is {{time}}. The sky is {{weather}}. {{npcs_present}}",
            feature.name
        ),
        "townland" | "locality" => format!(
            "The townland of {}. Fields stretch in every direction, divided by stone \
             walls and hedgerows. It is {{time}}. The weather is {{weather}}. {{npcs_present}}",
            feature.name
        ),
        _ => generate_generic_description(feature),
    }
}

fn generate_generic_description(feature: &GeoFeature) -> String {
    format!(
        "{}. It is {{time}}. The weather is {{weather}}. {{npcs_present}}",
        feature.name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_feature(name: &str, loc_type: LocationType) -> GeoFeature {
        GeoFeature {
            osm_id: 1,
            osm_type: "node".to_string(),
            lat: 53.5,
            lon: -8.0,
            name: name.to_string(),
            name_ga: None,
            location_type: loc_type,
            tags: HashMap::new(),
            curated: false,
        }
    }

    #[test]
    fn test_generate_pub_description() {
        let feature = make_feature("Darcy's Pub", LocationType::Pub);
        let (desc, source) = generate_description(&feature);
        assert_eq!(source, DescriptionSource::Template);
        assert!(desc.contains("Darcy's Pub"));
        assert!(desc.contains("{time}"));
        assert!(desc.contains("{weather}"));
        assert!(desc.contains("{npcs_present}"));
    }

    #[test]
    fn test_generate_church_description() {
        let feature = make_feature("St. Brigid's Church", LocationType::Church);
        let (desc, _) = generate_description(&feature);
        assert!(desc.contains("St. Brigid's Church"));
        assert!(desc.contains("church"));
    }

    #[test]
    fn test_generate_ring_fort_description() {
        let feature = make_feature("The Rath", LocationType::RingFort);
        let (desc, _) = generate_description(&feature);
        assert!(desc.contains("ring fort"));
        assert!(desc.contains("The Rath"));
    }

    #[test]
    fn test_mythological_significance_ring_fort() {
        let feature = make_feature("Fairy Fort", LocationType::RingFort);
        let myth = generate_mythological_significance(&feature);
        assert!(myth.is_some());
        assert!(myth.unwrap().contains("sídhe"));
    }

    #[test]
    fn test_mythological_significance_pub() {
        let feature = make_feature("The Local", LocationType::Pub);
        let myth = generate_mythological_significance(&feature);
        assert!(myth.is_none());
    }

    #[test]
    fn test_mythological_significance_crossroads() {
        let feature = make_feature("A Crossroads", LocationType::Crossroads);
        let myth = generate_mythological_significance(&feature);
        assert!(myth.is_some());
        assert!(myth.unwrap().contains("veil"));
    }

    #[test]
    fn test_curated_feature_returns_curated_source() {
        let mut feature = make_feature("Test", LocationType::Pub);
        feature.curated = true;
        let (_, source) = generate_description(&feature);
        assert_eq!(source, DescriptionSource::Curated);
    }

    #[test]
    fn test_all_location_types_produce_templates() {
        let types = vec![
            LocationType::Pub,
            LocationType::Church,
            LocationType::Shop,
            LocationType::School,
            LocationType::PostOffice,
            LocationType::Farm,
            LocationType::Crossroads,
            LocationType::Bridge,
            LocationType::Well,
            LocationType::Waterside,
            LocationType::Bog,
            LocationType::Woodland,
            LocationType::RingFort,
            LocationType::StandingStone,
            LocationType::Graveyard,
            LocationType::Mill,
            LocationType::Forge,
            LocationType::LimeKiln,
            LocationType::Square,
            LocationType::Harbour,
            LocationType::Hill,
            LocationType::Ruin,
            LocationType::NamedPlace,
            LocationType::Other,
        ];

        for loc_type in types {
            let feature = make_feature("Test Place", loc_type);
            let (desc, source) = generate_description(&feature);
            assert_eq!(source, DescriptionSource::Template, "type: {loc_type:?}");
            assert!(!desc.is_empty(), "empty description for {loc_type:?}");
            assert!(desc.contains("{time}"), "missing {{time}} for {loc_type:?}");
        }
    }

    #[test]
    fn test_hill_with_elevation() {
        let mut feature = make_feature("Slieve Bawn", LocationType::Hill);
        feature.tags.insert("ele".to_string(), "264".to_string());
        let (desc, _) = generate_description(&feature);
        assert!(desc.contains("264m"));
    }
}
