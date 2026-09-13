import Foundation

public enum SessionAction: Sendable {
    case updateDraft(String)
    case apply(SemanticEvent)
    case setViewport(TranscriptViewport)
    case followNewest
    case readHistory(anchor: TranscriptAnchor?)
    case recallCommand(LogicalRequestID)
    case loadOlderTranscript(items: [TranscriptItem], hasOlderItems: Bool)
    case restore(SessionState)
}

public enum ReductionResult: Equatable, Sendable {
    case applied
    case ignoredDuplicate
    case ignoredObsoleteEvent
    case ignoredCommittedRequest
    case ignoredWrongSession
    case ignoredUnsupportedContract
    case invalidEvent
}

/// The only mutation lane for presentation state. Adapters may deliver
/// events from any queue, but callers should serialize calls to this reducer
/// (the `PresentationSession` helper below does so on the main actor).
public struct SessionReducer: Sendable {
    public let maxProcessedEventIDs: Int

    public init(maxProcessedEventIDs: Int = 1_024) {
        self.maxProcessedEventIDs = max(1, maxProcessedEventIDs)
    }

    @discardableResult
    public mutating func reduce(_ action: SessionAction, in state: inout SessionState) -> ReductionResult {
        switch action {
        case let .updateDraft(text):
            state.updateDraft(Draft(id: state.draft.id, text: text))
            return .applied

        case let .apply(event):
            return apply(event, in: &state)

        case let .setViewport(viewport):
            state.updateViewport(viewport)
            return .applied

        case .followNewest:
            var viewport = state.viewport
            viewport.followNewest()
            state.updateViewport(viewport)
            return .applied

        case let .readHistory(anchor):
            var viewport = state.viewport
            viewport.readHistory(at: anchor)
            state.updateViewport(viewport)
            return .applied

        case let .recallCommand(id):
            guard let entry = state.commandHistory.first(where: { $0.id == id }) else {
                return .invalidEvent
            }
            state.updateDraft(Draft(text: entry.text))
            return .applied

        case let .loadOlderTranscript(items, hasOlderItems):
            // `transcript` is a bounded window. When the caller asks for an
            // older page, keep the oldest side of the merged window so the
            // page is actually visible instead of being discarded by the
            // normal newest-tail trimming policy.
            var seenIDs = Set(state.transcript.map(\.id))
            let uniqueOlder = items.filter { seenIDs.insert($0.id).inserted }
            let merged = uniqueOlder + state.transcript
            state.setTranscript(Array(merged.prefix(state.transcriptCapacity)), hasOlder: hasOlderItems)
            return .applied

        case let .restore(restoredState):
            guard restoredState.contractVersion.isSupportedByCurrentClient else {
                return .ignoredUnsupportedContract
            }
            state = restoredState
            return .applied
        }
    }

