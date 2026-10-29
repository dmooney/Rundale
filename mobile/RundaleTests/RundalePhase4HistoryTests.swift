import SQLite3
import XCTest
import RundaleBridge
import RundaleKit
import RundaleSession
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

    func testNewGameClearsPresentationAndPreservesOldSave() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("new-game-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let configuration = LaunchConfiguration(
            arguments: ["--ui-tests", "--phase3", "--phase3-mock",
                        "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
            environment: [:], bundle: [:]
        )
        let controller = RundaleEngineController(configuration: configuration)
        controller.start()
        try await waitUntil { !controller.state.transcript.isEmpty || controller.persistenceError != nil }
        XCTAssertNil(controller.persistenceError)
        let originalID = controller.state.sessionID
        _ = try await controller.submit("/wait 60")
        let oldCursor = controller.state.eventCursor
        controller.updateDraft("old draft")
        controller.readHistory(anchor: controller.state.transcript.first.map {
            TranscriptAnchor(itemID: $0.id, offset: 10)
        })
        try await controller.newGame()
        let freshID = controller.state.sessionID
        XCTAssertNotEqual(freshID, originalID)
        XCTAssertTrue(controller.state.requests.isEmpty)
        XCTAssertTrue(controller.state.commandHistory.isEmpty)
        XCTAssertNil(controller.state.pendingClarification)
        XCTAssertEqual(controller.state.draft.text, "")
        XCTAssertTrue(controller.state.viewport.isFollowingNewest)
        XCTAssertGreaterThan(controller.state.eventCursor, oldCursor)
        XCTAssertEqual(controller.currentHeader.location, "Kilteevan Village")
        XCTAssertFalse(controller.state.transcript.contains { $0.content.contains("You wait for 60 minutes") })
        XCTAssertEqual(controller.state.transcript.filter { $0.kind == .sceneChanged }.count, 1)
        XCTAssertTrue(controller.advertisedCommands.contains { $0.label == "/new" })
        XCTAssertTrue(FileManager.default.fileExists(atPath: directory.appendingPathComponent("phase2.sqlite").path))
        // Yield to already-buffered event deliveries: they must not refill the new transcript.
        for _ in 0..<10 { await Task.yield() }
        XCTAssertTrue(controller.state.requests.isEmpty)
        XCTAssertEqual(controller.state.sessionID, freshID)
        XCTAssertFalse(controller.state.transcript.contains { $0.content.contains("You wait for 60 minutes") })
    }

    func testSelectedNewSaveDiscardsAnOlderPresentationProjection() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("new-game-projection-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let runtime = try LimerickRuntime.openResume(payload: payload(directory))
        struct Identity: Decodable { let sessionID: SessionID }
        struct Projection: Encodable {
            let draft: Draft
            let viewport: TranscriptViewport
            let sessionID: SessionID
        }
        let previous = try FixtureJSON.decode(Identity.self, from: await runtime.snapshotJSON())
        let oldProjection = Projection(draft: Draft(text: "a thought from the old game"),
                                       viewport: TranscriptViewport(), sessionID: previous.sessionID)
        let fresh = try FixtureJSON.decode(Identity.self, from: await runtime.newGame())
        try await runtime.close()
        let projectionURL = directory.appendingPathComponent("projection.json")
        try FixtureJSON.encode(oldProjection).write(to: projectionURL)
        let configuration = LaunchConfiguration(
            arguments: ["--ui-tests", "--phase3", "--phase3-mock", "--draft-file=\(projectionURL.path)"],
            environment: [:], bundle: [:]
        )
        let controller = RundaleEngineController(configuration: configuration)
        let model = RundalePresentationModel(launch: configuration, session: controller)
        model.start()
        try await waitUntil { controller.state.sessionID == fresh.sessionID || controller.persistenceError != nil }
        try await waitUntil { model.draft.isEmpty }
        XCTAssertNil(controller.persistenceError)
        XCTAssertEqual(controller.state.sessionID, fresh.sessionID)
        XCTAssertEqual(controller.state.draft.text, "")
        XCTAssertTrue(controller.state.viewport.isFollowingNewest)
    }

    func testBridgeNewGameSelectionSurvivesCloseAndResume() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("new-game-resume-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let options = try payload(directory)
        let runtime = try LimerickRuntime.openResume(payload: options)
        _ = try await runtime.submit(text: "/wait 60", draftID: DraftID(), logicalRequestID: nil)
        struct Snapshot: Decodable {
            let sessionID: SessionID
            let requests: [RequestRecord]
        }
        let fresh = try FixtureJSON.decode(Snapshot.self, from: await runtime.newGame())
        try await runtime.close()
        let resumed = try LimerickRuntime.openResume(payload: options)
        let snapshot = try FixtureJSON.decode(Snapshot.self, from: await resumed.snapshotJSON())
        XCTAssertEqual(snapshot.sessionID, fresh.sessionID)
        XCTAssertTrue(snapshot.requests.isEmpty)
        try await resumed.close()
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

    /// A launch after a new game was killed while it was being created
    /// opens a playable game on its opening scene (#2210).
    private func assertOpensOnTheOpeningScene(_ directory: URL) async throws {
        let controller = RundaleEngineController(configuration: LaunchConfiguration(
            arguments: ["--ui-tests", "--phase3", "--phase3-mock",
                        "--draft-file=\(directory.appendingPathComponent("projection.json").path)"],
            environment: [:], bundle: [:]
        ))
        controller.start()
        try await waitUntil { !controller.state.transcript.isEmpty || controller.persistenceError != nil }
        XCTAssertNil(controller.persistenceError, controller.persistenceDiagnostic ?? "")
        XCTAssertEqual(controller.state.transcript.map(\.content).filter {
            $0.hasPrefix("A muddy road runs between low stone walls")
        }.count, 1, "\(controller.state.transcript)")
    }

    private func temporarySaveDirectory(_ name: String) throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(name)-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    /// Killed after the save file was created but before anything was written
    /// to it, which earlier builds left unopenable.
    func testAnEmptySaveFileLeftByAnInterruptedNewGameOpensANewGame() async throws {
        let directory = try temporarySaveDirectory("phase4-empty-save")
        defer { try? FileManager.default.removeItem(at: directory) }
        XCTAssertTrue(FileManager.default.createFile(
            atPath: directory.appendingPathComponent("phase2.sqlite").path, contents: Data()
        ))
        try await assertOpensOnTheOpeningScene(directory)
    }

    /// Killed after the first snapshot but before the opening scene was
    /// journaled, which earlier builds resumed with an empty transcript.
    func testASaveWithoutItsOpeningSceneResumesWithIt() async throws {
        let directory = try temporarySaveDirectory("phase4-no-opening")
        defer { try? FileManager.default.removeItem(at: directory) }
        let seed = try LimerickRuntime.openResume(payload: payload(directory))
        try await seed.close()
        var database: OpaquePointer?
        XCTAssertEqual(sqlite3_open(directory.appendingPathComponent("phase2.sqlite").path, &database), SQLITE_OK)
        XCTAssertEqual(sqlite3_exec(database, "DELETE FROM transcript_events", nil, nil, nil), SQLITE_OK)
        XCTAssertEqual(sqlite3_close(database), SQLITE_OK)
        try await assertOpensOnTheOpeningScene(directory)
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
