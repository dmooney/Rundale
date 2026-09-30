import XCTest
import RundaleBridge
import RundaleKit
@testable import Rundale

/// Runs the packaged Rust FFI and SQLite journal inside the native test host.
/// Standalone bridge tests use C doubles, so they cannot establish this wiring.
@MainActor
final class RundalePhase4HistoryTests: XCTestCase {
    /// The open payload for a save in `directory`, on the bundled world (or
    /// `world`).
    private func payload(_ directory: URL, world: URL? = nil) throws -> Data {
        let mod = try world ?? XCTUnwrap(Bundle.main.url(forResource: "rundale", withExtension: nil, subdirectory: "Mods"))
        return try JSONSerialization.data(withJSONObject: [
            "save_path": directory.appendingPathComponent("phase2.sqlite").path,
            "mod_dir": mod.path
        ])
    }

    /// Plays `count` turns the local parser resolves, so no Endpoint is
    /// involved.
    private func seedLooks(_ runtime: LimerickRuntime, count: Int) async throws {
        for _ in 0..<count {
            _ = try await runtime.submit(text: "look", draftID: DraftID(), logicalRequestID: nil)
        }
    }

    func testFirstLaunchCreatesMissingSaveDirectoryBeforeOpeningRuntime() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("phase4-first-launch-\(UUID().uuidString)/nested", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory.deletingLastPathComponent()) }
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.path))
        let controller = RundaleEngineController(configuration: LaunchConfiguration(
            arguments: ["--ui-tests", "--phase3", "--phase3-mock",
                        "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
            environment: [:], bundle: [:]
        ))
        controller.start()
        try await waitUntil { !controller.state.transcript.isEmpty || controller.persistenceError != nil }
        XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")
        XCTAssertTrue(FileManager.default.fileExists(atPath: directory.appendingPathComponent("phase2.sqlite").path))
    }

    /// A question the engine asked survives relaunch (it is journaled with
    /// its request, not only in the transcript tail), and answering it
    /// continues the same request. New input would cancel the question
    /// (portable-turn-api.md §5.1), so nothing is played in between.
    func testPendingClarificationSurvivesRelaunchAndContinuesTheRequest() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("phase4-clarification-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        // Róisín is a drover too in this copy of the world, so "Drover" is
        // ambiguous in the cottage.
        let world = directory.appendingPathComponent("rundale", isDirectory: true)
        try FileManager.default.copyItem(
            at: try XCTUnwrap(Bundle.main.url(forResource: "rundale", withExtension: nil, subdirectory: "Mods")), to: world
        )
        let npcs = world.appendingPathComponent("npcs.json")
        try String(contentsOf: npcs, encoding: .utf8)
            .replacingOccurrences(of: "Spinner and household bookkeeper", with: "Smallholder and cattle drover")
            .write(to: npcs, atomically: true, encoding: .utf8)

        let seed = try LimerickRuntime.openResume(payload: payload(directory, world: world))
        _ = try await seed.submit(text: "go to Connolly Cottage", draftID: DraftID(), logicalRequestID: nil)
        let request = try await seed.submit(text: "Drover, is it a good day for the fair?",
                                            draftID: DraftID(), logicalRequestID: nil)
        while let pending = try await seed.pendingInvocation(), pending.role == "intent" {
            _ = try await seed.resolve(
                pending, output: Data(#"{"intent":"talk","target":null,"dialogue":null,"atmosphere":null}"#.utf8)
            )
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
        let roisin = try XCTUnwrap(clarification.prompt.choices.first { $0.entityID == "3" })
        try await controller.answerClarification(choiceID: roisin.id)
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
        let seed = try LimerickRuntime.openResume(payload: payload(directory))
        try await seedLooks(seed, count: 400)
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
        let seed = try LimerickRuntime.openResume(payload: payload(directory))
        try await seedLooks(seed, count: 400)
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
        _ = try await controller.submit("look")
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
