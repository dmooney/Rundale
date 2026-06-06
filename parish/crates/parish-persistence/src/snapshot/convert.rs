//! Conversion helpers between live NPC types and snapshot structs.
//!
//! [`NpcSnapshot::from_npc`] captures a live [`Npc`](parish_npc::Npc) into a
//! serializable snapshot. [`NpcSnapshot::into_npc`] restores it. Both use
//! exhaustive destructuring so that adding a new field to `Npc` or
//! `NpcSnapshot` produces a compile error until the mapping is updated.

use parish_npc::Npc;

use super::types::NpcSnapshot;

impl NpcSnapshot {
    /// Captures a snapshot from a live NPC.
    ///
    /// Uses exhaustive destructuring so that adding a new field to [`Npc`]
    /// produces a compile error here until it is either persisted or explicitly
    /// excluded with a `_field: _` binding and a comment explaining why.
    pub fn from_npc(npc: &Npc) -> Self {
        // Exhaustive destructuring — no `..`. Every field of `Npc` must be
        // listed. To intentionally exclude a field from persistence, bind it
        // as `field_name: _` and add an "intentionally not persisted" comment.
        let Npc {
            id,
            name,
            brief_description,
            age,
            occupation,
            personality,
            pronouns,
            intelligence,
            location,
            mood,
            home,
            workplace,
            schedule,
            relationships,
            memory,
            long_term_memory,
            knowledge,
            state,
            deflated_summary,
            reaction_log: _, // intentionally not persisted: transient runtime state, reset to default on load
            last_activity,
            is_ill,
            doom,
            banshee_heralded,
        } = npc;

        Self {
            id: *id,
            name: name.clone(),
            brief_description: brief_description.clone(),
            age: *age,
            occupation: occupation.clone(),
            personality: personality.clone(),
            pronouns: pronouns.clone(),
            intelligence: *intelligence,
            location: *location,
            mood: mood.clone(),
            home: *home,
            workplace: *workplace,
            schedule: schedule.clone(),
            relationships: relationships.clone(),
            memory: memory.clone(),
            long_term_memory: long_term_memory.clone(),
            knowledge: knowledge.clone(),
            state: state.clone(),
            last_activity: last_activity.clone(),
            is_ill: *is_ill,
            doom: *doom,
            banshee_heralded: *banshee_heralded,
            // #338: previously hard-coded to None, erasing the
            // demotion summary on every save/load cycle. Round-tripped
            // through NpcSnapshot.deflated_summary now.
            deflated_summary: deflated_summary.clone(),
        }
    }

    /// Restores the snapshot into a live NPC.
    ///
    /// Uses exhaustive destructuring so that adding a new field to
    /// [`NpcSnapshot`] produces a compile error here until it is mapped back
    /// to [`Npc`] or explicitly excluded.
    pub fn into_npc(self) -> Npc {
        // Exhaustive destructuring — no `..`.
        let NpcSnapshot {
            id,
            name,
            brief_description,
            age,
            occupation,
            personality,
            pronouns,
            intelligence,
            location,
            mood,
            home,
            workplace,
            schedule,
            relationships,
            memory,
            long_term_memory,
            knowledge,
            state,
            last_activity,
            is_ill,
            doom,
            banshee_heralded,
            deflated_summary,
        } = self;

        Npc {
            id,
            name,
            brief_description,
            age,
            occupation,
            personality,
            pronouns,
            intelligence,
            location,
            mood,
            home,
            workplace,
            schedule,
            relationships,
            memory,
            long_term_memory,
            knowledge,
            state,
            last_activity,
            is_ill,
            doom,
            banshee_heralded,
            // #338: previously hard-coded to None, erasing the
            // demotion summary on every save/load cycle. Round-tripped
            // through NpcSnapshot.deflated_summary now.
            deflated_summary,
            // intentionally not persisted: transient runtime state, reset to default on load
            reaction_log: parish_npc::reactions::ReactionLog::default(),
        }
    }
}
