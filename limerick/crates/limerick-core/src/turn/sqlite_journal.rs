//! The turn journal in the save database.
//!
//! [`SqliteTurnJournal`] keeps one branch's requests and transcript events
//! in the `requests` and `transcript_events` tables of the save's SQLite
//! database (`limerick-persistence`). Every call is one SQLite transaction.
//! A commit writes the request terminal, the transcript events, the
//! authoritative state (a snapshot of the world and NPCs the turn installs),
//! and the task batch (journaled against that snapshot) together, so the
//! save's latest snapshot always includes every committed turn and autosave
//! can never disagree with the turn journal.
//!
//! Requests and events belong to the branch they were played on. A fork
//! starts an empty turn history at revision 0; see
//! `docs/design/portable-turn-api.md` §6.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use limerick_types::PlayerTask;

use super::BoxFuture;
use super::ids::{EventSequence, LogicalRequestId};
use super::journal::{JournalError, TurnCommit, TurnJournal};
use super::lifecycle::RequestRecord;
use super::transcript::{PendingEvent, TranscriptEvent};
use crate::error::LimerickError;
use crate::persistence::{
    Database, GameSnapshot, NewTranscriptEvent, TranscriptEventRow, TurnJournalWriter,
    TurnRequestRow,
};
use crate::session_store::task_world_events;

impl From<LimerickError> for JournalError {
    fn from(error: LimerickError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<serde_json::Error> for JournalError {
    fn from(error: serde_json::Error) -> Self {
        Self::Storage(format!("journal row is not valid JSON: {error}"))
    }
}

/// One branch's turn journal in a save database.
pub struct SqliteTurnJournal {
    db: Arc<Mutex<Database>>,
    branch_id: i64,
    fail_next_writes: Arc<AtomicUsize>,
}

/// What a write carries besides the record and events.
enum Write {
    Accept,
    Update,
    Commit {
        tasks: Vec<PlayerTask>,
        state: Option<Box<GameSnapshot>>,
    },
}

impl SqliteTurnJournal {
    /// Opens the journal of `branch_id` in the save at `save_path`,
    /// migrating a save written before the turn journal existed.
    pub fn open(save_path: &Path, branch_id: i64) -> Result<Self, JournalError> {
        let db = Database::open(save_path)?;
        if !db
            .list_branches()?
            .iter()
            .any(|branch| branch.id == branch_id)
        {
            return Err(JournalError::Storage(format!(
                "branch {branch_id} does not exist in {}",
                save_path.display()
            )));
        }
        Ok(Self::from_database(db, branch_id))
    }

    /// The journal of `branch_id` in an already open database.
    pub fn from_database(db: Database, branch_id: i64) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            branch_id,
            fail_next_writes: Arc::default(),
        }
    }

    /// The branch this journal belongs to.
    pub fn branch_id(&self) -> i64 {
        self.branch_id
    }

    /// Makes the next `count` writes fail with [`JournalError::Storage`]
    /// after every statement of their transaction has run, so the whole
    /// transaction must roll back (fault injection for tests and proofs).
    pub fn fail_next_writes(&self, count: usize) {
        self.fail_next_writes.store(count, Ordering::SeqCst);
    }

    /// Every event of the branch, in sequence order. Blocking.
    pub fn events(&self) -> Result<Vec<TranscriptEvent>, JournalError> {
        let db = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        db.transcript_events(self.branch_id, 0)?
            .into_iter()
            .map(transcript_event)
            .collect()
    }

    /// The journaled record of `id` on this branch. Blocking.
    pub fn request(&self, id: &LogicalRequestId) -> Result<Option<RequestRecord>, JournalError> {
        let db = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        db.turn_request(self.branch_id, id.as_str())?
            .map(|row| Ok(serde_json::from_str(&row.record)?))
            .transpose()
    }

    /// Runs `work` against the database when the returned future is first
    /// polled. The work is synchronous inside that poll, so dropping the
    /// future can never leave a write half-observed by the engine; on a
    /// multi-threaded runtime it runs in `block_in_place` so other tasks
    /// move off the worker meanwhile.
    fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Database, i64) -> Result<T, JournalError> + Send + 'static,
    ) -> BoxFuture<'_, Result<T, JournalError>> {
        let db = Arc::clone(&self.db);
        let branch_id = self.branch_id;
        Box::pin(async move {
            let job = move || {
                let db = db.lock().unwrap_or_else(PoisonError::into_inner);
                work(&db, branch_id)
            };
            match tokio::runtime::Handle::try_current() {
                Ok(handle)
                    if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread =>
                {
                    tokio::task::block_in_place(job)
                }
                _ => job(),
            }
        })
    }

    fn write(
        &self,
        kind: Write,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        let fail = Arc::clone(&self.fail_next_writes);
        self.run(move |db, branch_id| {
            db.turn_journal_transaction(|writer| {
                let stored = write_in(writer, branch_id, kind, &record, events)?;
                let injected = fail
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                        left.checked_sub(1)
                    })
                    .is_ok();
                if injected {
                    return Err(JournalError::Storage("injected write failure".to_string()));
                }
                Ok(stored)
            })
        })
    }
}