    @discardableResult
    public mutating func apply(_ event: SemanticEvent, in state: inout SessionState) -> ReductionResult {
        guard event.sessionID == state.sessionID else { return .ignoredWrongSession }
        guard event.contractVersion.isSupportedByCurrentClient else {
            return .ignoredUnsupportedContract
        }
        guard !state.processedEventIDs.contains(event.eventID) else {
            return .ignoredDuplicate
        }

        if let requestID = event.logicalRequestID, let record = state.request(for: requestID) {
            if record.hasCommittedGameplay {
                acknowledge(event, in: &state)
                return .ignoredCommittedRequest
            }

            // A terminal attempt is closed against late callbacks. The only
            // events allowed to reopen an uncommitted logical request are an
            // explicit new accepted command or a retry marker with a new
            // attempt identity. This is the local half of the Stop-versus-
            // completion race: a callback from the canceled attempt cannot
            // commit merely because it arrives later.
            let startsNewCommand = event.kind == .playerCommand
                && event.accepted
                && event.attemptID != nil
                && event.attemptID != record.currentAttemptID
            let startsRetry = event.kind == .progress
                && event.metadata["retry"] == "true"
                && record.phase.canRetry
                && event.attemptID != nil
                && event.attemptID != record.currentAttemptID

            if record.phase.isTerminal {
                guard startsNewCommand || startsRetry else {
                    acknowledge(event, in: &state)
                    return .ignoredObsoleteEvent
                }
            }

            if let currentAttemptID = record.currentAttemptID,
               event.attemptID != currentAttemptID,
               !startsNewCommand,
               !startsRetry {
                acknowledge(event, in: &state)
                return .ignoredObsoleteEvent
            }
        }

        // The adapter's cursor is the latest consumed sequence, so a unique
        // older callback is still acknowledged but cannot be appended after
        // newer transcript content.
        guard event.sequence.rawValue > state.eventCursor.rawValue else {
            acknowledge(event, in: &state)
            return .ignoredObsoleteEvent
        }

        if event.provisional, let itemID = event.transcriptItemID {
            let incomingSequence = event.streamSequence ?? event.sequence.rawValue
            if let previous = state.latestStreamProgress(for: itemID), incomingSequence <= previous.sequence {
                acknowledge(event, in: &state)
                return .ignoredObsoleteEvent
            }
        }

        acknowledge(event, in: &state)

        switch event.kind {
        case .playerCommand:
            applyPlayerCommand(event, in: &state)
        case .commandInterpreted:
            updateRequest(event, phase: .interpreting, in: &state)
            upsertTranscript(for: event, state: &state, stateOverride: event.provisional ? .provisional : .committed)
        case .clarificationRequired:
            updateRequest(event, phase: .awaitingClarification, in: &state)
            if let requestID = event.logicalRequestID, let attemptID = event.attemptID, let clarification = event.clarification {
                state.updatePendingClarification(PendingClarification(requestID: requestID, attemptID: attemptID, prompt: clarification))
            }
            upsertTranscript(for: event, state: &state, stateOverride: .committed)
        case .clarificationSelected:
            updateRequest(event, phase: .executing, in: &state)
            state.updatePendingClarification(nil)
            state.removeTranscriptItems { item in
                item.kind == .clarificationRequired
                    && item.logicalRequestID == event.logicalRequestID
                    && item.attemptID == event.attemptID
                    && item.id != event.transcriptItemID
            }
            upsertTranscript(for: event, state: &state, stateOverride: .committed)
        case .progress:
            updateRequest(event, phase: .executing, in: &state)
            upsertTranscript(for: event, state: &state, stateOverride: event.provisional ? .provisional : .committed)
        case .sceneChanged:
            if let sceneID = event.metadata["sceneID"] {
                state.updateScene(SceneSummary(id: sceneID, name: event.metadata["sceneName"] ?? event.content ?? sceneID, detail: event.metadata["sceneDetail"]))
            } else if let content = event.content {
                state.updateScene(SceneSummary(id: event.metadata["scene"] ?? content, name: content))
            }
            upsertTranscript(for: event, state: &state, stateOverride: .committed)
        case .narration, .npcDialogue, .actionResult:
            updateRequest(event, phase: event.provisional ? .executing : nil, in: &state)
            upsertTranscript(for: event, state: &state, stateOverride: event.provisional ? .provisional : .committed)
        case .error:
            state.updateError(event.content)
            upsertTranscript(for: event, state: &state, stateOverride: .failed)
        case .responseCompleted:
            if let outcome = applyCompletion(event, in: &state),
               event.content != nil,
               event.transcriptItemID != nil {
                upsertTranscript(
                    for: event,
                    state: &state,
                    stateOverride: transcriptState(for: outcome)
                )
            }
        }

        if event.provisional, let itemID = event.transcriptItemID {
            state.updateStreamProgress(StreamProgress(
                itemID: itemID,
                attemptID: event.attemptID,
                sequence: event.streamSequence ?? event.sequence.rawValue
            ))
        }

        let canAdvanceRevision = event.kind != .responseCompleted
            || event.terminalOutcome == nil
            || event.terminalOutcome == .succeeded
        if canAdvanceRevision, let revision = event.stateRevision {
            state.updateStateRevision(revision)
        }

        if event.content != nil {
            var viewport = state.viewport
            viewport.receivedNewText()
            state.updateViewport(viewport)
        }

        return .applied
    }

    private func acknowledge(_ event: SemanticEvent, in state: inout SessionState) {
        state.recordProcessedEvent(event.eventID)
        state.trimProcessedEvents(to: maxProcessedEventIDs)
        state.updateCursor(EventCursor(event.sequence.rawValue))
    }

