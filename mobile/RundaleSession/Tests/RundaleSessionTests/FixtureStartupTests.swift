import Foundation
import RundaleKit
import XCTest
@testable import RundaleSession

@MainActor
final class FixtureStartupTests: XCTestCase {
    func testPagedOpeningHistoryPublishesCompleteInitialJournalOnce() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("RundaleFixtureBatch-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let draftURL = directory.appendingPathComponent("draft.json")
        let configuration = LaunchConfiguration(arguments: [
            "--ui-tests",
            "--fixture=paged-history",
            "--reset-fixture",
            "--draft-file=\(draftURL.path)"
        ])
        let controller = RundaleFixtureController(configuration: configuration)
        let initialSignal = expectation(description: "Consume the state stream's initial signal")
        let firstUpdate = expectation(description: "Observe the initial history publication")
        var firstPublishedCursor: UInt64?
        let observer = Task { @MainActor in
            var changes = controller.stateChanges().makeAsyncIterator()
            _ = await changes.next()
            initialSignal.fulfill()
            _ = await changes.next()
            firstPublishedCursor = controller.state.eventCursor.rawValue
            firstUpdate.fulfill()
        }
        await fulfillment(of: [initialSignal], timeout: 3)

        controller.start()

        await fulfillment(of: [firstUpdate], timeout: 3)
        XCTAssertEqual(firstPublishedCursor, 540)
        var saved: SessionSnapshot?
        for _ in 0..<100 {
            if let snapshot = try? controller.sessionStore.restoreSnapshot(),
               snapshot.transcriptHistory?.count == 540 {
                saved = snapshot
                break
            }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        let snapshot = try XCTUnwrap(saved)
        XCTAssertEqual(snapshot.transcriptHistory?.count, 540)
        XCTAssertEqual(snapshot.transcriptHistory?.last?.content, "Historical fixture entry 540.")
        observer.cancel()
    }

    func testIdleEventSubscriptionDoesNotRetainController() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("RundaleFixtureLifetime-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let draftURL = directory.appendingPathComponent("draft.json")
        let configuration = LaunchConfiguration(arguments: [
            "--ui-tests",
            "--fixture=standard",
            "--reset-fixture",
            "--draft-file=\(draftURL.path)"
        ])
        weak var weakController: RundaleFixtureController?

        do {
            var controller: RundaleFixtureController? = RundaleFixtureController(configuration: configuration)
            weakController = controller
            controller?.start()

            var didPersistOpening = false
            for _ in 0..<100 {
                if let controller,
                   let snapshot = try? controller.sessionStore.restoreSnapshot(),
                   snapshot.transcriptHistory?.count == 2 {
                    didPersistOpening = true
                    break
                }
                try await Task.sleep(nanoseconds: 10_000_000)
            }
            XCTAssertTrue(didPersistOpening)
            controller = nil
        }

        for _ in 0..<100 {
            if weakController == nil { break }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertNil(weakController)
    }

    func testPagedOpeningHistoryIsAppliedAndPersistedWithoutLoss() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("RundaleFixtureStartup-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let draftURL = directory.appendingPathComponent("draft.json")
        let configuration = LaunchConfiguration(arguments: [
            "--ui-tests",
            "--fixture=paged-history",
            "--reset-fixture",
            "--draft-file=\(draftURL.path)"
        ])
        let controller = RundaleFixtureController(configuration: configuration)
        controller.start()
        _ = try await controller.submit("still responsive")

        var saved: SessionSnapshot?
        for _ in 0..<500 {
            if let snapshot = try? controller.sessionStore.restoreSnapshot(),
               snapshot.transcriptHistory?.count == 541 {
                saved = snapshot
                break
            }
            try await Task.sleep(nanoseconds: 10_000_000)
        }

        let snapshot = try XCTUnwrap(saved)
        XCTAssertEqual(snapshot.state.eventCursor.rawValue, 541)
        XCTAssertTrue(snapshot.state.hasOlderTranscript)
        XCTAssertEqual(snapshot.transcriptHistory?.first?.content, "Historical fixture entry 1.")
        XCTAssertEqual(snapshot.transcriptHistory?.dropLast().last?.content, "Historical fixture entry 540.")
        XCTAssertEqual(snapshot.transcriptHistory?.last?.content, "still responsive")
        XCTAssertEqual(controller.state.transcript.last?.content, "still responsive")
        XCTAssertNil(controller.persistenceError)
    }
}
