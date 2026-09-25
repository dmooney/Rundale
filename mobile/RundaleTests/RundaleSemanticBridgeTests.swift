import Foundation
import RundaleBridge
import RundaleKit
import XCTest

/// Crosses the app's packaged Rust FFI, JSON decoder, and presentation reducer.
/// The standalone RundaleBridge test target links C doubles instead.
@MainActor
final class RundaleSemanticBridgeTests: XCTestCase {
    private struct Snapshot: Decodable {
        let sessionID: SessionID
        let events: [SemanticEvent]
    }

    func testLocalCommandKeepsRustRequestIdentityThroughPresentationAndReplay() async throws {
        let (runtime, directory, payload) = try makeRuntime()
        defer { try? FileManager.default.removeItem(at: directory) }

        let initial = try FixtureJSON.decode(Snapshot.self, from: await runtime.snapshotJSON())
        let receipt = try await runtime.submit(text: "/look", draftID: DraftID(), logicalRequestID: nil)
        XCTAssertTrue(receipt.accepted)

        let page = try await runtime.readEventPage()
        let command = try XCTUnwrap(page.events.first {
            $0.kind == .playerCommand && $0.logicalRequestID == receipt.logicalRequestID
        })
        let terminal = try XCTUnwrap(page.events.first {
            $0.kind == .responseCompleted && $0.logicalRequestID == receipt.logicalRequestID
        })
        XCTAssertEqual(receipt.commandEventID, command.eventID)
        XCTAssertEqual(command.attemptID, receipt.attemptID)
        XCTAssertEqual(terminal.attemptID, receipt.attemptID)
        XCTAssertEqual(terminal.terminalOutcome, .succeeded)
        XCTAssertEqual(Set(page.events.map(\.eventID)).count, page.events.count)

        let presentation = PresentationSession(state: SessionState(sessionID: initial.sessionID))
        for event in page.events { presentation.apply(event) }
        let record = try XCTUnwrap(presentation.state.request(for: receipt.logicalRequestID))
        XCTAssertEqual(record.phase, .completed)
        XCTAssertEqual(record.currentAttemptID, receipt.attemptID)
        XCTAssertEqual(record.currentAttempt?.terminalEventID, terminal.eventID)
        XCTAssertTrue(presentation.state.transcript.contains {
            $0.kind == .playerCommand && $0.logicalRequestID == receipt.logicalRequestID
        })
        let transcriptIDs = presentation.state.transcript.map(\.id)
        XCTAssertEqual(presentation.apply(command), .ignoredDuplicate)
        XCTAssertEqual(presentation.apply(terminal), .ignoredDuplicate)
        XCTAssertEqual(presentation.state.transcript.map(\.id), transcriptIDs)

        try await runtime.close()
        let resumed = try ParishRuntime.openResume(payload: payload)
        let restoredSnapshot = try FixtureJSON.decode(Snapshot.self, from: await resumed.snapshotJSON())
        let restoredPage = try await resumed.readEventPage()
        XCTAssertEqual(restoredSnapshot.sessionID, initial.sessionID)
        XCTAssertEqual(restoredPage.events.map(\.eventID), page.events.map(\.eventID))
        let replay = PresentationSession(state: SessionState(sessionID: restoredSnapshot.sessionID))
        for event in restoredPage.events { replay.apply(event) }
        XCTAssertEqual(replay.state.transcript.map(\.id), transcriptIDs)
        XCTAssertEqual(replay.state.request(for: receipt.logicalRequestID)?.phase, .completed)
        try await resumed.close()
    }

