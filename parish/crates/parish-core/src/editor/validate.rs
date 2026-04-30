//! Cross-reference validator for a loaded [`EditorModSnapshot`].
//!
//! Runs *after* per-file parsing so it only sees successfully parsed data.
//! Issues accumulate onto the snapshot's existing [`ValidationReport`] so
//! parse errors and cross-reference errors appear in a single list.

use std::collections::HashSet;

use parish_world::graph::WorldGraph;

use super::types::{
    EditorDoc, EditorModSnapshot, ValidationCategory, ValidationIssue, ValidationReport,
    ValidationSeverity,
};

/// Runs all cross-reference checks against `snapshot` and accumulates the
/// results onto `snapshot.validation`.
pub fn validate_snapshot(snapshot: &mut EditorModSnapshot) {
    let mut report = std::mem::take(&mut snapshot.validation);

    validate_world_graph(snapshot, &mut report);
    validate_npc_location_refs(snapshot, &mut report);
    validate_npc_relationships(snapshot, &mut report);
    validate_npc_schedules(snapshot, &mut report);
    validate_associated_npcs(snapshot, &mut report);
    validate_location_ids_unique(snapshot, &mut report);
    validate_npc_ids_unique(snapshot, &mut report);

    snapshot.validation = report;
}

/// Re-runs [`WorldGraph::validate`] against the snapshot's locations.
///
/// Uses the round-trip `load_from_str` path so we get every orphan,
/// non-bidirectional-edge, and bad-target error in one shot.
fn validate_world_graph(snapshot: &EditorModSnapshot, report: &mut ValidationReport) {
    #[derive(serde::Serialize)]
    struct WorldFileOut<'a> {
        locations: &'a [parish_world::graph::LocationData],
    }
    let out = WorldFileOut {
        locations: &snapshot.locations,
    };
    // Round-trip through JSON so we exercise the same code path as the
    // game loader. This is cheap (n << 100 locations) and guarantees
    // validation parity.
    let Ok(json) = serde_json::to_string(&out) else {
        report.push(ValidationIssue {
            category: ValidationCategory::World,
            severity: ValidationSeverity::Error,
            doc: EditorDoc::World,
            field_path: "locations".into(),
            message: "failed to serialize world locations for validation".into(),
            context: None,
        });
        return;
    };
    match WorldGraph::load_from_str(&json) {
        Ok(_) => {}
        Err(e) => {
            report.push(ValidationIssue {
                category: ValidationCategory::World,
                severity: ValidationSeverity::Error,
                doc: EditorDoc::World,
                field_path: "locations".into(),
                message: e.to_string(),
                context: None,
            });
        }
    }
}

fn validate_npc_location_refs(snapshot: &EditorModSnapshot, report: &mut ValidationReport) {
    let location_ids: HashSet<u32> = snapshot.locations.iter().map(|l| l.id.0).collect();
    for (idx, npc) in snapshot.npcs.npcs.iter().enumerate() {
        if !location_ids.contains(&npc.home) {
            report.push(ValidationIssue {
                category: ValidationCategory::Npc,
                severity: ValidationSeverity::Error,
                doc: EditorDoc::Npcs,
                field_path: format!("npcs[{idx}].home"),
                message: format!(
                    "{} home references nonexistent location id {}",
                    npc.name, npc.home
                ),
                context: Some(npc.home.to_string()),
            });
        }
        if let Some(workplace) = npc.workplace
            && !location_ids.contains(&workplace)
        {
            report.push(ValidationIssue {
                category: ValidationCategory::Npc,
                severity: ValidationSeverity::Error,
                doc: EditorDoc::Npcs,
                field_path: format!("npcs[{idx}].workplace"),
                message: format!(
                    "{} workplace references nonexistent location id {}",
                    npc.name, workplace
                ),
                context: Some(workplace.to_string()),
            });
        }
    }
}

fn validate_npc_relationships(snapshot: &EditorModSnapshot, report: &mut ValidationReport) {
    let npc_ids: HashSet<u32> = snapshot.npcs.npcs.iter().map(|n| n.id).collect();
    for (idx, npc) in snapshot.npcs.npcs.iter().enumerate() {
        for (rel_idx, rel) in npc.relationships.iter().enumerate() {
            if !npc_ids.contains(&rel.target_id) {
                report.push(ValidationIssue {
                    category: ValidationCategory::Relationship,
                    severity: ValidationSeverity::Error,
                    doc: EditorDoc::Npcs,
                    field_path: format!("npcs[{idx}].relationships[{rel_idx}].target_id"),
                    message: format!(
                        "{} has a relationship with nonexistent NPC id {}",
                        npc.name, rel.target_id
                    ),
                    context: Some(rel.target_id.to_string()),
                });
            }
            if !(-1.0..=1.0).contains(&rel.strength) {
                report.push(ValidationIssue {
                    category: ValidationCategory::Relationship,
                    severity: ValidationSeverity::Warning,
                    doc: EditorDoc::Npcs,
                    field_path: format!("npcs[{idx}].relationships[{rel_idx}].strength"),
                    message: format!(
                        "{} → NPC {} relationship strength {} is outside the -1.0..=1.0 range",
                        npc.name, rel.target_id, rel.strength
                    ),
                    context: Some(rel.strength.to_string()),
                });
            }
        }
    }
}

