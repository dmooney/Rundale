import XCTest
@testable import RundaleKit

final class RundaleKitTests: XCTestCase {
    func testSemanticEventRoundTripAndVersionValidation() throws {
        let event = SemanticEvent(
            eventID: SemanticEventID("event-1"),
            sessionID: SessionID("session-1"),
            sequence: EventSequence(7),
            kind: .npcDialogue,
            content: "á—你好",
            speaker: "Peig",
            logicalRequestID: LogicalRequestID("request-1"),
            attemptID: ExecutionAttemptID("attempt-1"),
            transcriptItemID: TranscriptItemID("item-1"),
            provisional: true,
            streamSequence: 2,
            stateRevision: StateRevision(3),
            metadata: ["entityID": "npc-peig"]
        )
        let encoded = try FixtureJSON.encode(event)
        let decoded = try FixtureJSON.decode(SemanticEvent.self, from: encoded)
        XCTAssertEqual(decoded, event)

        let jsonObject = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        var futureObject = jsonObject
        futureObject["contractVersion"] = ["major": 9, "minor": 0]
        let futureData = try JSONSerialization.data(withJSONObject: futureObject)
        XCTAssertThrowsError(try FixtureJSON.decode(SemanticEvent.self, from: futureData))
    }

    func testCheckedInSemanticFixtureUsesTheVersionedJSONContract() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "scene-changed", withExtension: "json"))
        let event = try FixtureJSON.decode(SemanticEvent.self, from: Data(contentsOf: url))
        XCTAssertEqual(event.contractVersion, .phase1)
        XCTAssertEqual(event.kind, .sceneChanged)
        XCTAssertEqual(event.metadata["sceneID"], "crossroads")
    }

    func testAcceptanceClearsOnlyMatchingDraftAndKeepsNewEdits() {
        let sessionID = SessionID("session-acceptance")
        var state = SessionState(sessionID: sessionID, draft: Draft(id: DraftID("draft-1"), text: "look around"))
        var reducer = SessionReducer()
        let requestID = LogicalRequestID("request-1")
        let attemptID = ExecutionAttemptID("attempt-1")

        let command = SemanticEvent(
            eventID: SemanticEventID("event-command"),
            sessionID: sessionID,
            sequence: 1,
            kind: .playerCommand,
            content: "look around",
            logicalRequestID: requestID,
            attemptID: attemptID,
            transcriptItemID: TranscriptItemID("command-1"),
            accepted: true,
            sourceDraftID: DraftID("draft-1")
        )
        XCTAssertEqual(reducer.reduce(.apply(command), in: &state), .applied)
        XCTAssertEqual(state.draft.text, "")

        XCTAssertEqual(reducer.reduce(.updateDraft("a new unsent command"), in: &state), .applied)
        let lateAcceptance = SemanticEvent(
            eventID: SemanticEventID("event-late-acceptance"),
            sessionID: sessionID,
            sequence: 2,
            kind: .playerCommand,
            content: "look around",
            logicalRequestID: LogicalRequestID("request-2"),
            attemptID: ExecutionAttemptID("attempt-2"),
            transcriptItemID: TranscriptItemID("command-2"),
            accepted: true,
            sourceDraftID: DraftID("draft-1")
        )
        XCTAssertEqual(reducer.reduce(.apply(lateAcceptance), in: &state), .applied)
        XCTAssertEqual(state.draft.text, "a new unsent command")
    }

    func testStreamingUpdatesOneItemAndRejectsDuplicateOrObsoleteChunks() {
        let sessionID = SessionID("session-stream")
        let requestID = LogicalRequestID("request-1")
        let attemptID = ExecutionAttemptID("attempt-1")
        let itemID = TranscriptItemID("response-1")
        var state = SessionState(sessionID: sessionID)
        var reducer = SessionReducer()

        let command = SemanticEvent(
            eventID: SemanticEventID("event-command"),
            sessionID: sessionID,
            sequence: 1,
            kind: .playerCommand,
            content: "ask Peig",
            logicalRequestID: requestID,
            attemptID: attemptID,
            transcriptItemID: TranscriptItemID("command"),
            accepted: true
        )
        _ = reducer.reduce(.apply(command), in: &state)

        let first = SemanticEvent(
            eventID: SemanticEventID("event-chunk-1"),
            sessionID: sessionID,
            sequence: 2,
            kind: .npcDialogue,
            content: "Á—",
            speaker: "Peig",
            logicalRequestID: requestID,
            attemptID: attemptID,
            transcriptItemID: itemID,
            provisional: true,
            streamSequence: 1
        )
        XCTAssertEqual(reducer.reduce(.apply(first), in: &state), .applied)

        let second = SemanticEvent(
            eventID: SemanticEventID("event-chunk-2"),
            sessionID: sessionID,
            sequence: 3,
            kind: .npcDialogue,
            content: "Á—你好",
            speaker: "Peig",
            logicalRequestID: requestID,
            attemptID: attemptID,
            transcriptItemID: itemID,
            provisional: true,
            streamSequence: 2
        )
        XCTAssertEqual(reducer.reduce(.apply(second), in: &state), .applied)
        XCTAssertEqual(state.transcript.first(where: { $0.id == itemID })?.content, "Á—你好")

        XCTAssertEqual(reducer.reduce(.apply(second), in: &state), .ignoredDuplicate)
        let obsolete = SemanticEvent(
            eventID: SemanticEventID("event-obsolete"),
            sessionID: sessionID,
            sequence: 4,
            kind: .npcDialogue,
            content: "old",
            logicalRequestID: requestID,
            attemptID: attemptID,
            transcriptItemID: itemID,
            provisional: true,
            streamSequence: 1
        )
        XCTAssertEqual(reducer.reduce(.apply(obsolete), in: &state), .ignoredObsoleteEvent)
        XCTAssertEqual(state.transcript.first(where: { $0.id == itemID })?.content, "Á—你好")
    }

    func testUniqueOutOfOrderEventAdvancesDeduplicationWithoutRewindingCursor() {
        let sessionID = SessionID("session-order")
        var state = SessionState(sessionID: sessionID, eventCursor: EventCursor(2))
        var reducer = SessionReducer()
        let oldEvent = SemanticEvent(
            eventID: SemanticEventID("old-event"), sessionID: sessionID, sequence: 1,
            kind: .narration, content: "late old text", transcriptItemID: TranscriptItemID("old")
        )

        XCTAssertEqual(reducer.reduce(.apply(oldEvent), in: &state), .ignoredObsoleteEvent)
        XCTAssertEqual(state.eventCursor, EventCursor(2))
        XCTAssertTrue(state.processedEventIDs.contains(oldEvent.eventID))
        XCTAssertTrue(state.transcript.isEmpty)
    }

    func testCompletionCommitsAndLateOldAttemptCannotWin() {
        let sessionID = SessionID("session-terminal")
        let requestID = LogicalRequestID("request-1")
        let attemptID = ExecutionAttemptID("attempt-1")
        var state = SessionState(sessionID: sessionID)
        var reducer = SessionReducer()
        let command = SemanticEvent(
            eventID: SemanticEventID("command"), sessionID: sessionID, sequence: 1,
            kind: .playerCommand, content: "ask", logicalRequestID: requestID,
            attemptID: attemptID, transcriptItemID: TranscriptItemID("command"), accepted: true
        )
        let chunk = SemanticEvent(
            eventID: SemanticEventID("chunk"), sessionID: sessionID, sequence: 2,
            kind: .npcDialogue, content: "A complete answer", logicalRequestID: requestID,
            attemptID: attemptID, transcriptItemID: TranscriptItemID("answer"), provisional: true, streamSequence: 1
        )
        let completion = SemanticEvent(
            eventID: SemanticEventID("completion"), sessionID: sessionID, sequence: 3,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: attemptID,
            terminalOutcome: .succeeded, stateRevision: StateRevision(4)
        )
        _ = reducer.reduce(.apply(command), in: &state)
        _ = reducer.reduce(.apply(chunk), in: &state)
        XCTAssertEqual(reducer.reduce(.apply(completion), in: &state), .applied)
        XCTAssertEqual(state.request(for: requestID)?.phase, .completed)
        XCTAssertEqual(state.request(for: requestID)?.committedStateRevision, StateRevision(4))
        XCTAssertEqual(state.transcript.first(where: { $0.id == TranscriptItemID("answer") })?.state, .committed)

        let lateChunk = SemanticEvent(
            eventID: SemanticEventID("late"), sessionID: sessionID, sequence: 4,
            kind: .npcDialogue, content: "late mutation", logicalRequestID: requestID,
            attemptID: attemptID, transcriptItemID: TranscriptItemID("answer"), provisional: true, streamSequence: 2
        )
        XCTAssertEqual(reducer.reduce(.apply(lateChunk), in: &state), .ignoredCommittedRequest)
        XCTAssertEqual(state.transcript.first(where: { $0.id == TranscriptItemID("answer") })?.content, "A complete answer")
    }

    @MainActor
    func testTailReplaySettlesResponseWhenAcceptedCommandIsOutsideTail() {
        let sessionID = SessionID("session-tail-replay")
        let requestID = LogicalRequestID("request-tail")
        let attemptID = ExecutionAttemptID("attempt-tail")
        let responseItemID = TranscriptItemID("response-tail")
        let response = SemanticEvent(
            eventID: SemanticEventID("tail-response"), sessionID: sessionID, sequence: 2,
            kind: .npcDialogue, content: "The final answer remains visible.", speaker: "Peig",
            logicalRequestID: requestID, attemptID: attemptID, transcriptItemID: responseItemID,
            provisional: true, streamSequence: 1
        )
        let completion = SemanticEvent(
            eventID: SemanticEventID("tail-completion"), sessionID: sessionID, sequence: 3,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: attemptID,
            terminalOutcome: .succeeded, stateRevision: StateRevision(1)
        )
        let presentation = PresentationSession(state: SessionState(sessionID: sessionID))

        XCTAssertEqual(presentation.apply(response), .applied)
        XCTAssertEqual(presentation.apply(completion), .applied)
        XCTAssertEqual(
            presentation.state.transcript.first(where: { $0.id == responseItemID })?.content,
            "The final answer remains visible."
        )
        XCTAssertEqual(
            presentation.state.transcript.first(where: { $0.id == responseItemID })?.state,
            .committed
        )
    }

    func testRetryCreatesCurrentAttemptBeforeRejectingOldAttemptEvents() {
        let sessionID = SessionID("session-retry")
        let requestID = LogicalRequestID("request-1")
        let firstAttempt = ExecutionAttemptID("attempt-1")
        let retryAttempt = ExecutionAttemptID("attempt-2")
        var state = SessionState(sessionID: sessionID)
        var reducer = SessionReducer()
        let command = SemanticEvent(
            eventID: SemanticEventID("command"), sessionID: sessionID, sequence: 1,
            kind: .playerCommand, content: "try again", logicalRequestID: requestID,
            attemptID: firstAttempt, transcriptItemID: TranscriptItemID("command"), accepted: true
        )
        let failed = SemanticEvent(
            eventID: SemanticEventID("failed"), sessionID: sessionID, sequence: 2,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: firstAttempt,
            terminalOutcome: .failed
        )
        let retryStarted = SemanticEvent(
            eventID: SemanticEventID("retry"), sessionID: sessionID, sequence: 3,
            kind: .progress, content: "Retrying", logicalRequestID: requestID,
            attemptID: retryAttempt, metadata: ["retry": "true"]
        )
        _ = reducer.reduce(.apply(command), in: &state)
        _ = reducer.reduce(.apply(failed), in: &state)
        XCTAssertEqual(reducer.reduce(.apply(retryStarted), in: &state), .applied)
        XCTAssertEqual(state.request(for: requestID)?.currentAttemptID, retryAttempt)
        XCTAssertEqual(state.request(for: requestID)?.phase, .executing)
        XCTAssertNil(state.request(for: requestID)?.terminalOutcome)
        XCTAssertNil(state.request(for: requestID)?.committedStateRevision)

        let oldCompletion = SemanticEvent(
            eventID: SemanticEventID("old-completion"), sessionID: sessionID, sequence: 4,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: firstAttempt,
            terminalOutcome: .succeeded, stateRevision: StateRevision(99)
        )
        XCTAssertEqual(reducer.reduce(.apply(oldCompletion), in: &state), .ignoredObsoleteEvent)
        XCTAssertEqual(state.request(for: requestID)?.phase, .executing)
        XCTAssertNil(state.request(for: requestID)?.committedStateRevision)

        let retryCompletion = SemanticEvent(
            eventID: SemanticEventID("retry-completion"), sessionID: sessionID, sequence: 5,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: retryAttempt,
            terminalOutcome: .succeeded, stateRevision: StateRevision(5)
        )
        XCTAssertEqual(reducer.reduce(.apply(retryCompletion), in: &state), .applied)
        XCTAssertEqual(state.request(for: requestID)?.phase, .completed)
        XCTAssertEqual(state.request(for: requestID)?.terminalOutcome, .succeeded)
        XCTAssertEqual(state.request(for: requestID)?.committedStateRevision, StateRevision(5))
    }

    func testLateCallbacksFromCancelledAttemptCannotCommit() {
        let sessionID = SessionID("session-cancel-race")
        let requestID = LogicalRequestID("request-1")
        let attemptID = ExecutionAttemptID("attempt-1")
        let itemID = TranscriptItemID("answer")
        var state = SessionState(sessionID: sessionID)
        var reducer = SessionReducer()

        let command = SemanticEvent(
            eventID: SemanticEventID("command"), sessionID: sessionID, sequence: 1,
            kind: .playerCommand, content: "ask", logicalRequestID: requestID,
            attemptID: attemptID, transcriptItemID: TranscriptItemID("command"), accepted: true
        )
        let chunk = SemanticEvent(
            eventID: SemanticEventID("chunk"), sessionID: sessionID, sequence: 2,
            kind: .npcDialogue, content: "partial", logicalRequestID: requestID,
            attemptID: attemptID, transcriptItemID: itemID, provisional: true, streamSequence: 1
        )
        let cancelled = SemanticEvent(
            eventID: SemanticEventID("cancelled"), sessionID: sessionID, sequence: 3,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: attemptID,
            terminalOutcome: .cancelled
        )
        _ = reducer.reduce(.apply(command), in: &state)
        _ = reducer.reduce(.apply(chunk), in: &state)
        XCTAssertEqual(reducer.reduce(.apply(cancelled), in: &state), .applied)
        XCTAssertEqual(state.request(for: requestID)?.phase, .cancelled)
        XCTAssertEqual(state.transcript.first(where: { $0.id == itemID })?.state, .cancelled)

        let lateChunk = SemanticEvent(
            eventID: SemanticEventID("late-chunk"), sessionID: sessionID, sequence: 4,
            kind: .npcDialogue, content: "late mutation", logicalRequestID: requestID,
            attemptID: attemptID, transcriptItemID: itemID, provisional: true, streamSequence: 2
        )
        let lateCompletion = SemanticEvent(
            eventID: SemanticEventID("late-success"), sessionID: sessionID, sequence: 5,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: attemptID,
            terminalOutcome: .succeeded, stateRevision: StateRevision(99)
        )
        XCTAssertEqual(reducer.reduce(.apply(lateChunk), in: &state), .ignoredObsoleteEvent)
        XCTAssertEqual(reducer.reduce(.apply(lateCompletion), in: &state), .ignoredObsoleteEvent)
        XCTAssertEqual(state.request(for: requestID)?.phase, .cancelled)
        XCTAssertNil(state.request(for: requestID)?.committedStateRevision)
        XCTAssertEqual(state.transcript.first(where: { $0.id == itemID })?.content, "partial")
    }

    func testFailedCompletionCannotAdvanceCommittedStateRevision() {
        let sessionID = SessionID("session-failed-revision")
        let requestID = LogicalRequestID("request-1")
        let attemptID = ExecutionAttemptID("attempt-1")
        var state = SessionState(sessionID: sessionID)
        var reducer = SessionReducer()
        let command = SemanticEvent(
            eventID: SemanticEventID("command"), sessionID: sessionID, sequence: 1,
            kind: .playerCommand, content: "fail", logicalRequestID: requestID,
            attemptID: attemptID, transcriptItemID: TranscriptItemID("command"), accepted: true
        )
        _ = reducer.reduce(.apply(command), in: &state)

        let failed = SemanticEvent(
            eventID: SemanticEventID("failed"), sessionID: sessionID, sequence: 2,
            kind: .responseCompleted, logicalRequestID: requestID, attemptID: attemptID,
            terminalOutcome: .failed, stateRevision: StateRevision(99)
        )
        XCTAssertEqual(reducer.reduce(.apply(failed), in: &state), .applied)
        XCTAssertEqual(state.stateRevision, StateRevision(0))
    }

    func testViewportPreservesHistoryAnchorAndShowsNewText() {
        let sessionID = SessionID("session-viewport")
        let item = TranscriptItem(
            id: TranscriptItemID("old"), kind: .narration, content: "old", state: .committed, lastEventSequence: 1
        )
        var state = SessionState(sessionID: sessionID, transcript: [item])
        var reducer = SessionReducer()
        _ = reducer.reduce(.readHistory(anchor: TranscriptAnchor(itemID: item.id, offset: 18)), in: &state)

        let incoming = SemanticEvent(
            eventID: SemanticEventID("incoming"), sessionID: sessionID, sequence: 2,
            kind: .narration, content: "new text", transcriptItemID: TranscriptItemID("new")
        )
        XCTAssertEqual(reducer.reduce(.apply(incoming), in: &state), .applied)
        XCTAssertFalse(state.viewport.isFollowingNewest)
        XCTAssertEqual(state.viewport.anchor?.itemID, item.id)
        XCTAssertTrue(state.viewport.hasNewText)
        XCTAssertEqual(state.viewport.unreadCount, 1)

        _ = reducer.reduce(.followNewest, in: &state)
        XCTAssertTrue(state.viewport.isFollowingNewest)
        XCTAssertFalse(state.viewport.hasNewText)
    }

    func testBoundedTranscriptAndHistoryRecall() {
        let sessionID = SessionID("session-bounded")
        var state = SessionState(sessionID: sessionID, transcriptCapacity: 2)
        var reducer = SessionReducer()
        for index in 1...4 {
            let requestID = LogicalRequestID("request-\(index)")
            let event = SemanticEvent(
                eventID: SemanticEventID("event-\(index)"), sessionID: sessionID, sequence: EventSequence(UInt64(index)),
                kind: .playerCommand, content: "command \(index)", logicalRequestID: requestID,
                attemptID: ExecutionAttemptID("attempt-\(index)"), transcriptItemID: TranscriptItemID("command-\(index)"), accepted: true
            )
            _ = reducer.reduce(.apply(event), in: &state)
        }
        XCTAssertEqual(state.transcript.count, 2)
        XCTAssertTrue(state.hasOlderTranscript)
        XCTAssertEqual(state.commandHistory.count, 4)
        _ = reducer.reduce(.recallCommand(LogicalRequestID("request-2")), in: &state)
        XCTAssertEqual(state.draft.text, "command 2")
    }

    func testLoadingOlderTranscriptShowsTheOlderPageWithinTheBoundedWindow() {
        let sessionID = SessionID("session-older-page")
        let currentItems = (3...4).map {
            TranscriptItem(
                id: TranscriptItemID("item-\($0)"), kind: .narration, content: "entry \($0)",
                state: .committed, lastEventSequence: EventSequence(UInt64($0))
            )
        }
        let olderItems = (1...2).map {
            TranscriptItem(
                id: TranscriptItemID("item-\($0)"), kind: .narration, content: "entry \($0)",
                state: .committed, lastEventSequence: EventSequence(UInt64($0))
            )
        }
        var state = SessionState(sessionID: sessionID, transcript: currentItems, transcriptCapacity: 2)
        var reducer = SessionReducer()

        XCTAssertEqual(
            reducer.reduce(.loadOlderTranscript(items: olderItems, hasOlderItems: false), in: &state),
            .applied
        )
        XCTAssertEqual(state.transcript.map(\.id.rawValue), ["item-1", "item-2"])
        XCTAssertFalse(state.hasOlderTranscript)
    }

    func testCompletionRegistrySupportsSlashNPCAndAccents() {
        let registry = FixtureCompletionRegistry.phase1
        XCTAssertEqual(registry.suggestions(for: "/lo").map(\.id), ["look"])
        let npcSuggestions = registry.suggestions(for: "ask @mic")
        XCTAssertEqual(npcSuggestions.first?.entityID, "npc-micheal")
        XCTAssertEqual(registry.applying(npcSuggestions[0], to: "ask @mic"), "ask @Mícheál Connolly")
        XCTAssertTrue(registry.suggestions(for: "ask @ró").contains { $0.entityID == "npc-roisin" })
    }

    func testFixtureAdapterManualStreamingStopAndRetry() async throws {
        let sessionID = SessionID("session-adapter")
        let adapter = FixtureSessionAdapter(sessionID: sessionID, script: .phase1)
        _ = await adapter.allEvents() // materialize the opening fixture before subscribing
        let stream = await adapter.events(after: await adapter.currentCursor)
        var iterator = stream.makeAsyncIterator()
        let draftID = DraftID("draft")
        let receipt = try await adapter.submit(text: "long stream", draftID: draftID, logicalRequestID: LogicalRequestID("request"))
        let command = try await iterator.next()
        XCTAssertEqual(command?.kind, .playerCommand)
        XCTAssertEqual(command?.sourceDraftID, draftID)
        XCTAssertEqual(command?.attemptID, receipt.attemptID)

        _ = await adapter.step()
        _ = await adapter.step()
        let stop = await adapter.stop()
        XCTAssertEqual(stop.result, .cancelled)
        let isStreaming = await adapter.isStreaming
        XCTAssertFalse(isStreaming)

        let retry = try await adapter.retry(logicalRequestID: receipt.logicalRequestID)
        XCTAssertNotEqual(retry.attemptID, receipt.attemptID)
        XCTAssertTrue(retry.isRetry)
        _ = await adapter.stop()
        await adapter.finishEventStream()
    }

    func testFixtureEventStreamFailsExplicitlyWhenConsumerFallsBehind() async throws {
        let adapter = FixtureSessionAdapter(
            sessionID: SessionID("session-overflow"),
            script: .longHistory(count: FixtureSessionAdapter.eventBufferCapacity + 32)
        )
        let stream = await adapter.events()
        var iterator = stream.makeAsyncIterator()

        do {
            while let _ = try await iterator.next() { }
            XCTFail("the bounded stream should terminate with an overflow")
        } catch let error as FixtureAdapterError {
            XCTAssertEqual(error, .eventBufferOverflow)
        }
    }

    func testFixtureClarificationContinuesSameLogicalRequest() async throws {
        let sessionID = SessionID("session-clarify")
        let adapter = FixtureSessionAdapter(sessionID: sessionID, script: .phase1)
        let stream = await adapter.events()
        var iterator = stream.makeAsyncIterator()
        // Opening scene + narration.
        _ = try await iterator.next(); _ = try await iterator.next()
        let requestID = LogicalRequestID("clarify-request")
        let receipt = try await adapter.submit(text: "ambiguous", draftID: nil, logicalRequestID: requestID)
        _ = try await iterator.next() // command
        _ = await adapter.step() // interpretation
        _ = try await iterator.next()
        _ = await adapter.step() // clarification
        _ = try await iterator.next()
        let answer = try await adapter.answerClarification(logicalRequestID: requestID, choiceID: "micheal")
        XCTAssertEqual(answer.logicalRequestID, requestID)
        XCTAssertEqual(answer.attemptID, receipt.attemptID)
        let selected = try await iterator.next()
        XCTAssertEqual(selected?.kind, .clarificationSelected)
        XCTAssertEqual(selected?.metadata["choiceID"], "micheal")
        await adapter.finishEventStream()
    }

    func testFixtureClarificationDoesNotInventTheSelectedSpeaker() async throws {
        let adapter = FixtureSessionAdapter(sessionID: SessionID("session-clarify-roisin"), script: .phase1)
        let requestID = LogicalRequestID("clarify-roisin-request")
        _ = try await adapter.submit(text: "ambiguous", draftID: nil, logicalRequestID: requestID)
        let pending = await adapter.runUntilFinished()
        XCTAssertTrue(pending.contains { $0.kind == .clarificationRequired })

        let answer = try await adapter.answerClarification(logicalRequestID: requestID, choiceID: "roisin")
        let continuation = await adapter.runUntilFinished()
        XCTAssertEqual(answer.logicalRequestID, requestID)
        XCTAssertTrue(continuation.allSatisfy { $0.logicalRequestID == requestID })
        XCTAssertTrue(continuation.contains { $0.kind == .actionResult })
        XCTAssertFalse(continuation.contains { $0.speaker == "Mícheál Connolly" })
        XCTAssertEqual(continuation.last?.kind, .responseCompleted)
    }

    func testEveryRegisteredSlashCommandHasAuthoredDeterministicOutput() async throws {
        for (index, command) in FixtureCompletionRegistry.phase1.slashCommands.enumerated() {
            let adapter = FixtureSessionAdapter(
                sessionID: SessionID("session-slash-\(index)"),
                script: .phase1
            )
            _ = try await adapter.submit(text: command.insertionText, draftID: nil, logicalRequestID: nil)
            let events = await adapter.runUntilFinished()
            XCTAssertTrue(events.contains { $0.kind == .actionResult }, "Missing authored output for \(command.insertionText)")
            XCTAssertFalse(events.contains { $0.content == FixtureScript.phase1.defaultPlan.steps[1].content })
        }
    }

    func testFixtureEventsDriveTheSameReducerPathAsLiveDelivery() async throws {
        let sessionID = SessionID("session-fixture-reducer")
        let adapter = FixtureSessionAdapter(sessionID: sessionID, script: .phase1)
        var state = SessionState(sessionID: sessionID)
        var reducer = SessionReducer()
        for event in await adapter.allEvents() {
            XCTAssertEqual(reducer.reduce(.apply(event), in: &state), .applied)
        }

        let requestID = LogicalRequestID("fixture-request")
        let receipt = try await adapter.submit(text: "ask Peig about the old church", draftID: nil, logicalRequestID: requestID)
        for event in await adapter.allEvents().filter({ $0.logicalRequestID == requestID }) {
            _ = reducer.reduce(.apply(event), in: &state)
        }
        while await adapter.isStreaming {
            let result = await adapter.step()
            if let event = result.event {
                XCTAssertEqual(reducer.reduce(.apply(event), in: &state), .applied)
            }
        }

        XCTAssertEqual(state.request(for: requestID)?.currentAttemptID, receipt.attemptID)
        XCTAssertEqual(state.request(for: requestID)?.phase, .completed)
        let dialogue = state.transcript.filter { $0.kind == .npcDialogue }
        XCTAssertEqual(dialogue.count, 1)
        XCTAssertEqual(dialogue.first?.state, .committed)
        XCTAssertEqual(dialogue.first?.content, "The old church? It stands beyond the alder trees, where the path bends toward the hill.")
    }

    func testRestoredAdapterSuppressesOpeningReplayAndInterruptsActiveAttempt() async throws {
        let sessionID = SessionID("session-restore-adapter")
        let requestID = LogicalRequestID("request-restore")
        let attemptID = ExecutionAttemptID("attempt-restore")
        let request = RequestRecord(
            id: requestID,
            originalText: "long stream",
            attempts: [RequestAttempt(
                id: attemptID,
                originalText: "long stream",
                phase: .executing,
                provisionalItemIDs: [TranscriptItemID("answer")],
                startedAt: EventSequence(2)
            )],
            currentAttemptID: attemptID,
            phase: .executing
        )
        let state = SessionState(
            sessionID: sessionID,
            eventCursor: EventCursor(10),
            transcript: [TranscriptItem(
                id: TranscriptItemID("answer"), kind: .npcDialogue, content: "partial", state: .provisional,
                lastEventSequence: EventSequence(9)
            )],
            requests: [request],
            activeRequestID: requestID
        )
        let adapter = FixtureSessionAdapter(script: .phase1, restoring: state)
        let restoredEvents = await adapter.allEvents()
        XCTAssertEqual(restoredEvents.count, 1)
        let restoredCursor = await adapter.currentCursor
        XCTAssertEqual(restoredCursor, EventCursor(11))
        let stream = await adapter.events(after: state.eventCursor)
        var iterator = stream.makeAsyncIterator()
        let interrupted = try await iterator.next()
        XCTAssertEqual(interrupted?.kind, .responseCompleted)
        XCTAssertEqual(interrupted?.terminalOutcome, .interrupted)
        XCTAssertEqual(interrupted?.sequence, EventSequence(11))
        XCTAssertEqual(interrupted?.content, "Interrupted before completion.")
        XCTAssertNotNil(interrupted?.transcriptItemID)

        var restoredState = state
        var reducer = SessionReducer()
        XCTAssertEqual(reducer.reduce(.apply(interrupted!), in: &restoredState), .applied)
        XCTAssertEqual(restoredState.request(for: requestID)?.phase, .interrupted)
        XCTAssertEqual(restoredState.transcript.first?.state, .interrupted)
        XCTAssertEqual(restoredState.transcript.last?.kind, .responseCompleted)
        XCTAssertEqual(restoredState.transcript.last?.content, "Interrupted before completion.")
        XCTAssertEqual(restoredState.transcript.last?.state, .interrupted)

        let retry = try await adapter.retry(logicalRequestID: requestID)
        XCTAssertNotEqual(retry.attemptID, attemptID)
        XCTAssertTrue(retry.isRetry)
        _ = await adapter.stop()

        let idleRestored = FixtureSessionAdapter(
            script: .phase1,
            restoring: SessionState(sessionID: SessionID("idle-restore"), eventCursor: EventCursor(42))
        )
        let idleCursor = await idleRestored.currentCursor
        XCTAssertEqual(idleCursor, EventCursor(42))
    }

    func testFixturePersistenceRestoresDraftAndLeavesUnsupportedSaveUntouched() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("rundale-kit-tests-\(UUID().uuidString)", isDirectory: true)
        let sessionURL = directory.appendingPathComponent("session.json")
        let draftURL = directory.appendingPathComponent("draft.json")
        let state = SessionState(
            sessionID: SessionID("persisted"),
            draft: Draft(id: DraftID("draft"), text: "unfinished command")
        )
        let store = FixtureSessionStore(fileURL: sessionURL)
        let draftStore = FixtureDraftStore(fileURL: draftURL)
        try store.save(state, savedAt: Date(timeIntervalSince1970: 123))
        try draftStore.save(state.draft)
        XCTAssertEqual(try store.restore().draft, state.draft)
        XCTAssertEqual(try draftStore.restore(), state.draft)

        let original = try Data(contentsOf: sessionURL)
        var object = try XCTUnwrap(JSONSerialization.jsonObject(with: original) as? [String: Any])
        object["formatVersion"] = ["major": 99, "minor": 0]
        let unsupported = try JSONSerialization.data(withJSONObject: object)
        try unsupported.write(to: sessionURL, options: [.atomic])
        let beforeRestoreFailure = try Data(contentsOf: sessionURL)
        XCTAssertThrowsError(try store.restore())
        XCTAssertEqual(try Data(contentsOf: sessionURL), beforeRestoreFailure)

        try FileManager.default.removeItem(at: directory)
    }

    func testSnapshotWriterOrdersWritesByGenerationEvenWhenCursorIsEqual() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("rundale-kit-writer-\(UUID().uuidString)", isDirectory: true)
        let store = FixtureSessionStore(fileURL: directory.appendingPathComponent("session.json"))
        let writer = FixtureSessionSnapshotWriter(store: store)
        let sessionID = SessionID("writer-session")
        let older = SessionState(
            sessionID: sessionID,
            eventCursor: EventCursor(8),
            draft: Draft(id: DraftID("draft"), text: "older viewport")
        )
        let newer = SessionState(
            sessionID: sessionID,
            eventCursor: EventCursor(8),
            draft: Draft(id: DraftID("draft"), text: "newer viewport")
        )

        let newerError = await writer.save(state: newer, generation: 2)
        let olderError = await writer.save(state: older, generation: 1)
        XCTAssertNil(newerError)
        XCTAssertNil(olderError)
        XCTAssertEqual(try store.restore().draft.text, "newer viewport")

        let equalGeneration = SessionState(
            sessionID: sessionID,
            eventCursor: EventCursor(8),
            draft: Draft(id: DraftID("draft"), text: "equal generation")
        )
        let equalError = await writer.save(state: equalGeneration, generation: 2)
        XCTAssertNil(equalError)
        XCTAssertEqual(try store.restore().draft.text, "newer viewport")

        let newest = SessionState(
            sessionID: sessionID,
            eventCursor: EventCursor(8),
            draft: Draft(id: DraftID("draft"), text: "newest viewport")
        )
        let newestError = await writer.save(state: newest, generation: 3)
        XCTAssertNil(newestError)
        XCTAssertEqual(try store.restore().draft.text, "newest viewport")

        try FileManager.default.removeItem(at: directory)
    }
}