    func testRustClarificationSurvivesResumeAndSelectionClearsPresentationPrompt() async throws {
        let (runtime, directory, payload) = try makeRuntime()
        defer { try? FileManager.default.removeItem(at: directory) }

        let travel = try await runtime.submit(text: "/go Connolly Cottage", draftID: DraftID(), logicalRequestID: nil)
        let request = try await runtime.submit(
            text: "ask Connolly about the household", draftID: DraftID(), logicalRequestID: nil
        )
        XCTAssertTrue(travel.accepted)
        XCTAssertTrue(request.accepted)

        let before = try await runtime.readEventPage()
        let prompt = try XCTUnwrap(before.events.first {
            $0.kind == .clarificationRequired && $0.logicalRequestID == request.logicalRequestID
        })
        XCTAssertEqual(prompt.attemptID, request.attemptID)
        let choice = try XCTUnwrap(prompt.clarification?.choices.first)
        let snapshot = try FixtureJSON.decode(Snapshot.self, from: await runtime.snapshotJSON())
        let presentation = PresentationSession(state: SessionState(sessionID: snapshot.sessionID))
        for event in before.events { presentation.apply(event) }
        XCTAssertEqual(presentation.state.pendingClarification?.requestID, request.logicalRequestID)
        XCTAssertEqual(presentation.state.pendingClarification?.prompt, prompt.clarification)
        XCTAssertEqual(presentation.state.request(for: request.logicalRequestID)?.phase, .awaitingClarification)

        try await runtime.close()
        let resumed = try ParishRuntime.openResume(payload: payload)
        let restoredSnapshot = try FixtureJSON.decode(Snapshot.self, from: await resumed.snapshotJSON())
        let restored = try await resumed.readEventPage()
        XCTAssertEqual(restoredSnapshot.sessionID, snapshot.sessionID)
        XCTAssertEqual(restored.events.map(\.eventID), before.events.map(\.eventID))
        let replay = PresentationSession(state: SessionState(sessionID: restoredSnapshot.sessionID))
        for event in restored.events { replay.apply(event) }
        XCTAssertEqual(replay.state.pendingClarification?.prompt, prompt.clarification)

        let continuation = try await resumed.answerClarification(
            logicalRequestID: request.logicalRequestID, choiceID: choice.id
        )
        XCTAssertEqual(continuation.logicalRequestID, request.logicalRequestID)
        let after = try await resumed.readEventPage(after: restored.nextCursor)
        let selection = try XCTUnwrap(after.events.first {
            $0.kind == .clarificationSelected && $0.logicalRequestID == request.logicalRequestID
        })
        XCTAssertEqual(selection.attemptID, continuation.attemptID)
        for event in after.events { replay.apply(event) }
        XCTAssertNil(replay.state.pendingClarification)
        XCTAssertNotEqual(replay.state.request(for: request.logicalRequestID)?.phase, .awaitingClarification)
        try await resumed.close()
    }