    private func applyPlayerCommand(_ event: SemanticEvent, in state: inout SessionState) {
        let text = event.content ?? ""
        let requestID = event.logicalRequestID
        let attemptID = event.attemptID
        var record: RequestRecord?

        if let requestID {
            if var existing = state.request(for: requestID) {
                if let attemptID, existing.attempts.first(where: { $0.id == attemptID }) == nil {
                    existing.attempts.append(RequestAttempt(id: attemptID, originalText: text, startedAt: event.sequence))
                    existing.currentAttemptID = attemptID
                    existing.phase = .accepted
                    existing.terminalOutcome = nil
                    existing.committedStateRevision = nil
                }
                record = existing
            } else {
                let attempt = attemptID.map { RequestAttempt(id: $0, originalText: text, startedAt: event.sequence) }
                record = RequestRecord(
                    id: requestID,
                    originalText: text,
                    acceptedCommandItemID: event.transcriptItemID,
                    attempts: attempt.map { [$0] } ?? [],
                    currentAttemptID: attemptID,
                    phase: .accepted
                )
            }
        }

        if let record {
            state.replaceRequest(record)
            state.addHistory(CommandHistoryEntry(id: record.id, text: record.originalText, commandItemID: event.transcriptItemID))
        }

        upsertTranscript(for: event, state: &state, stateOverride: .accepted)

        // The acceptance event owns composer clearing. Matching both the
        // draft identity and text protects edits made while a request is
        // waiting for its acceptance receipt or streaming its response.
        if event.accepted,
           let sourceDraftID = event.sourceDraftID,
           state.draft.id == sourceDraftID,
           (state.draft.text == text || state.draft.text.trimmingCharacters(in: .whitespacesAndNewlines) == text.trimmingCharacters(in: .whitespacesAndNewlines)) {
            state.updateDraft(.empty)
        }
    }

    private func updateRequest(
        _ event: SemanticEvent,
        phase: RequestPhase?,
        in state: inout SessionState
    ) {
        guard let requestID = event.logicalRequestID,
              var record = state.request(for: requestID) else { return }

        if event.kind == .progress, event.metadata["retry"] == "true" {
            // A retry is a new execution attempt for the same logical
            // request. Its in-flight record must not continue to advertise
            // the previous attempt's terminal outcome or revision.
            record.terminalOutcome = nil
            record.committedStateRevision = nil
        }

        if let phase {
            record.phase = phase
            if let attemptID = event.attemptID {
                if let index = record.attempts.firstIndex(where: { $0.id == attemptID }) {
                    record.attempts[index].phase = phase
                    if event.provisional, let itemID = event.transcriptItemID,
                       !record.attempts[index].provisionalItemIDs.contains(itemID) {
                        record.attempts[index].provisionalItemIDs.append(itemID)
                    }
                } else {
                    // Retry and clarification continuation events introduce a
                    // new attempt without repeating the player's command.
                    // Record it before accepting any stream callback so a
                    // late event from the old attempt is obsolete.
                    let provisionalItems = event.provisional
                        ? event.transcriptItemID.map { [$0] } ?? []
                        : []
                    record.attempts.append(RequestAttempt(
                        id: attemptID,
                        originalText: record.originalText,
                        phase: phase,
                        provisionalItemIDs: provisionalItems,
                        startedAt: event.sequence
                    ))
                    record.currentAttemptID = attemptID
                }
            }
        } else if event.provisional,
                  let attemptID = event.attemptID,
                  let itemID = event.transcriptItemID {
            if let index = record.attempts.firstIndex(where: { $0.id == attemptID }) {
                if !record.attempts[index].provisionalItemIDs.contains(itemID) {
                    record.attempts[index].provisionalItemIDs.append(itemID)
                }
            } else {
                record.attempts.append(RequestAttempt(
                    id: attemptID,
                    originalText: record.originalText,
                    phase: .executing,
                    provisionalItemIDs: [itemID],
                    startedAt: event.sequence
                ))
                record.currentAttemptID = attemptID
                record.phase = .executing
            }
        }

        state.replaceRequest(record)
    }

