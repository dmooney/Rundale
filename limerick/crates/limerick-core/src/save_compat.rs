//! Opening saves across builds (ADR-025 §4).
//!
//! Every runtime (server, Tauri, headless REPL) opens a save through
//! [`check_save`] before anything writes to it:
//!
//! - A save is refused only when its authoritative state cannot be read: the
//!   world snapshot and the world events replayed over it, and the turn
//!   requests (checked here as [`RequestRecord`]s), or when it was played
//!   with other content. The refusal is [`LimerickError::SaveIncompatible`];
//!   the runtime shows [`INCOMPATIBLE_SAVE_MESSAGE`], keeps its current game,
//!   and offers a new one. Nothing has written to the file, so it stays
//!   byte-identical.
//! - Content compatibility uses the content's stable identity (mod id and
//!   version, recorded in every snapshot since save format 3), not a hash or
//!   a file path. A save opens against any version of the same content; it
//!   is refused against different content, whose places and people reuse
//!   the same numeric ids. A save written before format 3 records no
//!   identity and opens as before.
//! - Transcript events are presentation. One of a kind this build does not
//!   know (or cannot read) never blocks opening: it stays in the save
//!   verbatim and is shown as [`FALLBACK_LINE`] ([`transcript_fallback_lines`]).
//!
//! The save format version and the read-only inspection live in
//! `limerick-persistence` (`database/format.rs`).

use std::path::Path;

use crate::error::LimerickError;
use crate::persistence::{Database, SaveInspection, TranscriptEventRow, inspect_save_with};
use crate::turn::{FALLBACK_LINE, PendingEvent, RequestRecord, TranscriptEventKind};
use limerick_types::ContentIdentity;

/// What a runtime tells the player when it refuses a save.
pub const INCOMPATIBLE_SAVE_MESSAGE: &str = "This save can't be opened by this version of the \
     game. It has been left exactly as it was. Type /new to start a new game.";

/// The most fallback lines shown when a save opens; older ones stay in the
/// save but are not listed.
pub const MAX_FALLBACK_LINES: usize = 10;

/// Reads the save at `path` without writing to it and refuses it when its
/// authoritative state cannot be read or it was played with content other
/// than `content` (the runtime's loaded content; `None` skips that check).
///
/// Pass the returned inspection to [`Database::open_inspected`] (or use
/// [`open_checked`]) to open the save without inspecting it twice.
pub fn check_save(
    path: &Path,
    content: Option<&ContentIdentity>,
) -> Result<SaveInspection, LimerickError> {
    let inspection = inspect_save_with(path, &|record| {
        serde_json::from_str::<RequestRecord>(record)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })?;
    if let Some(current) = content {
        for branch in &inspection.branches {
            if let Some(saved) = &branch.content
                && !saved.is_compatible_with(current)
            {
                return Err(LimerickError::SaveIncompatible(format!(
                    "branch '{}' was played with content '{}' (version {}), not '{}'",
                    branch.name, saved.id, saved.version, current.id
                )));
            }
        }
    }
    Ok(inspection)
}

/// [`check_save`], then opens the save for play.
pub fn open_checked(
    path: &Path,
    content: Option<&ContentIdentity>,
) -> Result<Database, LimerickError> {
    let inspection = check_save(path, content)?;
    Database::open_inspected(path, &inspection)
}

/// Whether `error` is a refusal to open a save (as opposed to an I/O or
/// locking failure worth retrying).
pub fn is_incompatible(error: &LimerickError) -> bool {
    matches!(error, LimerickError::SaveIncompatible(_))
}

/// Logs why a save was refused and returns the player-facing message.
pub fn refusal_message(path: &Path, error: &LimerickError) -> &'static str {
    tracing::warn!(
        save = %path.display(),
        %error,
        "refused to open a save; the file was left unchanged"
    );
    INCOMPATIBLE_SAVE_MESSAGE
}

/// The lines to show for branch `branch_id`'s transcript events this build
/// cannot present: one [`FALLBACK_LINE`] per such event, in sequence order,
/// at most the [`MAX_FALLBACK_LINES`] most recent. Blocking.
pub fn transcript_fallback_lines(
    db: &Database,
    branch_id: i64,
) -> Result<Vec<String>, LimerickError> {
    let unpresentable: Vec<TranscriptEventRow> = db
        .transcript_events(branch_id, 0)?
        .into_iter()
        .filter(|row| !is_presentable(row))
        .collect();
    for row in &unpresentable {
        tracing::info!(
            sequence = row.sequence,
            event_id = %row.event_id,
            kind = %row.kind,
            "transcript event this build cannot present; showing a fallback line"
        );
    }
    let shown = unpresentable.len().min(MAX_FALLBACK_LINES);
    Ok(vec![FALLBACK_LINE.to_string(); shown])
}

/// [`transcript_fallback_lines`] for the save at `path`. Blocking.
pub fn transcript_fallback_lines_at(
    path: &Path,
    branch_id: i64,
) -> Result<Vec<String>, LimerickError> {
    transcript_fallback_lines(&Database::open(path)?, branch_id)
}

/// Whether this build can present a stored transcript event: its kind is
/// known and its payload reads.
fn is_presentable(row: &TranscriptEventRow) -> bool {
    TranscriptEventKind::parse(&row.kind).is_known()
        && serde_json::from_str::<PendingEvent>(&row.event)
            .is_ok_and(|event| event.kind.is_known())
}

#[cfg(test)]
mod tests;