    /// #1993 / P2-F02, P2-F04, P2-F10: the Intent stage, its receipt, a
    /// provisional frame, and a differing validated final all cross the real
    /// FFI and reducer, keeping one request, one attempt, and one dialogue row.
    func testIntentReceiptStreamAndDifferingFinalProjectThroughReducer() async throws {
        let (runtime, directory, _) = try makeRuntime()
        defer { try? FileManager.default.removeItem(at: directory) }
        let initial = try FixtureJSON.decode(Snapshot.self, from: await runtime.snapshotJSON())
        let presentation = PresentationSession(state: SessionState(sessionID: initial.sessionID))
        for event in initial.events { presentation.apply(event) }

        let receipt = try await runtime.submit(text: "Would Peig know anything of the post today?")
        let intent = try await pendingInvocation(runtime)
        XCTAssertEqual(intent["role"] as? String, "intent")
        XCTAssertEqual(intent["attemptID"] as? String, receipt.attemptID.rawValue)
        XCTAssertNil(intent["speaker"], "The Intent stage must not select a speaker")

        var events = try await runtime.readEventPage().events
        events += try await operation(runtime, [
            "op": "receive_intent_candidate",
            "attempt_id": receipt.attemptID.rawValue,
            "base_revision": try baseRevision(intent),
            "output": ["intent": "talk", "target": "Peig", "dialogue": NSNull(), "atmosphere": NSNull()],
            "structured": true
        ]).events
        let dialogue = try await pendingInvocation(runtime)
        XCTAssertEqual(dialogue["role"] as? String, "npc_dialogue")
        XCTAssertEqual(dialogue["attemptID"] as? String, receipt.attemptID.rawValue)
        XCTAssertNotEqual(dialogue["idempotencyKey"] as? String, intent["idempotencyKey"] as? String)
        XCTAssertEqual((dialogue["speaker"] as? [String: Any])?["id"] as? String, "npc-peig")

        let provisionalText = "A letter came, but I cannot say"
        let finalText = "The post came in this morning, and I sorted it myself."
        events += try await operation(runtime, [
            "op": "receive_frame", "attempt_id": receipt.attemptID.rawValue,
            "base_revision": try baseRevision(dialogue), "sequence": 1,
            "text": provisionalText, "stream_update": "replace", "done": false
        ]).events
        for event in events { presentation.apply(event) }
        let provisionalRow = try XCTUnwrap(presentation.state.transcript.last { $0.kind == .npcDialogue })
        XCTAssertEqual(provisionalRow.state, .provisional)
        XCTAssertEqual(provisionalRow.content, provisionalText)
        XCTAssertEqual(presentation.state.request(for: receipt.logicalRequestID)?.phase, .executing)

        let final = try await operation(runtime, [
            "op": "receive_candidate", "attempt_id": receipt.attemptID.rawValue,
            "base_revision": try baseRevision(dialogue), "dialogue": finalText,
            "metadata": [String: String](), "structured": true
        ]).events
        for event in final { presentation.apply(event) }

        let receiptEvent = try XCTUnwrap(events.first { $0.kind == .commandInterpreted })
        XCTAssertEqual(receiptEvent.content, "Speak with Peig Hannigan.")
        XCTAssertEqual(receiptEvent.metadata["role"], "intent")
        XCTAssertEqual(receiptEvent.attemptID, receipt.attemptID)
        let dialogueRows = presentation.state.transcript.filter { $0.kind == .npcDialogue }
        XCTAssertEqual(dialogueRows.count, 1, "The final must replace the provisional row, not add one")
        XCTAssertEqual(dialogueRows.first?.id, provisionalRow.id)
        XCTAssertEqual(dialogueRows.first?.state, .committed)
        XCTAssertEqual(dialogueRows.first?.content, finalText)
        let record = try XCTUnwrap(presentation.state.request(for: receipt.logicalRequestID))
        XCTAssertEqual(record.phase, .completed)
        XCTAssertEqual(record.terminalOutcome, .succeeded)
        XCTAssertEqual(record.attempts.count, 1)
        let settled = try await pendingInvocationIsNull(runtime)
        XCTAssertTrue(settled)
        try await runtime.close()
    }

