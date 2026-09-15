import XCTest
import RundaleBridge
import RundaleKit
@testable import Rundale

/// Runs the packaged Rust FFI and SQLite journal inside the native test host.
/// Standalone bridge tests use C doubles, so they cannot establish this wiring.
@MainActor
final class RundalePhase4HistoryTests: XCTestCase {
    func testClarificationSurvivesBeyondTheRetainedEventTail() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("phase4-clarification-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let payload = try JSONSerialization.data(withJSONObject: [
            "save_path": directory.appendingPathComponent("phase2.sqlite").path
        ])
        let seed = try LimerickRuntime.openResume(payload: payload)
        _ = try await seed.submit(text: "/go Connolly Cottage", draftID: DraftID(), logicalRequestID: nil)
        let request = try await seed.submit(text: "ask Connolly about the household", draftID: DraftID(), logicalRequestID: nil)
        for _ in 0..<800 {
            _ = try await seed.submit(text: "/look", draftID: DraftID(), logicalRequestID: nil)
        }
        try await seed.close()
        let controller = RundaleEngineController(configuration: LaunchConfiguration(
            arguments: ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus",
                        "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
            environment: [:], bundle: [:]
        ))
        controller.start()
        try await waitUntil { !controller.state.transcript.isEmpty || controller.persistenceError != nil }
        XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")
        let clarification = try XCTUnwrap(controller.state.pendingClarification)
        XCTAssertEqual(clarification.requestID, request.logicalRequestID)
        XCTAssertTrue(clarification.prompt.choices.contains { $0.id == "choose-npc-roisin" })
        try await controller.answerClarification(choiceID: "choose-npc-roisin")
        XCTAssertNil(controller.state.pendingClarification)
        try await waitUntil {
            controller.state.request(for: request.logicalRequestID)?.phase == .completed
                || controller.persistenceError != nil
        }
        XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")
        XCTAssertEqual(controller.state.request(for: request.logicalRequestID)?.phase, .completed)
    }

    func testRelaunchRestoresFarBackAnchorFromDurableHistory() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("phase4-anchor-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let payload = try JSONSerialization.data(withJSONObject: [
            "save_path": directory.appendingPathComponent("phase2.sqlite").path
        ])
        let seed = try LimerickRuntime.openResume(payload: payload)
        for _ in 0..<400 {
            _ = try await seed.submit(text: "/look", draftID: DraftID(), logicalRequestID: nil)
        }
        try await seed.close()
        let configuration = LaunchConfiguration(
            arguments: ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus",
                        "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
            environment: [:], bundle: [:]
        )
        weak var previous: RundaleEngineController?
        var savedAnchor: TranscriptAnchor?
        do {
            let controller = RundaleEngineController(configuration: configuration)
            previous = controller
            controller.start()
            try await waitUntil { !controller.state.transcript.isEmpty || controller.persistenceError != nil }
            XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")
            controller.readHistory(anchor: TranscriptAnchor(
                itemID: try XCTUnwrap(controller.state.transcript.first?.id), offset: 0
            ))
            await controller.loadOlderTranscript()
            await controller.loadOlderTranscript()
            let anchor = TranscriptAnchor(itemID: try XCTUnwrap(controller.state.transcript.first?.id), offset: 17)
            savedAnchor = anchor
            controller.readHistory(anchor: anchor)
            await controller.persistLifecycleSnapshot()
        }
        try await waitUntil { previous == nil }
        let restored = RundaleEngineController(configuration: configuration)
        restored.start()
        try await waitUntil { !restored.state.transcript.isEmpty || restored.persistenceError != nil }
        XCTAssertNil(restored.persistenceError, restored.persistenceDiagnostic ?? "")
        let anchor = try XCTUnwrap(savedAnchor)
        XCTAssertEqual(restored.state.viewport.anchor, anchor)
        XCTAssertFalse(restored.state.viewport.isFollowingNewest)
        XCTAssertTrue(restored.state.transcript.contains { $0.id == anchor.itemID })
        XCTAssertLessThanOrEqual(restored.state.transcript.count, 500)
    }

    func testDurableHistoryPagesWithoutEvictingReadingAnchorAndReturnsToLiveTail() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("phase4-history-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let save = directory.appendingPathComponent("phase2.sqlite")
        let payload = try JSONSerialization.data(withJSONObject: ["save_path": save.path])
        let seed = try LimerickRuntime.openResume(payload: payload)
        for _ in 0..<400 {
            _ = try await seed.submit(text: "/look", draftID: DraftID(), logicalRequestID: nil)
        }
        try await seed.close()

        let controller = RundaleEngineController(configuration: LaunchConfiguration(
            arguments: ["--ui-tests", "--phase3", "--phase3-mock", "--no-auto-focus",
                        "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
            environment: [:], bundle: [:]
        ))
        controller.start()
        try await waitUntil { !controller.state.transcript.isEmpty || controller.persistenceError != nil }
        XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")
        XCTAssertLessThanOrEqual(controller.state.transcript.count, 500)
        let initialFirst = try XCTUnwrap(controller.state.transcript.first)
        let initialLast = try XCTUnwrap(controller.state.transcript.last)
        let anchor = TranscriptAnchor(itemID: initialFirst.id, offset: 12)
        controller.readHistory(anchor: anchor)

        await controller.loadOlderTranscript()
        let firstPageStart = try XCTUnwrap(controller.state.transcript.first?.lastEventSequence)
        XCTAssertLessThan(firstPageStart, initialFirst.lastEventSequence)
        XCTAssertLessThanOrEqual(controller.state.transcript.count, 500)
        XCTAssertEqual(controller.state.viewport.anchor, anchor)
        XCTAssertTrue(controller.state.transcript.contains { $0.id == initialFirst.id })
        XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")

        await controller.loadOlderTranscript()
        XCTAssertLessThan(try XCTUnwrap(controller.state.transcript.first?.lastEventSequence), firstPageStart)
        XCTAssertEqual(Set(controller.state.transcript.map(\.id)).count, controller.state.transcript.count)
        let readingIDs = controller.state.transcript.map(\.id)
        let model = RundalePresentationModel(
            launch: LaunchConfiguration(arguments: ["--ui-tests", "--phase3"], environment: [:], bundle: [:]),
            session: controller
        )
        model.start()
        let previousStreamRevision = model.streamRevision
        _ = try await controller.submit("/look")
        try await waitUntil { model.streamRevision > previousStreamRevision }
        XCTAssertEqual(controller.state.transcript.map(\.id), readingIDs,
                       "Live acceptance and completion must not displace the older window")
        XCTAssertEqual(controller.state.viewport.anchor, anchor)

        controller.followNewest()
        try await waitUntil {
            (controller.state.transcript.last?.lastEventSequence.rawValue ?? 0)
                > initialLast.lastEventSequence.rawValue
        }
        XCTAssertLessThanOrEqual(controller.state.transcript.count, 500)
        XCTAssertTrue(controller.state.viewport.isFollowingNewest)
        XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")
        XCTAssertGreaterThan(try XCTUnwrap(controller.state.transcript.first?.lastEventSequence), firstPageStart)
    }

    private func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0..<1_000 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Native history state did not reach the expected condition")
    }
}
