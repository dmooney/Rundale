//! Conversion helpers between live NPC types and snapshot structs.
//!
//! [`NpcSnapshot::from_npc`] captures a live [`Npc`](parish_npc::Npc) into a
//! serializable snapshot. [`NpcSnapshot::into_npc`] restores it. Both use
//! exhaustive destructuring so that adding a new field to `Npc` or
//! `NpcSnapshot` produces a compile error until the mapping is updated.

use parish_npc::{Npc, NpcPersistedFields};

use super::types::NpcSnapshot;

impl NpcSnapshot {
    /// Captures a snapshot from a live NPC.
    ///
    /// Uses exhaustive destructuring so that adding a new field to [`Npc`]
    /// produces a compile error here until it is either persisted or explicitly
    /// excluded with a `_field: _` binding and a comment explaining why.
    pub fn from_npc(npc: &Npc) -> Self {
        // `Npc::persisted_fields` performs the exhaustive live-state
        // destructure inside parish-npc, where the grounding-sensitive fields
        // remain inaccessible to downstream mutation.
        let NpcPersistedFields {
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
            reaction_log,
            last_activity,
            is_ill,
            doom,
            banshee_heralded,
        } = npc.persisted_fields();

        Self {
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
            reaction_log,
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
            reaction_log,
        } = self;

        Npc::from_persisted_fields(NpcPersistedFields {
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
            reaction_log,
        })
    }
}
