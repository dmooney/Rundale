//! Loads Rundale mod data and splits it into retrievable lore chunks.
//!
//! The aim is one fact per chunk: a chunk is a small, self-contained passage
//! about a single location, a single NPC trait, one festival, etc. Retrieval
//! is only as good as the chunking — merging everything for an NPC into one
//! blob would mean one recall returns that whole blob, blowing out the prompt.

use serde::Deserialize;
use std::path::Path;

/// A retrievable lore passage, pre-embedding.
#[derive(Debug, Clone)]
pub struct LoreChunk {
    pub id: String,
    pub source: String,
    pub content: String,
}

impl LoreChunk {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct WorldFile {
    locations: Vec<Location>,
}

#[derive(Debug, Deserialize)]
struct Location {
    id: u32,
    name: String,
    #[serde(default)]
    description_template: Option<String>,
    #[serde(default)]
    mythological_significance: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpcsFile {
    npcs: Vec<NpcEntry>,
}

#[derive(Debug, Deserialize)]
struct NpcEntry {
    id: u32,
    name: String,
    #[serde(default)]
    age: Option<u32>,
    #[serde(default)]
    occupation: Option<String>,
    #[serde(default)]
    personality: Option<String>,
    #[serde(default)]
    knowledge: Vec<String>,
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
struct Relationship {
    target_id: u32,
    kind: String,
    strength: f32,
}

#[derive(Debug, Deserialize)]
struct Festival {
    name: String,
    month: u32,
    day: u32,
    description: String,
}

/// Builds the default corpus for the shipped Rundale mod.
///
/// Reads `mods/rundale/{world,npcs,festivals}.json` relative to `mod_dir` and
/// produces one chunk per logical fact: per location (description + folklore
/// as separate chunks), per NPC (identity + personality + each knowledge
/// entry + each relationship), per festival.
pub fn build_rundale_corpus(mod_dir: &Path) -> Result<Vec<LoreChunk>, String> {
    let world: WorldFile = read_json(&mod_dir.join("world.json"))?;
    let npcs: NpcsFile = read_json(&mod_dir.join("npcs.json"))?;
    let festivals: Vec<Festival> = read_json(&mod_dir.join("festivals.json"))?;

    let mut chunks = Vec::new();
    chunks.extend(chunk_locations(&world.locations));
    chunks.extend(chunk_npcs(&npcs.npcs));
    chunks.extend(chunk_festivals(&festivals));
    Ok(chunks)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

fn chunk_locations(locations: &[Location]) -> Vec<LoreChunk> {
    let mut out = Vec::new();
    for loc in locations {
        let source = format!("location:{}", loc.name);
        if let Some(desc) = &loc.description_template {
            let cleaned = strip_template(desc);
            out.push(LoreChunk::new(
                format!("loc-{}-desc", loc.id),
                &source,
                format!("{}: {}", loc.name, cleaned),
            ));
        }
        if let Some(myth) = &loc.mythological_significance {
            out.push(LoreChunk::new(
                format!("loc-{}-myth", loc.id),
                &source,
                format!("{} — folklore: {}", loc.name, myth),
            ));
        }
    }
    out
}

fn chunk_npcs(npcs: &[NpcEntry]) -> Vec<LoreChunk> {
    let id_to_name: std::collections::HashMap<u32, String> =
        npcs.iter().map(|n| (n.id, n.name.clone())).collect();

    let mut out = Vec::new();
    for npc in npcs {
        let source = format!("npc:{}", npc.name);
        let identity = match (&npc.occupation, npc.age) {
            (Some(occ), Some(age)) => format!("{} is a {} ({} years old).", npc.name, occ, age),
            (Some(occ), None) => format!("{} is a {}.", npc.name, occ),
            (None, Some(age)) => format!("{} is {} years old.", npc.name, age),
            (None, None) => format!("{} lives in the parish.", npc.name),
        };
        out.push(LoreChunk::new(
            format!("npc-{}-identity", npc.id),
            &source,
            identity,
        ));

        if let Some(personality) = &npc.personality {
            out.push(LoreChunk::new(
                format!("npc-{}-personality", npc.id),
                &source,
                format!("About {}: {}", npc.name, personality),
            ));
        }

        for (i, fact) in npc.knowledge.iter().enumerate() {
            out.push(LoreChunk::new(
                format!("npc-{}-knowledge-{}", npc.id, i),
                &source,
                format!("{} knows: {}", npc.name, fact),
            ));
        }

        for (i, rel) in npc.relationships.iter().enumerate() {
            let other = id_to_name
                .get(&rel.target_id)
                .map(|s| s.as_str())
                .unwrap_or("someone");
            let tone = relationship_tone(rel.strength);
            out.push(LoreChunk::new(
                format!("npc-{}-rel-{}", npc.id, i),
                &source,
                format!(
                    "{} has a {} relationship with {} ({}).",
                    npc.name,
                    rel.kind.to_lowercase(),
                    other,
                    tone
                ),
            ));
        }
    }
    out
}

fn chunk_festivals(festivals: &[Festival]) -> Vec<LoreChunk> {
    festivals
        .iter()
        .map(|f| {
            LoreChunk::new(
                format!("festival-{}", f.name.to_lowercase()),
                format!("festival:{}", f.name),
                format!(
                    "{} is a festival held on the {} of {}. {}",
                    f.name,
                    ordinal(f.day),
                    month_name(f.month),
                    f.description
                ),
            )
        })
        .collect()
}

fn strip_template(s: &str) -> String {
    // Location descriptions contain `{weather}` / `{time}` placeholders — keep
    // the surrounding prose but drop the braces so they don't pollute the
    // embedding tokens.
    s.replace("{weather}", "weather").replace("{time}", "time")
}

fn relationship_tone(strength: f32) -> &'static str {
    if strength > 0.7 {
        "very close"
    } else if strength > 0.3 {
        "friendly"
    } else if strength > -0.2 {
        "acquainted"
    } else if strength > -0.6 {
        "strained"
    } else {
        "hostile"
    }
}

fn ordinal(day: u32) -> String {
    let suffix = match day % 100 {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{day}{suffix}")
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rundale_dir() -> PathBuf {
        // CARGO_MANIFEST_DIR points at the crate root.
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("mods")
            .join("rundale")
    }

    #[test]
    fn build_rundale_corpus_loads_shipped_mod() {
        let chunks = build_rundale_corpus(&rundale_dir()).expect("corpus should load");
        assert!(
            chunks.len() > 50,
            "expected > 50 chunks from shipped Rundale mod, got {}",
            chunks.len()
        );
        // Every chunk has a non-empty content
        for c in &chunks {
            assert!(!c.content.is_empty(), "chunk {} has empty content", c.id);
            assert!(!c.id.is_empty());
            assert!(!c.source.is_empty());
        }
    }

    #[test]
    fn corpus_contains_padraig_identity() {
        let chunks = build_rundale_corpus(&rundale_dir()).expect("corpus should load");
        assert!(
            chunks
                .iter()
                .any(|c| c.content.contains("Padraig") && c.content.contains("Publican")),
            "expected a chunk identifying Padraig as the publican"
        );
    }

    #[test]
    fn corpus_contains_festivals() {
        let chunks = build_rundale_corpus(&rundale_dir()).expect("corpus should load");
        let names = ["Imbolc", "Bealtaine", "Lughnasa", "Samhain"];
        for name in names {
            assert!(
                chunks.iter().any(|c| c.content.contains(name)),
                "missing festival: {name}"
            );
        }
    }

    #[test]
    fn corpus_contains_location_folklore() {
        let chunks = build_rundale_corpus(&rundale_dir()).expect("corpus should load");
        assert!(
            chunks
                .iter()
                .any(|c| c.source.starts_with("location:") && c.content.contains("folklore")),
            "expected at least one location:*:folklore chunk"
        );
    }

    #[test]
    fn template_placeholders_are_stripped() {
        let cleaned = strip_template("It is {time} and the sky is {weather}.");
        assert!(!cleaned.contains('{'));
        assert!(!cleaned.contains('}'));
    }

    #[test]
    fn ordinal_handles_common_cases() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(4), "4th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(21), "21st");
    }

    #[test]
    fn relationship_tone_boundaries() {
        assert_eq!(relationship_tone(0.9), "very close");
        assert_eq!(relationship_tone(0.5), "friendly");
        assert_eq!(relationship_tone(0.0), "acquainted");
        assert_eq!(relationship_tone(-0.4), "strained");
        assert_eq!(relationship_tone(-0.9), "hostile");
    }
}
