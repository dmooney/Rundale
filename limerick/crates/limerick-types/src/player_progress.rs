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
    /// Task id zero is reserved and cannot appear in authoritative state.
    #[error("task identifier must be nonzero")]
    InvalidTaskId,
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
                && task_identity_key(&task.description) == task_identity_key(&description)
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

    /// Hydrates an authoritative post-mutation task payload during journal replay.
    ///
    /// Existing ids are replaced in place so replaying the same event is
    /// idempotent and assignment order is stable. A stale or equal-lifecycle
    /// payload cannot move a task backwards or rewrite immutable assignment
    /// fields. Absent ids use the same bounded-ledger policy as
    /// [`Self::assign_task`], and the monotonic id counter is advanced past
    /// every accepted replay id.
    ///
    /// Returns `true` when retained task state changed and `false` for an exact
    /// duplicate or a stale lifecycle payload.
    pub fn apply_replayed_task(
        &mut self,
        mut task: PlayerTask,
    ) -> Result<bool, PlayerProgressError> {
        if task.id.0 == 0 {
            return Err(PlayerProgressError::InvalidTaskId);
        }
        let following_id = task
            .id
            .0
            .checked_add(1)
            .ok_or(PlayerProgressError::IdExhausted)?;
        task.description = bounded_nonblank(&task.description, MAX_TASK_DESCRIPTION_CHARS)
            .ok_or(PlayerProgressError::BlankDescription)?;
        task.last_matching_action = task
            .last_matching_action
            .as_deref()
            .and_then(|action| bounded_nonblank(action, MAX_TASK_ACTION_CHARS));

        if let Some(index) = self
            .tasks
            .iter()
            .position(|existing| existing.id == task.id)
        {
            self.next_task_id = self.next_task_id.max(following_id);
            let existing = &mut self.tasks[index];
            if task_status_rank(task.status) <= task_status_rank(existing.status) {
                return Ok(false);
            }
            *existing = task;
            return Ok(true);
        }

        while self.tasks.len() >= MAX_PLAYER_TASKS {
            let Some(completed_index) = self
                .tasks
                .iter()
                .position(|existing| existing.status == TaskStatus::Completed)
            else {
                return Err(PlayerProgressError::ActiveTaskCapacity {
                    capacity: MAX_PLAYER_TASKS,
                });
            };
            self.tasks.remove(completed_index);
        }

        self.next_task_id = self.next_task_id.max(following_id);
        self.tasks.push(task);
        Ok(true)
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
        let action_verbs = affirmative_direct_work_verbs(&action)?;
        let candidates: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| {
                (task.status == TaskStatus::Assigned && task.location == location).then_some(index)
            })
            .collect();

        let selected =
            select_unique_task_by_overlap(&self.tasks, &candidates, &action, &action_verbs)?;

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

fn task_status_rank(status: TaskStatus) -> u8 {
    match status {
        TaskStatus::Assigned => 0,
        TaskStatus::InProgress => 1,
        TaskStatus::Completed => 2,
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
    action_verbs: &HashSet<&'static str>,
) -> Option<usize> {
    let action_tokens = meaningful_tokens(action);
    if action_tokens.is_empty() {
        return None;
    }

    let mut scored: Vec<(usize, usize)> = candidates
        .iter()
        .copied()
        .map(|index| {
            let task_verbs =
                affirmative_direct_work_verbs(&tasks[index].description).unwrap_or_default();
            let description_tokens = meaningful_tokens(&tasks[index].description);
            let compatible_verb = action_verbs.contains("work")
                || action_verbs.iter().any(|verb| task_verbs.contains(verb));
            let score = if compatible_verb {
                action_tokens.intersection(&description_tokens).count()
            } else {
                0
            };
            (index, score)
        })
        .collect();
    scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));

    let (best_index, best_score) = *scored.first()?;
    if best_score < 2 || scored.get(1).is_some_and(|(_, score)| *score == best_score) {
        return None;
    }
    Some(best_index)
}