/// The body of one journal transaction: validates against the stored rows
/// and writes the call's rows. Any error rolls back the whole transaction.
fn write_in(
    writer: &TurnJournalWriter<'_>,
    branch_id: i64,
    kind: Write,
    record: &RequestRecord,
    events: Vec<PendingEvent>,
) -> Result<Vec<TranscriptEvent>, JournalError> {
    let known = writer
        .request(record.id.as_str())?
        .filter(|row| matches!(kind, Write::Accept) || row.branch_id == branch_id);
    match (&kind, known) {
        (Write::Accept, Some(_)) => return Err(JournalError::DuplicateRequest(record.id.clone())),
        (Write::Update | Write::Commit { .. }, None) => {
            return Err(JournalError::UnknownRequest(record.id.clone()));
        }
        _ => {}
    }

    let mut stored = Vec::with_capacity(events.len());
    for event in events {
        if let Some(row) = writer.event(event.id.as_str())? {
            let existing = transcript_event(row.clone())?;
            if existing.event != event || row.branch_id != branch_id {
                return Err(JournalError::ConflictingEvent(event.id));
            }
            stored.push(existing);
            continue;
        }
        let json = serde_json::to_string(&event)?;
        let sequence = writer.append_event(
            branch_id,
            NewTranscriptEvent {
                event_id: event.id.as_str(),
                request_id: event.request_id.as_ref().map(LogicalRequestId::as_str),
                kind: event.kind.as_str(),
                event: &json,
            },
        )?;
        stored.push(TranscriptEvent {
            sequence: EventSequence(sequence),
            event,
        });
    }

    if let Write::Commit { tasks, state } = kind {
        let snapshot_id = match state {
            Some(state) => Some(writer.save_snapshot(branch_id, &state)?),
            None if tasks.is_empty() => None,
            None => Some(writer.latest_snapshot_id(branch_id)?.ok_or_else(|| {
                JournalError::Storage(format!(
                    "cannot journal player task: branch {branch_id} has no snapshot"
                ))
            })?),
        };
        if let Some(snapshot_id) = snapshot_id
            && !tasks.is_empty()
        {
            writer.append_world_events(branch_id, snapshot_id, &task_world_events(&tasks))?;
        }
    }

    writer.put_request(&TurnRequestRow {
        request_id: record.id.as_str().to_string(),
        branch_id,
        phase: serde_json::to_value(record.phase)?
            .as_str()
            .unwrap_or_default()
            .to_string(),
        is_open: record.phase.is_open(),
        committed_revision: record.committed_revision.map(|revision| revision.0),
        record: serde_json::to_string(record)?,
    })?;
    Ok(stored)
}

fn transcript_event(row: TranscriptEventRow) -> Result<TranscriptEvent, JournalError> {
    Ok(TranscriptEvent {
        sequence: EventSequence(row.sequence),
        event: serde_json::from_str(&row.event)?,
    })
}

fn records(rows: Vec<TurnRequestRow>) -> Result<Vec<RequestRecord>, JournalError> {
    rows.into_iter()
        .map(|row| Ok(serde_json::from_str(&row.record)?))
        .collect()
}

impl TurnJournal for SqliteTurnJournal {
    fn accept(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        self.write(Write::Accept, record, events)
    }

    fn update(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        self.write(Write::Update, record, events)
    }

    fn commit(
        &self,
        commit: TurnCommit,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        self.write(
            Write::Commit {
                tasks: commit.task_mutations,
                state: commit.state.map(Box::new),
            },
            commit.record,
            commit.events,
        )
    }

    fn open_requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>> {
        self.run(|db, branch_id| records(db.open_turn_requests(branch_id)?))
    }

