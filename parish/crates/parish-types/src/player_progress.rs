//! Durable player-task progression.
//!
//! The task ledger deliberately records only authoritative facts supplied by
//! the engine: who assigned the task, where it was assigned, and game-clock
//! timestamps. Natural-language player actions may start an unambiguous task,
//! but they never infer task completion.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{LocationId, NpcId};

/// Maximum number of task records retained in the player's durable ledger.
///
/// Completed history is evicted oldest-first when necessary. Assigned and
/// in-progress tasks are never evicted to make room for a new assignment.
pub const MAX_PLAYER_TASKS: usize = 128;

/// Maximum number of Unicode scalar values retained in a task description.
pub const MAX_TASK_DESCRIPTION_CHARS: usize = 240;

/// Maximum number of Unicode scalar values retained from a matching action.
pub const MAX_TASK_ACTION_CHARS: usize = 240;

/// Stable, monotonically increasing identifier for a player task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerTaskId(pub u64);

/// Lifecycle state of a player task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// The NPC assigned the task, but the player has not begun it.
    #[default]
    Assigned,
    /// The player performed an unambiguous matching action at the task site.
    InProgress,
    /// The engine explicitly confirmed the task's completion.
    Completed,
}

/// A single authoritative entry in the player's task ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerTask {
    /// Stable task identifier.
    pub id: PlayerTaskId,
    /// Bounded, nonblank description of the assigned work.
    pub description: String,
    /// NPC who assigned the task.
    pub assigned_by: NpcId,
    /// Location where the task can be advanced.
    pub location: LocationId,
    /// Game time when the task was assigned.
    pub assigned_at: DateTime<Utc>,
    /// Current authoritative status.
    #[serde(default)]
    pub status: TaskStatus,
    /// Game time when a matching action first started the task.
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    /// Game time when the engine explicitly completed the task.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// Most recent bounded action accepted as relevant to this task.
    #[serde(default)]
    pub last_matching_action: Option<String>,
}

/// Errors returned when a task cannot safely be assigned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlayerProgressError {
    /// Whitespace-only descriptions do not create tasks.
    #[error("task description must not be blank")]
    BlankDescription,
    /// The bounded ledger is full of active tasks, which may not be evicted.
    #[error("task ledger is full with {capacity} active tasks")]
    ActiveTaskCapacity { capacity: usize },
    /// The monotonic task identifier space has been exhausted.
    #[error("task identifier space is exhausted")]
    IdExhausted,
}

/// Durable, bounded player-task state.
///
/// The container-level serde default keeps old save files compatible when the
/// entire ledger or its monotonic counter is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerProgress {
    tasks: Vec<PlayerTask>,
    next_task_id: u64,
}

impl Default for PlayerProgress {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            next_task_id: 1,
        }
    }
}