fn task_identity_key(value: &str) -> String {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn affirmative_direct_work_verbs(value: &str) -> Option<HashSet<&'static str>> {
    let lower = value.to_lowercase().replace('\u{2019}', "'");
    let words: Vec<&str> = lower
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty()
        || words.iter().any(|word| {
            matches!(
                *word,
                "no" | "not"
                    | "never"
                    | "cannot"
                    | "dont"
                    | "cant"
                    | "wont"
                    | "avoid"
                    | "avoids"
                    | "avoided"
                    | "avoiding"
            )
        })
        || [
            "don't ",
            "do not ",
            "can't ",
            "cannot ",
            "won't ",
            "will not ",
            "instead of",
            "rather than",
            "i break the news",
            "i break the silence",
            "i break the ice",
            "i clear the air",
            "i bring the matter up",
        ]
        .iter()
        .any(|phrase| lower.contains(phrase))
        || (words.contains(&"leave") && words.contains(&"alone"))
    {
        return None;
    }

    let mut verbs = HashSet::new();
    for punctuation_clause in lower.split([',', ';']) {
        for clause in punctuation_clause
            .split(" and ")
            .flat_map(|part| part.split(" then "))
        {
            if let Some(verb) = direct_work_verb_at_clause_start(clause) {
                verbs.insert(verb);
            }
        }
    }
    (!verbs.is_empty()).then_some(verbs)
}

fn direct_work_verb_at_clause_start(clause: &str) -> Option<&'static str> {
    let words: Vec<&str> = clause
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let mut index = 0;
    if words.get(index) == Some(&"i") {
        index += 1;
    }
    if words.get(index) == Some(&"please") {
        index += 1;
    }
    let verb = words.get(index).copied()?;

    if verb == "set" {
        if words.get(index + 1) != Some(&"to") || words.get(index + 2) != Some(&"work") {
            return None;
        }
        return words
            .get(index + 3)
            .and_then(|word| canonical_work_gerund(word))
            .or(Some("work"));
    }
    if verb == "see" {
        return (words.get(index + 1) == Some(&"to")).then_some("see_to");
    }
    if verb == "take" {
        return (words.get(index + 1) == Some(&"care") && words.get(index + 2) == Some(&"of"))
            .then_some("take_care_of");
    }

    canonical_work_verb(verb)
}

fn canonical_work_verb(verb: &str) -> Option<&'static str> {
    Some(match verb {
        "mend" | "repair" => "repair",
        "break" => "break",
        "bring" => "bring",
        "carry" => "carry",
        "clean" => "clean",
        "clear" => "clear",
        "collect" => "collect",
        "cut" => "cut",
        "dig" => "dig",
        "draw" => "draw",
        "drink" => "drink",
        "drop" => "drop",
        "feed" => "feed",
        "fetch" => "fetch",
        "fill" => "fill",
        "gather" => "gather",
        "hang" => "hang",
        "harvest" => "harvest",
        "help" => "help",
        "hoe" => "hoe",
        "knead" => "knead",
        "kneel" => "kneel",
        "light" => "light",
        "lift" => "lift",
        "milk" => "milk",
        "open" => "open",
        "pick" => "pick",
        "place" => "place",
        "plant" => "plant",
        "pour" => "pour",
        "pump" => "pump",
        "put" => "put",
        "rake" => "rake",
        "scrub" => "scrub",
        "sow" => "sow",
        "stack" => "stack",
        "stoke" => "stoke",
        "sweep" => "sweep",
        "tend" => "tend",
        "tie" => "tie",
        "turn" => "turn",
        "wash" => "wash",
        "weed" => "weed",
        _ => return None,
    })
}