    fn requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>> {
        self.run(|db, branch_id| records(db.turn_requests(branch_id)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::WorldEvent;
    use crate::turn::ids::{ExecutionAttemptId, StateRevision};
    use crate::turn::journal_contract::{self, ContractJournal, accepted, committed, one_task};
    use crate::turn::lifecycle::{RequestPhase, TerminalOutcome};

    impl ContractJournal for SqliteTurnJournal {
        fn fail_next_writes(&self, count: usize) {
            SqliteTurnJournal::fail_next_writes(self, count);
        }

        fn stored_events(&self) -> Vec<TranscriptEvent> {
            self.events().unwrap()
        }

        fn stored_request(&self, id: &LogicalRequestId) -> Option<RequestRecord> {
            self.request(id).unwrap()
        }

        fn durable_task_count(&self) -> usize {
            let db = self.db.lock().unwrap();
            db.branch_log(self.branch_id)
                .unwrap()
                .iter()
                .flat_map(|snapshot| {
                    db.events_since_snapshot(self.branch_id, snapshot.id)
                        .unwrap()
                })
                .filter(|event| matches!(event, WorldEvent::PlayerTaskStateChanged { .. }))
                .count()
        }

        fn durable_state_count(&self) -> usize {
            let db = self.db.lock().unwrap();
            db.branch_log(self.branch_id).unwrap().len()
        }
    }

    fn in_memory() -> SqliteTurnJournal {
        let db = Database::open_memory().unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        SqliteTurnJournal::from_database(db, main)
    }

    journal_contract::journal_contract_tests!(super::in_memory());

    /// A save on disk with the main branch and its first snapshot.
    fn save_on_disk() -> (tempfile::TempDir, std::path::PathBuf, i64) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("limerick_001.db");
        let db = Database::open(&path).unwrap();
        let main = db.find_branch("main").unwrap().unwrap().id;
        db.save_snapshot(main, &journal_contract::state()).unwrap();
        (dir, path, main)
    }

    /// Row counts of every table a commit writes.
    fn rows(path: &Path) -> [i64; 4] {
        let conn = rusqlite::Connection::open(path).unwrap();
        [
            "requests",
            "transcript_events",
            "snapshots",
            "journal_events",
        ]
        .map(|table| {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
        })
    }

    #[tokio::test]
    async fn a_committed_turn_survives_reopening_the_save() {
        let (_dir, path, main) = save_on_disk();
        let journal = SqliteTurnJournal::open(&path, main).unwrap();
        let (mut record, command) = accepted("r1");
        journal.accept(record.clone(), vec![command]).await.unwrap();
        let commit = committed(&mut record, one_task());
        journal.update(record.clone(), Vec::new()).await.unwrap();
        let stored = journal.commit(commit.clone()).await.unwrap();
        drop(journal);

        let reopened = SqliteTurnJournal::open(&path, main).unwrap();
        assert_eq!(reopened.requests().await.unwrap(), vec![commit.record]);
        let events = reopened.events().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(&events[1..], &stored[..]);
        assert!(reopened.open_requests().await.unwrap().is_empty());

        // The committed state is the save's latest snapshot, and the task
        // batch replays from it.
        let db = Database::open(&path).unwrap();
        let recovery = db.load_recovery_data(main).unwrap().unwrap();
        assert_eq!(recovery.snapshot, journal_contract::state());
        assert_eq!(recovery.journal.len(), 1);
        assert!(matches!(
            recovery.journal[0],
            WorldEvent::PlayerTaskStateChanged { .. }
        ));
    }

    // Oracle: ios-port persistence `sqlite_failure_rolls_back_generation_state_request_and_event_together`.
    #[tokio::test]
    async fn a_failing_statement_rolls_back_state_tasks_request_and_events_together() {
        for table in [
            "transcript_events",
            "snapshots",
            "journal_events",
            "requests",
        ] {
            let (_dir, path, main) = save_on_disk();
            let journal = SqliteTurnJournal::open(&path, main).unwrap();
            let (mut record, command) = accepted("r1");
            journal.accept(record.clone(), vec![command]).await.unwrap();
            let commit = committed(&mut record, one_task());
            journal.update(record.clone(), Vec::new()).await.unwrap();
            let before = rows(&path);

            rusqlite::Connection::open(&path)
                .unwrap()
                .execute_batch(&format!(
                    "CREATE TRIGGER fail_commit BEFORE INSERT ON {table}
                     BEGIN SELECT RAISE(ABORT, 'injected {table} failure'); END;
                     CREATE TRIGGER fail_commit_update BEFORE UPDATE ON {table}
                     BEGIN SELECT RAISE(ABORT, 'injected {table} failure'); END;"
                ))
                .unwrap();
            let error = journal.commit(commit.clone()).await.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&format!("injected {table} failure")),
                "{table}: {error}"
            );
            assert_eq!(rows(&path), before, "{table}: no partial write");
            assert_eq!(
                journal.request(&record.id).unwrap().unwrap().phase,
                RequestPhase::Executing,
                "{table}: the request is not terminal"
            );

            rusqlite::Connection::open(&path)
                .unwrap()
                .execute_batch("DROP TRIGGER fail_commit; DROP TRIGGER fail_commit_update;")
                .unwrap();
            journal.commit(commit).await.unwrap();
            let after = rows(&path);
            assert_eq!(after[1], before[1] + 2, "{table}: events on retry");
            assert_eq!(after[2], before[2] + 1, "{table}: snapshot on retry");
            assert_eq!(after[3], before[3] + 1, "{table}: task on retry");
        }
    }

