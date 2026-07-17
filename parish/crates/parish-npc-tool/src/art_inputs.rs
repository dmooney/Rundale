//! Notebook person-art input export.
//!
//! The runtime NPC catalogue is strong on identity, role, schedule, and
//! relationships, but it is not a complete visual contract. This module merges
//! that canonical data with a reviewed art-direction supplement so the later
//! image-generation pipeline can start from structured source data rather than
//! hand-written one-off prompts.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use parish_npc::data::{NpcFileEntry, ScheduleFileEntry};
use serde::{Deserialize, Serialize};
use serde_json::ser::{PrettyFormatter, Serializer};

use crate::catalog::load_catalog;

const ART_INPUT_SCHEMA_VERSION: u32 = 3;
const ART_DIRECTION_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Deserialize)]
struct WorldFile {
    locations: Vec<WorldLocation>,
}

#[derive(Debug, Deserialize)]
struct WorldLocation {
    id: u32,
    name: String,
    #[serde(default)]
    description_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArtDirectionFile {
    schema_version: u32,
    global_style: GlobalStyle,
    fallback: FallbackArtDirection,
    npcs: Vec<NpcArtDirection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobalStyle {
    style_reference: String,
    source_assets: SourceAssetContract,
    medium: Vec<String>,
    setting: Vec<String>,
    palette: Vec<String>,
    common_constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceAssetContract {
    portrait_source: Vec<String>,
    marker_source: Vec<String>,
    runtime_derivatives: Vec<String>,
    sheet_policy: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FallbackArtDirection {
    portrait_identity: PortraitDirection,
    marker_identity: MarkerDirection,
    avoid: Vec<String>,
    authoring_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NpcArtDirection {
    npc_id: u32,
    #[serde(default)]
    npc_name: Option<String>,
    portrait_identity: PortraitDirection,
    marker_identity: MarkerDirection,
    avoid: Vec<String>,
    authoring_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortraitDirection {
    visual_identity_seed: String,
    identity_cohort: String,
    apparent_age: String,
    facial_geometry: FacialGeometry,
    distinguishing_features: Vec<String>,
    hair: String,
    #[serde(skip_serializing)]
    hair_topology: HairIdentity,
    clothing: String,
    pose_expression: String,
    props: Vec<String>,
    palette_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HairIdentity {
    color_and_texture: String,
    front: HairFeature,
    rear: HairFeature,
    covering: HairFeature,
    silhouette: HairFeature,
    loose_details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HairFeature {
    family: String,
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FacialGeometry {
    face_shape: String,
    proportions: String,
    brow_and_eyes: String,
    nose: String,
    mouth: String,
    jaw_and_chin: String,
    cheekbones: String,
    hairline: String,
    age_detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerDirection {
    composition: MarkerComposition,
    silhouette: String,
    stance: String,
    empty_hand_pose: MarkerHandPose,
    readability_cues: Vec<MarkerReadabilityCue>,
    tiny_readability_notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MarkerComposition {
    CharacterOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MarkerHandPose {
    BothAtSides,
    HandsClasped,
    OneOnHipOneAtSide,
    ArmsFolded,
    HandsInPockets,
    OneHandGesturing,
    HandsBehindBack,
    HandsNearCoatFront,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerReadabilityCue {
    kind: MarkerReadabilityCueKind,
    description: String,
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MarkerReadabilityCueKind {
    Face,
    HairOrHeadwear,
    Clothing,
    BodyShape,
    Stance,
}

impl MarkerReadabilityCueKind {
    fn prompt_text(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::HairOrHeadwear => "hair or headwear",
            Self::Clothing => "clothing",
            Self::BodyShape => "body shape",
            Self::Stance => "stance",
        }
    }
}

impl MarkerHandPose {
    fn prompt_text(self) -> &'static str {
        match self {
            Self::BothAtSides => "both empty hands relaxed at the sides",
            Self::HandsClasped => "both empty hands clasped together",
            Self::OneOnHipOneAtSide => "one empty hand on the hip and one relaxed at the side",
            Self::ArmsFolded => "empty arms folded across the body",
            Self::HandsInPockets => "both empty hands tucked into clothing pockets",
            Self::OneHandGesturing => "one empty hand making a small gesture and the other relaxed",
            Self::HandsBehindBack => "both empty hands held behind the back",
            Self::HandsNearCoatFront => "both empty hands resting near the coat front",
        }
    }
}

#[derive(Debug, Serialize)]
struct ArtInputDataset {
    schema_version: u32,
    source: ArtInputSource,
    global_style: GlobalStyle,
    existing_metadata_assessment: ExistingMetadataAssessment,
    fallback: GeneratedFallbackArtInput,
    npcs: Vec<GeneratedNpcArtInput>,
}

#[derive(Debug, Serialize)]
struct ArtInputSource {
    npcs_json: String,
    world_json: String,
    art_direction_json: String,
    generator: &'static str,
}

#[derive(Debug, Serialize)]
struct ExistingMetadataAssessment {
    npc_count: usize,
    canonical_npc_data_sufficient_without_art_direction: bool,
    reason: String,
    weak_partial_count: usize,
    strong_partial_count: usize,
}

#[derive(Debug, Serialize)]
struct GeneratedFallbackArtInput {
    art_direction: FallbackArtDirection,
    pair_prompt: String,
    portrait_prompt: String,
    marker_prompt: String,
}

#[derive(Debug, Serialize)]
struct GeneratedNpcArtInput {
    npc_id: u32,
    name: String,
    age: u8,
    pronouns: Option<String>,
    occupation: String,
    mood: String,
    brief_description: Option<String>,
    home: LocationSummary,
    workplace: Option<LocationSummary>,
    source_personality_summary: String,
    source_knowledge: Vec<String>,
    source_relationships: Vec<RelationshipSummary>,
    source_schedule_cues: Vec<ScheduleCue>,
    source_visual_cues: Vec<String>,
    source_metadata_status: SourceMetadataStatus,
    art_direction: NpcArtDirection,
    pair_prompt: String,
    portrait_prompt: String,
    marker_prompt: String,
}

#[derive(Debug, Serialize)]
struct LocationSummary {
    id: u32,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Serialize)]
struct RelationshipSummary {
    target_id: u32,
    target_name: String,
    kind: String,
    strength: f64,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct ScheduleCue {
    location_id: u32,
    location_name: String,
    activity: String,
}

#[derive(Debug, Serialize)]
struct SourceMetadataStatus {
    canonical_npc_data_sufficient_for_production_art: bool,
    status: &'static str,
    reason: String,
}

/// Writes a complete, generator-ready person-art input dataset and returns the
/// number of NPC entries exported.
pub(crate) fn export_art_inputs(
    npcs_path: &Path,
    world_path: &Path,
    art_direction_path: &Path,
    output_path: &Path,
) -> Result<usize> {
    let npc_file = load_catalog(npcs_path)?;
    let world = load_world(world_path)?;
    let art_direction = load_art_direction(art_direction_path)?;

    validate_art_direction(&npc_file.npcs, &art_direction)?;

    let location_by_id: HashMap<u32, &WorldLocation> =
        world.locations.iter().map(|loc| (loc.id, loc)).collect();
    let npc_name_by_id: HashMap<u32, &str> = npc_file
        .npcs
        .iter()
        .map(|entry| (entry.id, entry.name.as_str()))
        .collect();
    let art_by_npc_id: HashMap<u32, &NpcArtDirection> = art_direction
        .npcs
        .iter()
        .map(|direction| (direction.npc_id, direction))
        .collect();

    let mut weak_partial_count = 0;
    let mut strong_partial_count = 0;
    let mut generated = Vec::with_capacity(npc_file.npcs.len());
    for entry in &npc_file.npcs {
        let direction = art_by_npc_id
            .get(&entry.id)
            .expect("validate_art_direction checked coverage");
        let visual_cues = source_visual_cues(entry);
        let source_status = source_metadata_status(&visual_cues);
        if source_status.status == "strong-partial" {
            strong_partial_count += 1;
        } else {
            weak_partial_count += 1;
        }

        let home = location_summary(entry.home, &location_by_id)?;
        let workplace = entry
            .workplace
            .map(|location_id| location_summary(location_id, &location_by_id))
            .transpose()?;
        let schedule_cues = schedule_cues(entry, &location_by_id)?;
        let relationships = relationship_summaries(entry, &npc_name_by_id)?;
        let personality_summary = first_sentence(&entry.personality);
        let prompt_context = PromptContext {
            name: &entry.name,
            age: entry.age,
            occupation: &entry.occupation,
            mood: &entry.mood,
            brief_description: entry.brief_description.as_deref(),
            home_name: &home.name,
            workplace_name: workplace.as_ref().map(|w| w.name.as_str()),
        };
        let portrait_prompt_text =
            portrait_prompt(&art_direction.global_style, &prompt_context, direction);
        let marker_prompt_text =
            marker_prompt(&art_direction.global_style, &prompt_context, direction);
        let pair_prompt_text = pair_prompt(&art_direction.global_style, &prompt_context, direction);

        generated.push(GeneratedNpcArtInput {
            npc_id: entry.id,
            name: entry.name.clone(),
            age: entry.age,
            pronouns: entry.pronouns.clone(),
            occupation: entry.occupation.clone(),
            mood: entry.mood.clone(),
            brief_description: entry.brief_description.clone(),
            home,
            workplace,
            source_personality_summary: personality_summary,
            source_knowledge: entry.knowledge.clone(),
            source_relationships: relationships,
            source_schedule_cues: schedule_cues,
            source_visual_cues: visual_cues,
            source_metadata_status: source_status,
            art_direction: (*direction).clone(),
            pair_prompt: pair_prompt_text,
            portrait_prompt: portrait_prompt_text,
            marker_prompt: marker_prompt_text,
        });
    }

    generated.sort_by_key(|entry| entry.npc_id);
    let dataset = ArtInputDataset {
        schema_version: ART_INPUT_SCHEMA_VERSION,
        source: ArtInputSource {
            npcs_json: stable_source_path(npcs_path)?,
            world_json: stable_source_path(world_path)?,
            art_direction_json: stable_source_path(art_direction_path)?,
            generator: "parish-npc-tool art-inputs",
        },
        global_style: art_direction.global_style.clone(),
        existing_metadata_assessment: ExistingMetadataAssessment {
            npc_count: generated.len(),
            canonical_npc_data_sufficient_without_art_direction: false,
            reason: "npcs.json has age, role, schedule, relationships, and some brief visual cues, but no structured face/hair/clothing/marker contract. Production art requires the reviewed art-direction supplement merged into this export.".to_string(),
            weak_partial_count,
            strong_partial_count,
        },
        fallback: GeneratedFallbackArtInput {
            pair_prompt: fallback_pair_prompt(
                &art_direction.global_style,
                &art_direction.fallback,
            ),
            portrait_prompt: fallback_portrait_prompt(
                &art_direction.global_style,
                &art_direction.fallback,
            ),
            marker_prompt: fallback_marker_prompt(&art_direction.global_style, &art_direction.fallback),
            art_direction: art_direction.fallback,
        },
        npcs: generated,
    };

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create output dir {}", parent.display()))?;
    }
    let body = to_ui_art_json(&dataset)?;
    std::fs::write(output_path, body)
        .with_context(|| format!("write art inputs {}", output_path.display()))?;
    Ok(dataset.existing_metadata_assessment.npc_count)
}

/// Serializes derived UI art JSON using the UI-local Prettier convention.
fn to_ui_art_json<T: Serialize>(value: &T) -> Result<String> {
    let mut buf = Vec::with_capacity(4096);
    let formatter = PrettyFormatter::with_indent(b"\t");
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser).context("serialize UI art JSON")?;
    let mut body = String::from_utf8(buf).context("UI art JSON output is not UTF-8")?;
    body.push('\n');
    Ok(body)
}

fn load_world(path: &Path) -> Result<WorldFile> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("read world {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse world {}", path.display()))
}

fn load_art_direction(path: &Path) -> Result<ArtDirectionFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read art direction {}", path.display()))?;
    let file: ArtDirectionFile = serde_json::from_str(&raw)
        .with_context(|| format!("parse art direction {}", path.display()))?;
    if file.schema_version != ART_DIRECTION_SCHEMA_VERSION {
        bail!(
            "unsupported art direction schema_version {}; expected {}",
            file.schema_version,
            ART_DIRECTION_SCHEMA_VERSION
        );
    }
    Ok(file)
}

fn stable_source_path(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize source path {}", path.display()))?;
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("parish-npc-tool must be nested under parish/crates")
        .canonicalize()
        .context("canonicalize repository root")?;
    let stable = canonical
        .strip_prefix(&repo_root)
        .map(Path::to_path_buf)
        .unwrap_or(canonical);
    Ok(stable.display().to_string())
}

#[derive(Clone, Copy)]
enum IdentityOwner {
    Npc(u32),
    Fallback,
}

struct CohortIdentity {
    owner: IdentityOwner,
    facial_dimensions: [String; 9],
    hair_topology: [String; 4],
}

fn identity_pair(left: IdentityOwner, right: IdentityOwner) -> String {
    match (left, right) {
        (IdentityOwner::Npc(left), IdentityOwner::Npc(right)) => {
            format!("NPC ids {left} and {right}")
        }
        (IdentityOwner::Npc(id), IdentityOwner::Fallback)
        | (IdentityOwner::Fallback, IdentityOwner::Npc(id)) => {
            format!("NPC id {id} and fallback")
        }
        (IdentityOwner::Fallback, IdentityOwner::Fallback) => "fallback and fallback".to_string(),
    }
}

fn register_identity(
    direction: &PortraitDirection,
    owner: IdentityOwner,
    seeds: &mut HashMap<String, IdentityOwner>,
    facial_fingerprints: &mut HashMap<String, IdentityOwner>,
) -> Result<()> {
    let seed = normalize_identity_text(&direction.visual_identity_seed);
    if let Some(existing) = seeds.insert(seed, owner) {
        bail!(
            "duplicate visual_identity_seed for {}",
            identity_pair(existing, owner)
        );
    }

    let dimensions = facial_dimensions(direction);
    let fingerprint = dimensions.join("\u{1f}");
    if let Some(existing) = facial_fingerprints.insert(fingerprint, owner) {
        bail!(
            "duplicate facial geometry fingerprint for {}",
            identity_pair(existing, owner)
        );
    }
    Ok(())
}

fn validate_art_direction(npcs: &[NpcFileEntry], art: &ArtDirectionFile) -> Result<()> {
    let npc_by_id: HashMap<u32, &NpcFileEntry> =
        npcs.iter().map(|entry| (entry.id, entry)).collect();
    let mut seen = HashSet::new();
    let mut seeds = HashMap::new();
    let mut facial_fingerprints = HashMap::new();
    let mut cohort_identities: HashMap<String, Vec<CohortIdentity>> = HashMap::new();
    for direction in &art.npcs {
        let Some(npc) = npc_by_id.get(&direction.npc_id) else {
            bail!(
                "art direction references unknown NPC id {}",
                direction.npc_id
            );
        };
        if let Some(name) = &direction.npc_name
            && name != &npc.name
        {
            bail!(
                "art direction name mismatch for NPC id {}: expected {}, got {}",
                direction.npc_id,
                npc.name,
                name
            );
        }
        if !seen.insert(direction.npc_id) {
            bail!("duplicate art direction for NPC id {}", direction.npc_id);
        }

        validate_portrait_direction(
            &direction.portrait_identity,
            &format!("NPC id {} portrait_identity", direction.npc_id),
        )?;
        validate_marker_direction(
            &direction.marker_identity,
            &format!("NPC id {} marker_identity", direction.npc_id),
        )?;
        register_identity(
            &direction.portrait_identity,
            IdentityOwner::Npc(direction.npc_id),
            &mut seeds,
            &mut facial_fingerprints,
        )?;
        cohort_identities
            .entry(normalize_identity_text(
                &direction.portrait_identity.identity_cohort,
            ))
            .or_default()
            .push(CohortIdentity {
                owner: IdentityOwner::Npc(direction.npc_id),
                facial_dimensions: facial_dimensions(&direction.portrait_identity),
                hair_topology: hair_topology_dimensions(&direction.portrait_identity),
            });
    }

    validate_portrait_direction(
        &art.fallback.portrait_identity,
        "fallback portrait_identity",
    )?;
    validate_marker_direction(&art.fallback.marker_identity, "fallback marker_identity")?;
    register_identity(
        &art.fallback.portrait_identity,
        IdentityOwner::Fallback,
        &mut seeds,
        &mut facial_fingerprints,
    )?;

    let fallback_facial_dimensions = facial_dimensions(&art.fallback.portrait_identity);
    let fallback_hair_topology = hair_topology_dimensions(&art.fallback.portrait_identity);
    for direction in &art.npcs {
        let named_facial_dimensions = facial_dimensions(&direction.portrait_identity);
        let differing_dimensions = named_facial_dimensions
            .iter()
            .zip(&fallback_facial_dimensions)
            .filter(|(named, fallback)| named != fallback)
            .count();
        if differing_dimensions < 4 {
            bail!(
                "fallback and NPC id {} differ in only {} of 9 facial geometry dimensions; at least 4 must differ",
                direction.npc_id,
                differing_dimensions
            );
        }

        let named_hair_topology = hair_topology_dimensions(&direction.portrait_identity);
        let differing_dimensions = named_hair_topology
            .iter()
            .zip(&fallback_hair_topology)
            .filter(|(named, fallback)| named != fallback)
            .count();
        if differing_dimensions < 2 {
            bail!(
                "fallback and NPC id {} differ in only {} of 4 hair topology dimensions; at least 2 must differ",
                direction.npc_id,
                differing_dimensions
            );
        }
    }

    for identities in cohort_identities.values() {
        for (index, left) in identities.iter().enumerate() {
            for right in &identities[index + 1..] {
                let differing_dimensions = left
                    .facial_dimensions
                    .iter()
                    .zip(&right.facial_dimensions)
                    .filter(|(left, right)| left != right)
                    .count();
                if differing_dimensions < 4 {
                    bail!(
                        "same-cohort {} differ in only {} of 9 facial geometry dimensions; at least 4 must differ",
                        identity_pair(left.owner, right.owner),
                        differing_dimensions
                    );
                }

                let differing_dimensions = left
                    .hair_topology
                    .iter()
                    .zip(&right.hair_topology)
                    .filter(|(left, right)| left != right)
                    .count();
                if differing_dimensions < 2 {
                    bail!(
                        "same-cohort {} differ in only {} of 4 hair topology dimensions; at least 2 must differ",
                        identity_pair(left.owner, right.owner),
                        differing_dimensions
                    );
                }
            }
        }
    }

    let missing: Vec<String> = npcs
        .iter()
        .filter(|entry| !seen.contains(&entry.id))
        .map(|entry| format!("{} ({})", entry.id, entry.name))
        .collect();
    if !missing.is_empty() {
        bail!(
            "missing art direction for NPC id(s): {}",
            missing.join(", ")
        );
    }
    Ok(())
}

fn validate_portrait_direction(direction: &PortraitDirection, label: &str) -> Result<()> {
    for (field, value) in [
        (
            "visual_identity_seed",
            direction.visual_identity_seed.as_str(),
        ),
        ("identity_cohort", direction.identity_cohort.as_str()),
        ("apparent_age", direction.apparent_age.as_str()),
        (
            "facial_geometry.face_shape",
            direction.facial_geometry.face_shape.as_str(),
        ),
        (
            "facial_geometry.proportions",
            direction.facial_geometry.proportions.as_str(),
        ),
        (
            "facial_geometry.brow_and_eyes",
            direction.facial_geometry.brow_and_eyes.as_str(),
        ),
        (
            "facial_geometry.nose",
            direction.facial_geometry.nose.as_str(),
        ),
        (
            "facial_geometry.mouth",
            direction.facial_geometry.mouth.as_str(),
        ),
        (
            "facial_geometry.jaw_and_chin",
            direction.facial_geometry.jaw_and_chin.as_str(),
        ),
        (
            "facial_geometry.cheekbones",
            direction.facial_geometry.cheekbones.as_str(),
        ),
        (
            "facial_geometry.hairline",
            direction.facial_geometry.hairline.as_str(),
        ),
        (
            "facial_geometry.age_detail",
            direction.facial_geometry.age_detail.as_str(),
        ),
        ("hair", direction.hair.as_str()),
        (
            "hair_topology.color_and_texture",
            direction.hair_topology.color_and_texture.as_str(),
        ),
        (
            "hair_topology.front.family",
            direction.hair_topology.front.family.as_str(),
        ),
        (
            "hair_topology.front.description",
            direction.hair_topology.front.description.as_str(),
        ),
        (
            "hair_topology.rear.family",
            direction.hair_topology.rear.family.as_str(),
        ),
        (
            "hair_topology.rear.description",
            direction.hair_topology.rear.description.as_str(),
        ),
        (
            "hair_topology.covering.family",
            direction.hair_topology.covering.family.as_str(),
        ),
        (
            "hair_topology.covering.description",
            direction.hair_topology.covering.description.as_str(),
        ),
        (
            "hair_topology.silhouette.family",
            direction.hair_topology.silhouette.family.as_str(),
        ),
        (
            "hair_topology.silhouette.description",
            direction.hair_topology.silhouette.description.as_str(),
        ),
        (
            "hair_topology.loose_details",
            direction.hair_topology.loose_details.as_str(),
        ),
        ("clothing", direction.clothing.as_str()),
        ("pose_expression", direction.pose_expression.as_str()),
    ] {
        if value.trim().is_empty() {
            bail!("{label}.{field} must be nonempty");
        }
    }

    for (field, family) in [
        (
            "hair_topology.front.family",
            direction.hair_topology.front.family.as_str(),
        ),
        (
            "hair_topology.rear.family",
            direction.hair_topology.rear.family.as_str(),
        ),
        (
            "hair_topology.covering.family",
            direction.hair_topology.covering.family.as_str(),
        ),
        (
            "hair_topology.silhouette.family",
            direction.hair_topology.silhouette.family.as_str(),
        ),
    ] {
        if !is_lowercase_kebab_slug(family) {
            bail!("{label}.{field} must be a lowercase kebab slug");
        }
    }

    if direction.distinguishing_features.len() < 2 {
        bail!("{label}.distinguishing_features must contain at least 2 entries");
    }
    let mut features = HashSet::new();
    for feature in &direction.distinguishing_features {
        let normalized = normalize_identity_text(feature);
        if normalized.is_empty() {
            bail!("{label}.distinguishing_features entries must be nonempty");
        }
        if !features.insert(normalized) {
            bail!("{label}.distinguishing_features entries must be unique");
        }
    }
    Ok(())
}

fn validate_marker_direction(direction: &MarkerDirection, label: &str) -> Result<()> {
    for (field, value) in [
        ("silhouette", direction.silhouette.as_str()),
        ("stance", direction.stance.as_str()),
    ] {
        if value.trim().is_empty() {
            bail!("{label}.{field} must be nonempty");
        }
    }

    if direction.readability_cues.len() < 2 {
        bail!("{label}.readability_cues must contain at least 2 entries");
    }
    let mut cue_kinds = HashSet::new();
    let mut cue_descriptions = HashSet::new();
    for cue in &direction.readability_cues {
        if !cue_kinds.insert(cue.kind) {
            bail!("{label}.readability_cues must use distinct cue kinds");
        }
        let normalized = normalize_identity_text(&cue.description);
        if normalized.is_empty() {
            bail!("{label}.readability_cues descriptions must be nonempty");
        }
        if !cue_descriptions.insert(normalized) {
            bail!("{label}.readability_cues descriptions must be unique");
        }
    }

    if direction.tiny_readability_notes.is_empty()
        || direction
            .tiny_readability_notes
            .iter()
            .any(|note| note.trim().is_empty())
    {
        bail!("{label}.tiny_readability_notes must contain nonempty entries");
    }

    Ok(())
}

fn normalize_identity_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn is_lowercase_kebab_slug(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn facial_dimensions(direction: &PortraitDirection) -> [String; 9] {
    let geometry = &direction.facial_geometry;
    [
        &geometry.face_shape,
        &geometry.proportions,
        &geometry.brow_and_eyes,
        &geometry.nose,
        &geometry.mouth,
        &geometry.jaw_and_chin,
        &geometry.cheekbones,
        &geometry.hairline,
        &geometry.age_detail,
    ]
    .map(|value| normalize_identity_text(value))
}

fn hair_topology_dimensions(direction: &PortraitDirection) -> [String; 4] {
    [
        &direction.hair_topology.front.family,
        &direction.hair_topology.rear.family,
        &direction.hair_topology.covering.family,
        &direction.hair_topology.silhouette.family,
    ]
    .map(|value| normalize_identity_text(value))
}

fn location_summary(
    location_id: u32,
    location_by_id: &HashMap<u32, &WorldLocation>,
) -> Result<LocationSummary> {
    let location = location_by_id
        .get(&location_id)
        .with_context(|| format!("missing world location id {location_id}"))?;
    Ok(LocationSummary {
        id: location.id,
        name: location.name.clone(),
        description: location.description_template.as_ref().map(|text| {
            text.replace("{time}", "daylight")
                .replace("{weather}", "weather")
        }),
    })
}

fn relationship_summaries(
    entry: &NpcFileEntry,
    npc_name_by_id: &HashMap<u32, &str>,
) -> Result<Vec<RelationshipSummary>> {
    let mut out = Vec::with_capacity(entry.relationships.len());
    for rel in &entry.relationships {
        let target_name = npc_name_by_id.get(&rel.target_id).with_context(|| {
            format!("NPC {} relates to missing NPC {}", entry.id, rel.target_id)
        })?;
        out.push(RelationshipSummary {
            target_id: rel.target_id,
            target_name: (*target_name).to_string(),
            kind: format!("{:?}", rel.kind),
            strength: rel.strength,
        });
    }
    Ok(out)
}

fn schedule_cues(
    entry: &NpcFileEntry,
    location_by_id: &HashMap<u32, &WorldLocation>,
) -> Result<Vec<ScheduleCue>> {
    let mut cues = BTreeSet::new();
    if let Some(variants) = &entry.seasonal_schedule {
        for variant in variants {
            add_schedule_entries(&variant.entries, location_by_id, &mut cues)?;
        }
    } else if let Some(entries) = &entry.schedule {
        add_schedule_entries(entries, location_by_id, &mut cues)?;
    }
    Ok(cues.into_iter().collect())
}

fn add_schedule_entries(
    entries: &[ScheduleFileEntry],
    location_by_id: &HashMap<u32, &WorldLocation>,
    cues: &mut BTreeSet<ScheduleCue>,
) -> Result<()> {
    for entry in entries {
        let location = location_by_id
            .get(&entry.location)
            .with_context(|| format!("schedule references missing location {}", entry.location))?;
        cues.insert(ScheduleCue {
            location_id: entry.location,
            location_name: location.name.clone(),
            activity: entry.activity.clone(),
        });
    }
    Ok(())
}

fn source_visual_cues(entry: &NpcFileEntry) -> Vec<String> {
    let mut cues = Vec::new();
    cues.push(format!("age: {}", entry.age));
    cues.push(format!("occupation: {}", entry.occupation));
    if let Some(description) = &entry.brief_description {
        cues.push(format!("brief_description: {description}"));
        let lower = description.to_ascii_lowercase();
        for (label, needles) in [
            (
                "body/build",
                &["broad", "lanky", "thin", "small", "strong", "wiry"][..],
            ),
            ("face/eyes", &["eyes", "face", "looking", "look"][..]),
            ("hair", &["hair", "red-haired"][..]),
            (
                "clothing",
                &["apron", "shawl", "dressed", "wrapped", "boots"][..],
            ),
            (
                "prop/activity",
                &[
                    "bar", "counter", "flour", "herb", "loom", "ledger", "baby", "soot", "hands",
                ][..],
            ),
        ] {
            if needles.iter().any(|needle| lower.contains(needle)) {
                cues.push(label.to_string());
            }
        }
    }
    cues
}

fn source_metadata_status(cues: &[String]) -> SourceMetadataStatus {
    let rich_cue_count = cues
        .iter()
        .filter(|cue| {
            matches!(
                cue.as_str(),
                "body/build" | "face/eyes" | "hair" | "clothing" | "prop/activity"
            )
        })
        .count();
    if rich_cue_count >= 2 {
        SourceMetadataStatus {
            canonical_npc_data_sufficient_for_production_art: false,
            status: "strong-partial",
            reason: "The canonical NPC data has useful visual cues, but still lacks a structured production portrait/marker contract.".to_string(),
        }
    } else {
        SourceMetadataStatus {
            canonical_npc_data_sufficient_for_production_art: false,
            status: "weak-partial",
            reason: "The canonical NPC data identifies the character but does not provide enough structured appearance, clothing, or tiny-marker guidance for production art.".to_string(),
        }
    }
}

fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    if let Some((first, _)) = trimmed.split_once('.') {
        format!("{}.", first.trim())
    } else {
        trimmed.to_string()
    }
}

struct PromptContext<'a> {
    name: &'a str,
    age: u8,
    occupation: &'a str,
    mood: &'a str,
    brief_description: Option<&'a str>,
    home_name: &'a str,
    workplace_name: Option<&'a str>,
}

fn portrait_prompt(
    style: &GlobalStyle,
    context: &PromptContext<'_>,
    direction: &NpcArtDirection,
) -> String {
    let prop = direction
        .portrait_identity
        .props
        .first()
        .map(String::as_str)
        .unwrap_or("none");
    format!(
        "Subject identity: {}, age {}, {}; visual identity seed {}; apparent age {}; {} {} Clothing: {}, indicated only with contour, seam, and a few fold lines; expression and pose {}; current mood {}. Optional lower-edge identity cue: {}, at most one simply outlined prop. Canonical context: {}; {}. Hard constraints: one character, head and shoulders only, period-appropriate rural County Roscommon clothing, no text, no label, no border, no card, no UI chrome. Character-specific avoid list: {}.",
        context.name,
        context.age,
        context.occupation,
        direction.portrait_identity.visual_identity_seed,
        direction.portrait_identity.apparent_age,
        portrait_identity_facts(&direction.portrait_identity),
        portrait_style_prefix(style),
        direction.portrait_identity.clothing,
        direction.portrait_identity.pose_expression,
        context.mood,
        prop,
        context
            .brief_description
            .unwrap_or("ordinary parish neighbour"),
        place_context(context),
        direction.avoid.join(", ")
    )
}

fn portrait_style_prefix(style: &GlobalStyle) -> String {
    format!(
        "Artifact and lore: this is a quick observational sketch the player character drew by hand in the margin of their working parish notebook after meeting the subject. It is diegetic notebook ephemera, not a commissioned illustration, formal portrait study, character card, or polished concept painting. Non-negotiable drawing language: sparse, uncolored pen-and-ink line drawing under this visual authority: {} Use one irregular sepia/graphite ink line, economical contours, open shapes, and only a few short loose hatch marks where structurally necessary. Leave most of the face, hair, clothing, and canvas unfilled. No skin-tone fill, white underpainting, cream fill, solid dark garment, gray wash, watercolor, smooth tonal modeling, dense cross-hatching, photorealistic rendering, or glamour. Delivery composition: {}; keep the complete inked drawing between roughly 40 and 60 percent of canvas height with generous empty space on every side. Uninked areas are transparent in the delivery asset so the UI-controlled notebook paper shows through. Setting: {}.",
        first_sentence(&style.style_reference),
        style.source_assets.portrait_source.join(", "),
        style.setting.join(", ")
    )
}

fn marker_prompt(
    style: &GlobalStyle,
    context: &PromptContext<'_>,
    direction: &NpcArtDirection,
) -> String {
    format!(
        "Subject identity: {}, age {}, {}; visual identity seed {}; apparent age {}; {} {} Character-only silhouette: {}. Stance: {}. Empty-hand pose: {}. Tiny-readability cues from the person's body, hair or headwear, clothing, and stance alone: {}. Tiny-readability requirements: {}. Hard constraints: one character only, both hands empty, period-appropriate ordinary rural clothing, simplified face, complete feet, no held or carried objects, tools, books, containers, bundles, furniture, architecture, vegetation, scenery fragments, ground plane, shadow, text, label, border, or UI chrome. Worn clothing and headwear are allowed. Do not illustrate an occupation, workplace, activity, or narrative context around the person. Character-specific avoid list: {}.",
        context.name,
        context.age,
        context.occupation,
        direction.portrait_identity.visual_identity_seed,
        direction.portrait_identity.apparent_age,
        portrait_identity_facts(&direction.portrait_identity),
        marker_style_prefix(style),
        direction.marker_identity.silhouette,
        direction.marker_identity.stance,
        direction.marker_identity.empty_hand_pose.prompt_text(),
        marker_readability_cues(&direction.marker_identity),
        direction.marker_identity.tiny_readability_notes.join(", "),
        direction.avoid.join(", ")
    )
}

fn marker_style_prefix(style: &GlobalStyle) -> String {
    format!(
        "Asset role: one tiny static full-body NPC marker delivered as a transparent character-only cutout for compositing over Rundale's painted world surface. It is not a UI portrait, paper doll, animation sheet, formal character illustration, vignette, or sprite-sheet panel. Visual language: loose hand-inked watercolor miniature under this visual authority: {} Use irregular sepia/graphite contours, a simple readable human silhouette, restrained translucent washes, low facial detail, and only enough clothing folds to identify the person at scene size. Keep the treatment handmade and subordinate to the environment, never glossy, photorealistic, densely rendered, or cut out with a broad halo. Delivery composition: {}. Palette: sepia/graphite line with muted wool gray, bog green, weathered tan, dull brick red, peat brown, and faded indigo only; no saturated primary colors. Setting: {}.",
        first_sentence(&style.style_reference),
        style.source_assets.marker_source.join(", "),
        style.setting.join(", ")
    )
}

fn pair_prompt(
    style: &GlobalStyle,
    context: &PromptContext<'_>,
    direction: &NpcArtDirection,
) -> String {
    let portrait_prop = direction
        .portrait_identity
        .props
        .first()
        .map(String::as_str)
        .unwrap_or("none");
    format!(
        "Identity contract for {}, age {}, {}: visual identity seed {}; apparent age {}; {} Production task: create one identity-locked portrait-and-marker pair in {}, under this visual authority: {} The two renderings must unmistakably be the same person by reproducing every stated identity fact. Clothing: {}; canonical biography cue for clothing and expression only, never for setting, activity, or objects: {}; current mood {}. Left asset, notebook portrait: a quick observational head-and-shoulders sketch the player character drew in their working notebook after meeting this person. Use sparse uncolored sepia/graphite contours, open unfilled shapes, and only a few isolated short structural hatch marks; optional lower-edge prop {}, simply outlined and confined to the left cell. Do not cross-hatch or shade any broad region of the face, hair, neck, scarf, waistcoat, coat, dress, or apron, and do not render a dark garment as a filled or densely hatched mass. Keep the complete ink drawing between 40 and 60 percent of the left cell height with generous empty padding; every uninked interior region must remain provider key, never white, cream, parchment, skin tone, gray, or any other fill. It must not read as a formal illustration or painted portrait. Right asset, painted-world marker: one tiny static full-body transparent character-only cutout designed for compositing into the painted parish scene; silhouette {}; stance {}; empty-hand pose {}; tiny-readability cues from the person's body, hair or headwear, clothing, and stance alone: {}; complete figure roughly 45 percent of the right cell height, acceptable range 40 to 60 percent, with generous key-visible margins; restrained translucent watercolor within loose sepia/graphite contours, simplified face, and complete feet. The right cell must contain the person only: both hands empty; no held or carried objects, tools, books, containers, bundles, furniture, counters, architecture, vegetation, scenery fragments, ground plane, shadow, or extra person. Worn clothing and headwear are allowed. Do not illustrate the canonical biography's occupation, workplace, activity, or narrative context around the marker. Limit painted color to muted wool gray, bog green, weathered tan, dull brick red, peat brown, and faded indigo; no saturated primary colors. Shared constraints: one depiction in each assigned cell, ordinary 1820 rural County Roscommon clothing, no modern or fantasy elements, no text, labels, frames, contact-sheet furniture, or sprite-sheet poses. Never copy a left-cell portrait prop into the marker. Character-specific avoid list: {}.",
        context.name,
        context.age,
        context.occupation,
        direction.portrait_identity.visual_identity_seed,
        direction.portrait_identity.apparent_age,
        portrait_identity_facts(&direction.portrait_identity),
        style.setting.join(", "),
        first_sentence(&style.style_reference),
        direction.portrait_identity.clothing,
        context
            .brief_description
            .unwrap_or("ordinary parish neighbour"),
        context.mood,
        portrait_prop,
        direction.marker_identity.silhouette,
        direction.marker_identity.stance,
        direction.marker_identity.empty_hand_pose.prompt_text(),
        marker_readability_cues(&direction.marker_identity),
        direction.avoid.join(", ")
    )
}

fn fallback_portrait_prompt(style: &GlobalStyle, fallback: &FallbackArtDirection) -> String {
    let prop = fallback
        .portrait_identity
        .props
        .first()
        .map(String::as_str)
        .unwrap_or("none");
    format!(
        "Subject identity: unknown Rundale parish neighbour fallback; visual identity seed {}; apparent age {}; {} {} Clothing: {}, indicated only with contour, seam, and a few fold lines; expression and pose {}. Optional lower-edge identity cue: {}, at most one simply outlined prop. Hard constraints: one anonymous character, head and shoulders only, period-appropriate rural County Roscommon clothing, no text, no label, no border, no card, no UI chrome. Character-specific avoid list: {}.",
        fallback.portrait_identity.visual_identity_seed,
        fallback.portrait_identity.apparent_age,
        portrait_identity_facts(&fallback.portrait_identity),
        portrait_style_prefix(style),
        fallback.portrait_identity.clothing,
        fallback.portrait_identity.pose_expression,
        prop,
        fallback.avoid.join(", ")
    )
}

fn fallback_marker_prompt(style: &GlobalStyle, fallback: &FallbackArtDirection) -> String {
    format!(
        "Subject identity: unknown Rundale parish neighbour fallback; visual identity seed {}; apparent age {}; {} {} Character-only silhouette: {}. Stance: {}. Empty-hand pose: {}. Tiny-readability cues from the person's body, hair or headwear, clothing, and stance alone: {}. Tiny-readability requirements: {}. Hard constraints: one anonymous character only, both hands empty, period-appropriate ordinary rural clothing, simplified face, complete feet, no held or carried objects, tools, books, containers, bundles, furniture, architecture, vegetation, scenery fragments, ground plane, shadow, text, label, border, or UI chrome. Worn clothing and headwear are allowed. Character-specific avoid list: {}.",
        fallback.portrait_identity.visual_identity_seed,
        fallback.portrait_identity.apparent_age,
        portrait_identity_facts(&fallback.portrait_identity),
        marker_style_prefix(style),
        fallback.marker_identity.silhouette,
        fallback.marker_identity.stance,
        fallback.marker_identity.empty_hand_pose.prompt_text(),
        marker_readability_cues(&fallback.marker_identity),
        fallback.marker_identity.tiny_readability_notes.join(", "),
        fallback.avoid.join(", ")
    )
}

fn fallback_pair_prompt(style: &GlobalStyle, fallback: &FallbackArtDirection) -> String {
    let portrait_prop = fallback
        .portrait_identity
        .props
        .first()
        .map(String::as_str)
        .unwrap_or("none");
    format!(
        "Identity contract for an unknown Rundale parish neighbour: visual identity seed {}; apparent age {}; {} Production task: create one identity-locked portrait-and-marker pair in {}, under this visual authority: {} Both renderings must unmistakably depict the same anonymous person by reproducing every stated identity fact without resembling a named NPC. Clothing: {}; expression {}. Left asset, notebook portrait: quick sparse uncolored sepia/graphite head-and-shoulders observation drawn by the player character, open unfilled shapes, only a few isolated short structural hatch marks, optional simply outlined lower-edge prop {} confined to the left cell. Do not cross-hatch or shade any broad region of the face, hair, neck, scarf, waistcoat, coat, dress, or apron, and do not render a dark garment as a filled or densely hatched mass. Keep the complete ink drawing between 40 and 60 percent of the left cell height with generous empty padding; every uninked interior region must remain provider key, never white, cream, parchment, skin tone, gray, or any other fill. Right asset, painted-world marker: one tiny static full-body transparent character-only cutout, silhouette {}; stance {}; empty-hand pose {}; tiny-readability cues from body, hair or headwear, clothing, and stance alone: {}; complete figure roughly 45 percent of the right cell height, acceptable range 40 to 60 percent, with generous key-visible margins; restrained translucent watercolor within loose sepia/graphite contours, simplified face, complete feet. The right cell must contain the person only: both hands empty; no held or carried objects, tools, books, containers, bundles, furniture, counters, architecture, vegetation, scenery fragments, ground plane, shadow, or extra person. Worn clothing and headwear are allowed. Limit painted color to muted wool gray, bog green, weathered tan, dull brick red, peat brown, and faded indigo; no saturated primary colors. Shared constraints: one depiction in each assigned cell, ordinary 1820 rural County Roscommon clothing, no modern or fantasy elements, no text, labels, frames, contact-sheet furniture, or sprite-sheet poses. Never copy a left-cell portrait prop into the marker. Character-specific avoid list: {}.",
        fallback.portrait_identity.visual_identity_seed,
        fallback.portrait_identity.apparent_age,
        portrait_identity_facts(&fallback.portrait_identity),
        style.setting.join(", "),
        first_sentence(&style.style_reference),
        fallback.portrait_identity.clothing,
        fallback.portrait_identity.pose_expression,
        portrait_prop,
        fallback.marker_identity.silhouette,
        fallback.marker_identity.stance,
        fallback.marker_identity.empty_hand_pose.prompt_text(),
        marker_readability_cues(&fallback.marker_identity),
        fallback.avoid.join(", ")
    )
}

fn marker_readability_cues(direction: &MarkerDirection) -> String {
    direction
        .readability_cues
        .iter()
        .map(|cue| format!("{}: {}", cue.kind.prompt_text(), cue.description))
        .collect::<Vec<_>>()
        .join("; ")
}

fn portrait_identity_facts(direction: &PortraitDirection) -> String {
    let geometry = &direction.facial_geometry;
    format!(
        "Explicit facial geometry: face shape {}; proportions {}; brow and eyes {}; nose {}; mouth {}; jaw and chin {}; cheekbones {}; hairline {}; age detail {}. Distinguishing features: {}. Hair: {}.",
        geometry.face_shape,
        geometry.proportions,
        geometry.brow_and_eyes,
        geometry.nose,
        geometry.mouth,
        geometry.jaw_and_chin,
        geometry.cheekbones,
        geometry.hairline,
        geometry.age_detail,
        direction.distinguishing_features.join(", "),
        direction.hair
    )
}

fn place_context(context: &PromptContext<'_>) -> String {
    match context.workplace_name {
        Some(workplace) if workplace != context.home_name => {
            format!("home at {}; works at {}", context.home_name, workplace)
        }
        Some(workplace) => format!("home/workplace at {workplace}"),
        None => format!("home at {}", context.home_name),
    }
}