fn validate_npc_schedules(snapshot: &EditorModSnapshot, report: &mut ValidationReport) {
    let location_ids: HashSet<u32> = snapshot.locations.iter().map(|l| l.id.0).collect();
    for (idx, npc) in snapshot.npcs.npcs.iter().enumerate() {
        // Legacy flat schedule.
        if let Some(entries) = &npc.schedule {
            for (e_idx, entry) in entries.iter().enumerate() {
                schedule_entry_checks(
                    &format!("npcs[{idx}].schedule[{e_idx}]"),
                    &npc.name,
                    entry,
                    &location_ids,
                    report,
                );
            }
        }
        // New season-aware schedule.
        if let Some(variants) = &npc.seasonal_schedule {
            for (v_idx, variant) in variants.iter().enumerate() {
                for (e_idx, entry) in variant.entries.iter().enumerate() {
                    schedule_entry_checks(
                        &format!("npcs[{idx}].seasonal_schedule[{v_idx}].entries[{e_idx}]"),
                        &npc.name,
                        entry,
                        &location_ids,
                        report,
                    );
                }
            }
        }
    }
}

fn schedule_entry_checks(
    field_path: &str,
    npc_name: &str,
    entry: &parish_npc::ScheduleFileEntry,
    location_ids: &HashSet<u32>,
    report: &mut ValidationReport,
) {
    if !location_ids.contains(&entry.location) {
        report.push(ValidationIssue {
            category: ValidationCategory::Schedule,
            severity: ValidationSeverity::Error,
            doc: EditorDoc::Npcs,
            field_path: format!("{field_path}.location"),
            message: format!(
                "{}'s schedule references nonexistent location id {}",
                npc_name, entry.location
            ),
            context: Some(entry.location.to_string()),
        });
    }
    if entry.start_hour > 23 || entry.end_hour > 23 {
        report.push(ValidationIssue {
            category: ValidationCategory::Schedule,
            severity: ValidationSeverity::Error,
            doc: EditorDoc::Npcs,
            field_path: field_path.to_string(),
            message: format!(
                "{}'s schedule hour out of range (got start={}, end={}; expected 0..=23)",
                npc_name, entry.start_hour, entry.end_hour
            ),
            context: None,
        });
    }
}

fn validate_associated_npcs(snapshot: &EditorModSnapshot, report: &mut ValidationReport) {
    let npc_ids: HashSet<u32> = snapshot.npcs.npcs.iter().map(|n| n.id).collect();
    for (idx, loc) in snapshot.locations.iter().enumerate() {
        for (a_idx, assoc) in loc.associated_npcs.iter().enumerate() {
            if !npc_ids.contains(&assoc.0) {
                report.push(ValidationIssue {
                    category: ValidationCategory::World,
                    severity: ValidationSeverity::Warning,
                    doc: EditorDoc::World,
                    field_path: format!("locations[{idx}].associated_npcs[{a_idx}]"),
                    message: format!("{} references nonexistent NPC id {}", loc.name, assoc.0),
                    context: Some(assoc.0.to_string()),
                });
            }
        }
    }
}

fn validate_location_ids_unique(snapshot: &EditorModSnapshot, report: &mut ValidationReport) {
    let mut seen = HashSet::new();
    for (idx, loc) in snapshot.locations.iter().enumerate() {
        if !seen.insert(loc.id.0) {
            report.push(ValidationIssue {
                category: ValidationCategory::World,
                severity: ValidationSeverity::Error,
                doc: EditorDoc::World,
                field_path: format!("locations[{idx}].id"),
                message: format!("duplicate location id {}", loc.id.0),
                context: Some(loc.id.0.to_string()),
            });
        }
    }
}

fn validate_npc_ids_unique(snapshot: &EditorModSnapshot, report: &mut ValidationReport) {
    let mut seen = HashSet::new();
    for (idx, npc) in snapshot.npcs.npcs.iter().enumerate() {
        if !seen.insert(npc.id) {
            report.push(ValidationIssue {
                category: ValidationCategory::Npc,
                severity: ValidationSeverity::Error,
                doc: EditorDoc::Npcs,
                field_path: format!("npcs[{idx}].id"),
                message: format!("duplicate NPC id {}", npc.id),
                context: Some(npc.id.to_string()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::mod_io::load_mod_snapshot;
    use std::path::PathBuf;

    #[test]
    fn rundale_validates_clean() {
        let root = PathBuf::from("../../../mods/rundale");
        if !root.exists() {
            return;
        }
        let snapshot = load_mod_snapshot(&root).unwrap();
        assert!(
            snapshot.validation.errors.is_empty(),
            "rundale should validate clean, got errors: {:?}",
            snapshot.validation.errors
        );
    }
}