    /// P2-F05, P2-F06, P2-F10: Stop, a late candidate, a transport failure,
    /// retry, and interruption by process loss keep one logical request with a
    /// new attempt each time, never commit dialogue, and replay identically.
    func testStopFailureRetryAndInterruptionKeepCorrelationWithoutCommitting() async throws {
        let (runtime, directory, payload) = try makeRuntime()
        defer { try? FileManager.default.removeItem(at: directory) }
        let initial = try FixtureJSON.decode(Snapshot.self, from: await runtime.snapshotJSON())

        let first = try await runtime.submit(text: "ask Peig about the wall")
        let firstInvocation = try await pendingInvocation(runtime)
        XCTAssertEqual(firstInvocation["role"] as? String, "npc_dialogue")
        _ = try await operation(runtime, [
            "op": "receive_frame", "attempt_id": first.attemptID.rawValue,
            "base_revision": try baseRevision(firstInvocation), "sequence": 1,
            "text": "A low stone", "stream_update": "replace", "done": false
        ])
        let stopped = try await runtime.stop()
        XCTAssertEqual(stopped.result, .cancelled)
        XCTAssertEqual(stopped.attemptID, first.attemptID)
        let late = try await operation(runtime, [
            "op": "receive_candidate", "attempt_id": first.attemptID.rawValue,
            "base_revision": try baseRevision(firstInvocation),
            "dialogue": "A late wall answer.", "metadata": [String: String](), "structured": true
        ])
        XCTAssertTrue(late.ignored, "A candidate after Stop must be ignored")
        XCTAssertTrue(late.events.isEmpty)

        let second = try await runtime.retry(logicalRequestID: first.logicalRequestID)
        XCTAssertEqual(second.logicalRequestID, first.logicalRequestID)
        XCTAssertNotEqual(second.attemptID, first.attemptID)
        let secondInvocation = try await pendingInvocation(runtime)
        XCTAssertEqual(secondInvocation["attemptID"] as? String, second.attemptID.rawValue)
        _ = try await runtime.receiveFailure(
            attemptID: second.attemptID,
            baseRevision: StateRevision(UInt64(try baseRevision(secondInvocation))),
            kind: .transport,
            message: "Endpoint unavailable"
        )

        let third = try await runtime.retry(logicalRequestID: first.logicalRequestID)
        XCTAssertNotEqual(third.attemptID, second.attemptID)
        let hasPending = try await pendingInvocationIsNull(runtime)
        XCTAssertFalse(hasPending)
        try await runtime.close()

        let resumed = try ParishRuntime.openResume(payload: payload)
        let resumedIdle = try await pendingInvocationIsNull(resumed)
        XCTAssertTrue(resumedIdle, "Relaunch must not replay the interrupted attempt")
        let page = try await resumed.readEventPage()
        let requestEvents = page.events.filter { $0.logicalRequestID == first.logicalRequestID }
        let terminals = requestEvents.filter { $0.kind == .responseCompleted }
        XCTAssertEqual(terminals.map(\.attemptID), [first.attemptID, second.attemptID, third.attemptID])
        XCTAssertEqual(terminals.map(\.terminalOutcome), [.cancelled, .failed, .interrupted])
        XCTAssertFalse(requestEvents.contains { $0.kind == .npcDialogue && !$0.provisional },
                       "No attempt may commit dialogue")
        XCTAssertEqual(Set(page.events.map(\.eventID)).count, page.events.count)

        let replay = PresentationSession(state: SessionState(sessionID: initial.sessionID))
        for event in page.events { replay.apply(event) }
        let record = try XCTUnwrap(replay.state.request(for: first.logicalRequestID))
        XCTAssertEqual(record.phase, .interrupted)
        XCTAssertEqual(record.currentAttemptID, third.attemptID)
        XCTAssertFalse(replay.state.transcript.contains { $0.kind == .npcDialogue && $0.state == .committed })
        XCTAssertEqual(replay.state.transcript.filter {
            $0.kind == .playerCommand && $0.logicalRequestID == first.logicalRequestID
        }.count, 1)
        try await resumed.close()
    }

    private struct OperationResult: Decodable {
        let events: [SemanticEvent]
        let ignored: Bool
    }

    private func operation(_ runtime: ParishRuntime, _ object: [String: Any]) async throws -> OperationResult {
        let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        return try FixtureJSON.decode(OperationResult.self, from: try await runtime.dispatchJSON(data))
    }

    private func pendingInvocation(_ runtime: ParishRuntime) async throws -> [String: Any] {
        let data = try await runtime.pendingEndpointJSON()
        return try XCTUnwrap(
            try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) as? [String: Any],
            "Expected a pending Endpoint invocation"
        )
    }

    private func pendingInvocationIsNull(_ runtime: ParishRuntime) async throws -> Bool {
        let data = try await runtime.pendingEndpointJSON()
        return try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) is NSNull
    }

    private func baseRevision(_ invocation: [String: Any]) throws -> Int {
        try XCTUnwrap((invocation["baseRevision"] as? [String: Any])?["rawValue"] as? Int)
    }

    private func makeRuntime() throws -> (ParishRuntime, URL, Data) {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("semantic-bridge-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let payload = try JSONSerialization.data(withJSONObject: [
            "save_path": directory.appendingPathComponent("game.sqlite").path
        ])
        return (try ParishRuntime.openResume(payload: payload), directory, payload)
    }
}