    #[tokio::test]
    async fn requests_and_events_belong_to_their_branch() {
        let (_dir, path, main) = save_on_disk();
        let fork = {
            let db = Database::open(&path).unwrap();
            db.create_branch_with_snapshot("fork", Some(main), &journal_contract::state())
                .unwrap()
                .0
        };
        let on_main = SqliteTurnJournal::open(&path, main).unwrap();
        let on_fork = SqliteTurnJournal::open(&path, fork).unwrap();
        let (record, command) = accepted("r1");
        on_main.accept(record.clone(), vec![command]).await.unwrap();

        assert!(on_fork.requests().await.unwrap().is_empty());
        assert!(on_fork.events().unwrap().is_empty());
        assert_eq!(
            on_fork
                .update(record.clone(), Vec::new())
                .await
                .unwrap_err(),
            JournalError::UnknownRequest(record.id.clone()),
            "a request cannot move to another branch"
        );
        let (second, second_command) = accepted("r2");
        let on_fork_events = on_fork.accept(second, vec![second_command]).await.unwrap();
        assert!(
            on_fork_events[0].sequence > on_main.events().unwrap()[0].sequence,
            "the sequence counter is save-wide"
        );
        assert!(SqliteTurnJournal::open(&path, 9999).is_err());
    }

    #[tokio::test]
    async fn a_commit_without_state_journals_its_tasks_on_the_latest_snapshot() {
        let (_dir, path, main) = save_on_disk();
        let journal = SqliteTurnJournal::open(&path, main).unwrap();
        let (mut record, command) = accepted("r1");
        journal.accept(record.clone(), vec![command]).await.unwrap();
        let mut commit = committed(&mut record, one_task());
        commit.state = None;
        journal.update(record.clone(), Vec::new()).await.unwrap();
        journal.commit(commit).await.unwrap();
        assert_eq!(rows(&path)[2], 1, "no snapshot written");
        assert_eq!(rows(&path)[3], 1, "the task is journaled");
    }

    #[tokio::test]
    async fn a_request_interrupted_mid_attempt_stays_open_until_recovered() {
        let (_dir, path, main) = save_on_disk();
        let journal = SqliteTurnJournal::open(&path, main).unwrap();
        let (mut record, command) = accepted("r1");
        journal.accept(record.clone(), vec![command]).await.unwrap();
        record
            .begin_attempt(ExecutionAttemptId::new("a1"), StateRevision(0))
            .unwrap();
        journal.update(record.clone(), Vec::new()).await.unwrap();
        drop(journal);

        let reopened = SqliteTurnJournal::open(&path, main).unwrap();
        let open = reopened.open_requests().await.unwrap();
        assert_eq!(open, vec![record.clone()]);
        let mut recovered = open[0].clone();
        assert!(recovered.recover());
        reopened.update(recovered, Vec::new()).await.unwrap();
        assert!(reopened.open_requests().await.unwrap().is_empty());
        assert_eq!(
            reopened
                .request(&record.id)
                .unwrap()
                .unwrap()
                .terminal_outcome,
            Some(TerminalOutcome::Interrupted)
        );
    }

    #[test]
    fn journal_calls_work_outside_a_runtime() {
        let journal = in_memory();
        let (record, command) = accepted("r1");
        let stored = poll_once(journal.accept(record, vec![command])).unwrap();
        assert_eq!(stored.len(), 1);
    }

    /// Polls a future that completes on its first poll (the journal's work
    /// runs synchronously inside it).
    fn poll_once<T>(future: BoxFuture<'_, T>) -> T {
        let mut future = future;
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("journal work must complete on its first poll"),
        }
    }
}