impl PlayerProgress {
    /// Assigns a new task using authoritative engine context.
    ///
    /// The description is trimmed and bounded. When the history is at
    /// capacity, the oldest completed task is discarded; active tasks are
    /// never evicted. Repeating the same bounded description
    /// (case-insensitively), assigning NPC, and location returns the existing
    /// non-completed task id without consuming another id.
    pub fn assign_task(
        &mut self,
        description: &str,
        assigned_by: NpcId,
        location: LocationId,
        assigned_at: DateTime<Utc>,
    ) -> Result<PlayerTaskId, PlayerProgressError> {
        let description = bounded_nonblank(description, MAX_TASK_DESCRIPTION_CHARS)
            .ok_or(PlayerProgressError::BlankDescription)?;

        if let Some(existing) = self.tasks.iter().find(|task| {
            task.status != TaskStatus::Completed
                && task.assigned_by == assigned_by
                && task.location == location
                && task.description.to_lowercase() == description.to_lowercase()
        }) {
            return Ok(existing.id);
        }

        // Compute the next id before pruning completed history. A transitional
        // save that has tasks but no counter must not reuse the id of a record
        // evicted by this assignment.
        let after_existing = self
            .tasks
            .iter()
            .map(|task| task.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(PlayerProgressError::IdExhausted)?;
        let raw_id = self.next_task_id.max(after_existing).max(1);
        let following_id = raw_id
            .checked_add(1)
            .ok_or(PlayerProgressError::IdExhausted)?;

        while self.tasks.len() >= MAX_PLAYER_TASKS {
            let Some(completed_index) = self
                .tasks
                .iter()
                .position(|task| task.status == TaskStatus::Completed)
            else {
                return Err(PlayerProgressError::ActiveTaskCapacity {
                    capacity: MAX_PLAYER_TASKS,
                });
            };
            self.tasks.remove(completed_index);
        }

        self.next_task_id = following_id;
        let id = PlayerTaskId(raw_id);

        self.tasks.push(PlayerTask {
            id,
            description,
            assigned_by,
            location,
            assigned_at,
            status: TaskStatus::Assigned,
            started_at: None,
            completed_at: None,
            last_matching_action: None,
        });
        Ok(id)
    }

    /// Returns every retained task in assignment order.
    pub fn tasks(&self) -> &[PlayerTask] {
        &self.tasks
    }

    /// Returns assigned and in-progress tasks in assignment order.
    pub fn active_tasks(&self) -> impl Iterator<Item = &PlayerTask> {
        self.tasks
            .iter()
            .filter(|task| task.status != TaskStatus::Completed)
    }

    /// Looks up a retained task by id.
    pub fn task(&self, id: PlayerTaskId) -> Option<&PlayerTask> {
        self.tasks.iter().find(|task| task.id == id)
    }

    /// Advances one unambiguous, same-location assigned task to in-progress.
    ///
    /// The action must overlap a task description on at least one meaningful
    /// token. If several tasks are eligible, that overlap must identify a
    /// unique highest-scoring description.
    ///
    /// This method intentionally has no path to [`TaskStatus::Completed`].
    /// Phrases such as "I set to work" can start a task, but never finish one.
    pub fn advance_assigned_task(
        &mut self,
        action: &str,
        location: LocationId,
        started_at: DateTime<Utc>,
    ) -> Option<PlayerTaskId> {
        let action = bounded_nonblank(action, MAX_TASK_ACTION_CHARS)?;
        let candidates: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| {
                (task.status == TaskStatus::Assigned && task.location == location).then_some(index)
            })
            .collect();

        let selected = select_unique_task_by_overlap(&self.tasks, &candidates, &action)?;

        let task = &mut self.tasks[selected];
        task.status = TaskStatus::InProgress;
        task.started_at = Some(started_at);
        task.last_matching_action = Some(action);
        Some(task.id)
    }

    /// Explicitly completes an in-progress task.
    ///
    /// Completion requires an exact task id, the task's location, and a
    /// nonblank engine-confirmed action. It is never inferred by
    /// [`Self::advance_assigned_task`].
    pub fn complete_task_explicitly(
        &mut self,
        id: PlayerTaskId,
        action: &str,
        location: LocationId,
        completed_at: DateTime<Utc>,
    ) -> bool {
        let Some(action) = bounded_nonblank(action, MAX_TASK_ACTION_CHARS) else {
            return false;
        };
        let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) else {
            return false;
        };
        if task.status != TaskStatus::InProgress || task.location != location {
            return false;
        }

        task.status = TaskStatus::Completed;
        task.completed_at = Some(completed_at);
        task.last_matching_action = Some(action);
        true
    }

    /// Number of retained task records, including completed history.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the ledger contains no task records.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

fn bounded_nonblank(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(max_chars).collect())
}

fn select_unique_task_by_overlap(
    tasks: &[PlayerTask],
    candidates: &[usize],
    action: &str,
) -> Option<usize> {
    let action_tokens = meaningful_tokens(action);
    if action_tokens.is_empty() {
        return None;
    }

    let mut scored: Vec<(usize, usize)> = candidates
        .iter()
        .copied()
        .map(|index| {
            let description_tokens = meaningful_tokens(&tasks[index].description);
            let score = action_tokens.intersection(&description_tokens).count();
            (index, score)
        })
        .collect();
    scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));

    let (best_index, best_score) = *scored.first()?;
    if best_score == 0 || scored.get(1).is_some_and(|(_, score)| *score == best_score) {
        return None;
    }
    Some(best_index)
}

