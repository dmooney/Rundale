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