fn canonical_work_gerund(verb: &str) -> Option<&'static str> {
    Some(match verb {
        "mending" | "repairing" => "repair",
        "breaking" => "break",
        "bringing" => "bring",
        "carrying" => "carry",
        "cleaning" => "clean",
        "clearing" => "clear",
        "collecting" => "collect",
        "cutting" => "cut",
        "digging" => "dig",
        "drawing" => "draw",
        "feeding" => "feed",
        "fetching" => "fetch",
        "filling" => "fill",
        "gathering" => "gather",
        "harvesting" => "harvest",
        "helping" => "help",
        "hoeing" => "hoe",
        "milking" => "milk",
        "planting" => "plant",
        "raking" => "rake",
        "sowing" => "sow",
        "stacking" => "stack",
        "sweeping" => "sweep",
        "tending" => "tend",
        "turning" => "turn",
        "weeding" => "weed",
        _ => return None,
    })
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
    fn harmless_punctuation_and_whitespace_do_not_duplicate_an_active_assignment() {
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
                "  DIG   over the potato patch  ",
                NpcId(7),
                LocationId(9),
                at(11),
            )
            .unwrap();

        assert_eq!(repeated, first);
        assert_eq!(progress.len(), 1);
        assert_eq!(
            progress.task(first).unwrap().description,
            "Dig over the potato patch."
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
            progress.advance_assigned_task("I dig over the potato patch.", LocationId(9), at(11),),
            Some(id)
        );
        let task = progress.task(id).unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.started_at, Some(at(11)));
        assert_eq!(task.completed_at, None);
        assert_eq!(
            task.last_matching_action.as_deref(),
            Some("I dig over the potato patch.")
        );

        assert_eq!(
            progress.advance_assigned_task("I set to work.", LocationId(9), at(12)),
            None
        );
        assert_eq!(progress.task(id).unwrap().status, TaskStatus::InProgress);
    }

    #[test]
    fn turning_soil_action_starts_matching_live_shaped_task() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task(
                "Turn over the soil in the potato patch.",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task(
                "I turn over the soil in the potato patch with the spade.",
                LocationId(9),
                at(11),
            ),
            Some(id)
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
    fn incidental_one_token_overlap_does_not_start_a_task() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task("Mend the west wall.", NpcId(7), LocationId(9), at(10))
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task(
                "I mend the torn coat by the fire.",
                LocationId(9),
                at(11),
            ),
            None
        );
        assert_eq!(progress.task(id).unwrap().status, TaskStatus::Assigned);
    }

    #[test]
    fn negated_reported_and_alternative_actions_do_not_start_tasks() {
        for action in [
            "I do not dig over the potato patch.",
            "I don't dig over the potato patch.",
            "I dig no potato patch.",
            "I never dig over the potato patch.",
            "I remember digging over the potato patch.",
            "I dig the ditch instead of the potato patch.",
            "I dig the ditch rather than the potato patch.",
            "I leave the potato patch alone.",
        ] {
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
                progress.advance_assigned_task(action, LocationId(9), at(11)),
                None,
                "{action:?} must not imply task progression"
            );
            assert_eq!(progress.task(id).unwrap().status, TaskStatus::Assigned);
        }
    }

    #[test]
    fn speech_idioms_do_not_start_tasks_even_with_strong_token_overlap() {
        for (description, action) in [
            (
                "Break the news seal by the board.",
                "I break the news by the seal board.",
            ),
            (
                "Break the stone beside the silence marker.",
                "I break the silence beside the stone marker.",
            ),
            (
                "Break the ice block by the pond.",
                "I break the ice with a joke by the block pond.",
            ),
            (
                "Clear the air passage by Liam.",
                "I clear the air with Liam by the passage.",
            ),
            (
                "Bring the matter ledger to Liam.",
                "I bring the matter up with Liam and the ledger.",
            ),
        ] {
            let mut progress = PlayerProgress::default();
            let id = progress
                .assign_task(description, NpcId(7), LocationId(9), at(10))
                .unwrap();

            assert_eq!(
                progress.advance_assigned_task(action, LocationId(9), at(11)),
                None,
                "{action:?} is speech, not completed work"
            );
            assert_eq!(progress.task(id).unwrap().status, TaskStatus::Assigned);
        }
    }

    #[test]
    fn genuine_physical_break_action_starts_compatible_task() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task("Break the stone clods.", NpcId(7), LocationId(9), at(10))
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task("I break the stone clods.", LocationId(9), at(11),),
            Some(id)
        );
    }

    #[test]
    fn compound_take_up_action_scores_affirmative_work_clauses() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task(
                "Break the clods and plant seed in the potato patch.",
                NpcId(7),
                LocationId(9),
                at(10),
            )
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task(
                "I take up a spade, break the clods in the potato patch, and plant the seed as Siobhan instructed.",
                LocationId(9),
                at(11),
            ),
            Some(id)
        );
        let task = progress.task(id).unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.completed_at, None);
    }

    #[test]
    fn incompatible_work_verb_does_not_advance_on_shared_object_tokens() {
        let mut progress = PlayerProgress::default();
        let id = progress
            .assign_task("Mend the west wall.", NpcId(7), LocationId(9), at(10))
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task("I sweep beside the west wall.", LocationId(9), at(11),),
            None
        );
        assert_eq!(
            progress.advance_assigned_task(
                "I set to work sweeping beside the west wall.",
                LocationId(9),
                at(11),
            ),
            None
        );
        assert_eq!(progress.task(id).unwrap().status, TaskStatus::Assigned);
    }

    #[test]
    fn genuine_some_object_actions_start_compatible_tasks() {
        for (description, action) in [
            ("Carry some turf.", "I carry some turf."),
            ("Harvest some oats.", "I harvest some oats."),
            ("Weed some rows.", "I weed some rows."),
        ] {
            let mut progress = PlayerProgress::default();
            let id = progress
                .assign_task(description, NpcId(7), LocationId(9), at(10))
                .unwrap();
            assert_eq!(
                progress.advance_assigned_task(action, LocationId(9), at(11)),
                Some(id)
            );
            assert_eq!(progress.task(id).unwrap().status, TaskStatus::InProgress);
        }
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
    fn equal_nonzero_overlap_does_not_choose_between_tasks() {
        let mut progress = PlayerProgress::default();
        let east = progress
            .assign_task("Mend the east stone wall.", NpcId(7), LocationId(9), at(10))
            .unwrap();
        let west = progress
            .assign_task("Mend the west stone wall.", NpcId(8), LocationId(9), at(10))
            .unwrap();

        assert_eq!(
            progress.advance_assigned_task("I mend the stone wall.", LocationId(9), at(11),),
            None
        );
        assert_eq!(progress.task(east).unwrap().status, TaskStatus::Assigned);
        assert_eq!(progress.task(west).unwrap().status, TaskStatus::Assigned);
    }

    #[test]
    fn descriptions_actions_and_completed_history_are_bounded() {
        let mut progress = PlayerProgress::default();
        let long_description = format!("Dig {}", "é".repeat(MAX_TASK_DESCRIPTION_CHARS + 20));
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
                        &format!("Dig task plot {index}"),
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

    #[test]
    fn replayed_task_upsert_is_idempotent_monotonic_and_never_regresses() {
        let mut progress = PlayerProgress::default();
        let assigned = PlayerTask {
            id: PlayerTaskId(41),
            description: "Dig over the potato patch.".to_string(),
            assigned_by: NpcId(7),
            location: LocationId(9),
            assigned_at: at(10),
            status: TaskStatus::Assigned,
            started_at: None,
            completed_at: None,
            last_matching_action: None,
        };

        assert_eq!(progress.apply_replayed_task(assigned.clone()), Ok(true));
        assert_eq!(
            progress.apply_replayed_task(assigned.clone()),
            Ok(false),
            "replaying the same post-state payload must be idempotent"
        );
        let same_status_tamper = PlayerTask {
            description: "Replace the immutable description.".to_string(),
            assigned_by: NpcId(99),
            location: LocationId(88),
            assigned_at: at(7),
            ..assigned.clone()
        };
        assert_eq!(
            progress.apply_replayed_task(same_status_tamper),
            Ok(false),
            "equal-status replay must not rewrite immutable assignment fields"
        );
        assert_eq!(progress.task(PlayerTaskId(41)), Some(&assigned));

        let mut in_progress = assigned.clone();
        in_progress.status = TaskStatus::InProgress;
        in_progress.started_at = Some(at(11));
        in_progress.last_matching_action = Some("I dig over the potato patch.".to_string());
        assert_eq!(progress.apply_replayed_task(in_progress.clone()), Ok(true));
        assert_eq!(progress.apply_replayed_task(assigned), Ok(false));
        assert_eq!(
            progress.task(PlayerTaskId(41)),
            Some(&in_progress),
            "a stale assignment event must not regress an in-progress task"
        );

        let mut completed = in_progress.clone();
        completed.status = TaskStatus::Completed;
        completed.completed_at = Some(at(12));
        completed.last_matching_action = Some("The engine confirms the work.".to_string());
        assert_eq!(progress.apply_replayed_task(completed.clone()), Ok(true));
        assert_eq!(progress.apply_replayed_task(in_progress), Ok(false));
        assert_eq!(progress.task(PlayerTaskId(41)), Some(&completed));

        assert_eq!(
            progress
                .assign_task("Mend the west wall.", NpcId(8), LocationId(9), at(13))
                .unwrap(),
            PlayerTaskId(42),
            "journal hydration must advance the monotonic id counter"
        );
    }

    #[test]
    fn replayed_task_enforces_field_and_ledger_bounds() {
        let mut progress = PlayerProgress::default();
        let bounded = PlayerTask {
            id: PlayerTaskId(500),
            description: format!("Dig {}", "é".repeat(MAX_TASK_DESCRIPTION_CHARS + 20)),
            assigned_by: NpcId(7),
            location: LocationId(9),
            assigned_at: at(10),
            status: TaskStatus::InProgress,
            started_at: Some(at(11)),
            completed_at: None,
            last_matching_action: Some("x".repeat(MAX_TASK_ACTION_CHARS + 20)),
        };
        assert_eq!(progress.apply_replayed_task(bounded), Ok(true));
        let hydrated = progress.task(PlayerTaskId(500)).unwrap();
        assert_eq!(
            hydrated.description.chars().count(),
            MAX_TASK_DESCRIPTION_CHARS
        );
        assert_eq!(
            hydrated
                .last_matching_action
                .as_deref()
                .unwrap()
                .chars()
                .count(),
            MAX_TASK_ACTION_CHARS
        );

        let mut full_progress = PlayerProgress::default();
        for index in 0..MAX_PLAYER_TASKS {
            full_progress
                .assign_task(
                    &format!("Active task {index}"),
                    NpcId(1),
                    LocationId(index as u32 + 1),
                    at(8),
                )
                .unwrap();
        }
        let extra = PlayerTask {
            id: PlayerTaskId(500),
            description: "Replayed task".to_string(),
            assigned_by: NpcId(2),
            location: LocationId(2),
            assigned_at: at(9),
            status: TaskStatus::Assigned,
            started_at: None,
            completed_at: None,
            last_matching_action: None,
        };
        assert_eq!(
            full_progress.apply_replayed_task(extra.clone()),
            Err(PlayerProgressError::ActiveTaskCapacity {
                capacity: MAX_PLAYER_TASKS
            })
        );
        full_progress.tasks[0].status = TaskStatus::Completed;
        assert_eq!(full_progress.apply_replayed_task(extra), Ok(true));
        assert_eq!(full_progress.len(), MAX_PLAYER_TASKS);
        assert!(full_progress.task(PlayerTaskId(1)).is_none());
        assert!(full_progress.task(PlayerTaskId(500)).is_some());

        assert_eq!(
            PlayerProgress::default().apply_replayed_task(PlayerTask {
                id: PlayerTaskId(0),
                description: "Invalid".to_string(),
                assigned_by: NpcId(1),
                location: LocationId(1),
                assigned_at: at(8),
                status: TaskStatus::Assigned,
                started_at: None,
                completed_at: None,
                last_matching_action: None,
            }),
            Err(PlayerProgressError::InvalidTaskId)
        );
    }
}