fn meaningful_tokens(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|token| token.chars().count() >= 3)
        .filter(|token| !is_action_stop_word(token))
        .collect()
}

fn is_action_stop_word(token: &str) -> bool {
    matches!(
        token,
        "and"
            | "for"
            | "from"
            | "help"
            | "into"
            | "mine"
            | "myself"
            | "now"
            | "set"
            | "start"
            | "started"
            | "starting"
            | "task"
            | "the"
            | "this"
            | "that"
            | "their"
            | "then"
            | "there"
            | "they"
            | "with"
            | "work"
            | "worked"
            | "working"
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, hour, 0, 0).unwrap()
    }

    #[test]
    fn assignment_records_authoritative_context_and_monotonic_id() {
        let mut progress = PlayerProgress::default();

        let first = progress
            .assign_task(
                "  Dig over the potato patch.  ",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();
        let second = progress
            .assign_task("Mend the west wall.", NpcId(8), LocationId(9), at(11))
            .unwrap();

        assert_eq!(first, PlayerTaskId(1));
        assert_eq!(second, PlayerTaskId(2));
        let task = progress.task(first).unwrap();
        assert_eq!(task.description, "Dig over the potato patch.");
        assert_eq!(task.assigned_by, NpcId(7));
        assert_eq!(task.location, LocationId(9));
        assert_eq!(task.assigned_at, at(10));
        assert_eq!(task.status, TaskStatus::Assigned);
        assert_eq!(progress.active_tasks().count(), 2);
    }

    #[test]
    fn blank_assignment_is_rejected_without_consuming_an_id() {
        let mut progress = PlayerProgress::default();

        assert_eq!(
            progress.assign_task(" \n\t ", NpcId(7), LocationId(9), at(10)),
            Err(PlayerProgressError::BlankDescription)
        );
        assert!(progress.is_empty());
        assert_eq!(
            progress
                .assign_task("Real work", NpcId(7), LocationId(9), at(10))
                .unwrap(),
            PlayerTaskId(1)
        );
    }

    #[test]
    fn repeated_active_assignment_is_idempotent_without_consuming_an_id() {
        let mut progress = PlayerProgress::default();
        let first = progress
            .assign_task(
                "Dig over the potato patch.",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();
        let repeated = progress
            .assign_task(
                "  dig over the potato patch.  ",
                NpcId(7),
                LocationId(9),
                at(11),
            )
            .unwrap();

        assert_eq!(repeated, first);
        assert_eq!(progress.len(), 1);
        assert_eq!(
            progress
                .assign_task("Mend the west wall.", NpcId(7), LocationId(9), at(11),)
                .unwrap(),
            PlayerTaskId(2),
            "idempotent reassignment must not consume an id"
        );
    }

    #[test]
    fn exact_potato_patch_action_starts_task_without_completing_it() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task(
                "Dig over the potato patch.",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task(
                "I set to work digging over the potato patch.",
                LocationId(9),
                at(11),
            ),
            Some(id)
        );
        let task = progress.task(id).unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.started_at, Some(at(11)));
        assert_eq!(task.completed_at, None);
        assert_eq!(
            task.last_matching_action.as_deref(),
            Some("I set to work digging over the potato patch.")
        );

        assert_eq!(
            progress.advance_assigned_task("I set to work.", LocationId(9), at(12)),
            None
        );
        assert_eq!(progress.task(id).unwrap().status, TaskStatus::InProgress);
    }

    #[test]
    fn task_does_not_advance_at_the_wrong_location() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task(
                "Dig over the potato patch.",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task("I dig over the potato patch.", LocationId(10), at(11)),
            None
        );
        assert_eq!(progress.task(id).unwrap().status, TaskStatus::Assigned);
    }

    #[test]
    fn unrelated_action_does_not_start_the_only_task_at_a_location() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task(
                "Dig over the potato patch.",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task("I sing a song.", LocationId(9), at(11)),
            None
        );
        assert_eq!(progress.task(id).unwrap().status, TaskStatus::Assigned);
        assert_eq!(progress.task(id).unwrap().started_at, None);
    }

    #[test]
    fn ambiguous_generic_action_does_not_choose_between_tasks() {
        let mut progress = PlayerProgress::default();
        let potato = progress
            .assign_task(
                "Dig over the potato patch.",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();
        let wall = progress
            .assign_task("Mend the stone wall.", NpcId(8), LocationId(9), at(10))
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task("I set to work.", LocationId(9), at(11)),
            None
        );
        assert_eq!(progress.task(potato).unwrap().status, TaskStatus::Assigned);
        assert_eq!(progress.task(wall).unwrap().status, TaskStatus::Assigned);

        assert_eq!(
            progress.advance_assigned_task(
                "I set to work digging the potato patch.",
                LocationId(9),
                at(11),
            ),
            Some(potato)
        );
        assert_eq!(
            progress.task(potato).unwrap().status,
            TaskStatus::InProgress
        );
        assert_eq!(progress.task(wall).unwrap().status, TaskStatus::Assigned);
    }

    #[test]
    fn descriptions_actions_and_completed_history_are_bounded() {
        let mut progress = PlayerProgress::default();
        let long_description = "é".repeat(MAX_TASK_DESCRIPTION_CHARS + 20);
        let first = progress
            .assign_task(&long_description, NpcId(1), LocationId(1), at(8))
            .unwrap();
        assert_eq!(
            progress.task(first).unwrap().description.chars().count(),
            MAX_TASK_DESCRIPTION_CHARS
        );

        for index in 0..MAX_PLAYER_TASKS {
            let assigned = if index == 0 {
                first
            } else {
                progress
                    .assign_task(
                        &format!("Task number {index}"),
                        NpcId(1),
                        LocationId(index as u32 + 2),
                        at(8),
                    )
                    .unwrap()
            };
            let location = progress.task(assigned).unwrap().location;
            let matching_action = format!(
                "{} {}",
                progress.task(assigned).unwrap().description,
                "x".repeat(MAX_TASK_ACTION_CHARS + 20)
            );
            assert_eq!(
                progress.advance_assigned_task(&matching_action, location, at(9)),
                Some(assigned)
            );
            assert_eq!(
                progress
                    .task(assigned)
                    .unwrap()
                    .last_matching_action
                    .as_deref()
                    .unwrap()
                    .chars()
                    .count(),
                MAX_TASK_ACTION_CHARS
            );
            assert!(progress.complete_task_explicitly(assigned, "done", location, at(10)));
        }
        assert_eq!(progress.len(), MAX_PLAYER_TASKS);

        let replacement = progress
            .assign_task("A fresh assignment", NpcId(2), LocationId(2), at(11))
            .unwrap();
        assert_eq!(replacement, PlayerTaskId(MAX_PLAYER_TASKS as u64 + 1));
        assert_eq!(progress.len(), MAX_PLAYER_TASKS);
        assert!(
            progress.task(first).is_none(),
            "oldest completed task evicted"
        );
        assert!(progress.task(replacement).is_some());
    }

    #[test]
    fn full_active_ledger_rejects_assignment_without_evicting_tasks() {
        let mut progress = PlayerProgress::default();
        for index in 0..MAX_PLAYER_TASKS {
            progress
                .assign_task(
                    &format!("Active task {index}"),
                    NpcId(1),
                    LocationId(index as u32 + 1),
                    at(8),
                )
                .unwrap();
        }
        let ids_before: Vec<_> = progress.tasks().iter().map(|task| task.id).collect();

        assert_eq!(
            progress.assign_task("One too many", NpcId(2), LocationId(2), at(9)),
            Err(PlayerProgressError::ActiveTaskCapacity {
                capacity: MAX_PLAYER_TASKS
            })
        );
        assert_eq!(
            progress
                .tasks()
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            ids_before
        );
    }

    #[test]
    fn missing_counter_is_reconciled_with_existing_ids() {
        let json = r#"{
            "tasks": [{
                "id": 41,
                "description": "Existing task",
                "assigned_by": 2,
                "location": 3,
                "assigned_at": "1820-03-20T08:00:00Z",
                "status": "assigned"
            }]
        }"#;
        let mut progress: PlayerProgress = serde_json::from_str(json).unwrap();

        let id = progress
            .assign_task("Later task", NpcId(3), LocationId(3), at(9))
            .unwrap();
        assert_eq!(id, PlayerTaskId(42));
    }
}