    private func applyCompletion(_ event: SemanticEvent,
                                 in state: inout SessionState) -> ResponseTerminalOutcome? {
        guard let requestID = event.logicalRequestID,
              let attemptID = event.attemptID else { return nil }

        let outcome = event.terminalOutcome ?? .succeeded

        // A bounded Rust snapshot may begin with the response for a request
        // whose accepted PlayerCommand is in the durable history just before
        // the retained tail.  Replay must still settle an orphaned response
        // row when its terminal event is present.  Do not synthesize a
        // terminal RequestRecord here: the authoritative request projection
        // is overlaid by the engine controller, and a synthetic completed
        // record would suppress the response event itself during replay.
        guard var record = state.request(for: requestID) else {
            let transcriptState = transcriptState(for: outcome)
            let matchingItemIDs = state.transcript
                .filter { $0.logicalRequestID == requestID && $0.attemptID == attemptID }
                .map(\.id)
            for itemID in matchingItemIDs {
                state.updateTranscriptItem(id: itemID) { item in
                    item.state = transcriptState
                }
            }
            return outcome
        }

        guard let attemptIndex = record.attempts.firstIndex(where: { $0.id == attemptID }) else { return nil }

        // A terminal logical request cannot be reopened by a late callback.
        if record.hasCommittedGameplay { return nil }

        record.terminalOutcome = outcome
        record.phase = phase(for: outcome)
        record.currentAttemptID = attemptID
        record.attempts[attemptIndex].terminalOutcome = outcome
        record.attempts[attemptIndex].phase = phase(for: outcome)
        record.attempts[attemptIndex].terminalEventID = event.eventID
        record.attempts[attemptIndex].committedStateRevision = outcome == .succeeded
            ? (event.stateRevision ?? state.stateRevision)
            : nil
        if let revision = record.attempts[attemptIndex].committedStateRevision {
            record.committedStateRevision = revision
            state.updateStateRevision(revision)
        }

        let provisionalIDs = record.attempts[attemptIndex].provisionalItemIDs
        for itemID in provisionalIDs {
            state.updateTranscriptItem(id: itemID) { item in
                item.state = transcriptState(for: outcome)
            }
        }

        state.updatePendingClarification(nil)
        state.replaceRequest(record)
        return outcome
    }

    private func upsertTranscript(
        for event: SemanticEvent,
        state: inout SessionState,
        stateOverride: TranscriptItemState
    ) {
        guard let content = event.content, !content.isEmpty || event.transcriptItemID != nil else { return }
        let itemID = event.transcriptItemID ?? TranscriptItemID("event:\(event.eventID.rawValue)")
        let current = state.transcript.first(where: { $0.id == itemID })
        let nextContent: String
        if let current, event.provisional, event.streamUpdate == .append {
            nextContent = current.content + content
        } else {
            nextContent = content
        }

        let item = TranscriptItem(
            id: itemID,
            kind: event.kind,
            content: nextContent,
            speaker: event.speaker ?? current?.speaker,
            logicalRequestID: event.logicalRequestID,
            attemptID: event.attemptID,
            state: stateOverride,
            gameTime: event.gameTime,
            lastEventSequence: event.sequence,
            metadata: event.metadata
        )
        state.upsertTranscriptItem(item)

        if event.provisional,
           let requestID = event.logicalRequestID,
           var record = state.request(for: requestID),
           let attemptID = event.attemptID,
           let attemptIndex = record.attempts.firstIndex(where: { $0.id == attemptID }) {
            if !record.attempts[attemptIndex].provisionalItemIDs.contains(itemID) {
                record.attempts[attemptIndex].provisionalItemIDs.append(itemID)
                state.replaceRequest(record)
            }
        }
    }

    private func phase(for outcome: ResponseTerminalOutcome) -> RequestPhase {
        switch outcome {
        case .succeeded: return .completed
        case .cancelled: return .cancelled
        case .interrupted: return .interrupted
        case .failed: return .failed
        }
    }

    private func transcriptState(for outcome: ResponseTerminalOutcome) -> TranscriptItemState {
        switch outcome {
        case .succeeded: return .committed
        case .cancelled: return .cancelled
        case .interrupted: return .interrupted
        case .failed: return .failed
        }
    }
}

/// A small main-actor façade suitable for a SwiftUI view model. It contains
/// no UI framework dependency, so the same reducer can be exercised in a
/// Foundation-only XCTest target.
@MainActor
public final class PresentationSession {
    public private(set) var state: SessionState
    private var reducer: SessionReducer

    public init(state: SessionState = SessionState(), reducer: SessionReducer = SessionReducer()) {
        self.state = state
        self.reducer = reducer
    }

    @discardableResult
    public func apply(_ event: SemanticEvent) -> ReductionResult {
        reducer.reduce(.apply(event), in: &state)
    }

    public func updateDraft(_ text: String) {
        reducer.reduce(.updateDraft(text), in: &state)
    }

    public func followNewest() {
        reducer.reduce(.followNewest, in: &state)
    }

    public func readHistory(anchor: TranscriptAnchor? = nil) {
        reducer.reduce(.readHistory(anchor: anchor), in: &state)
    }

    public func recallCommand(_ id: LogicalRequestID) {
        reducer.reduce(.recallCommand(id), in: &state)
    }
}
