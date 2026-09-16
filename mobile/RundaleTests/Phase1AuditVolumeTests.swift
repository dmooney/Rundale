import XCTest
import UIKit
import RundaleKit
@testable import Rundale

@MainActor
final class Phase1AuditVolumeTests: XCTestCase {
    func testFixtureHistoryPagesPastFiveHundredRowsAcrossRelaunchAndReturnsToLiveTail() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("phase1-cap-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let draftPath = folder.appendingPathComponent("draft.json").path
        let configuration = LaunchConfiguration(arguments: ["--ui-tests", "--fixture=paged-history", "--reset-fixture",
            "--draft-file=\(draftPath)"], environment: [:], bundle: [:])
        let controller = RundaleFixtureController(configuration: configuration)
        controller.start()
        _ = try await controller.submit("/look")
        while await controller.adapter.isStreaming { _ = await controller.step() }
        let earliest = TranscriptItem(
            id: TranscriptItemID("opening:history-0"), kind: .narration,
            content: "Historical fixture entry 1.", state: .committed,
            lastEventSequence: EventSequence(1)
        )
        XCTAssertEqual(controller.state.transcript.count, 500)
        XCTAssertFalse(controller.state.transcript.contains { $0.id == earliest.id })
        await controller.loadOlderTranscript()
        XCTAssertTrue(controller.state.isHistoricalWindow)
        controller.readHistory(anchor: TranscriptAnchor(itemID: controller.state.transcript.first?.id, offset: 12))
        await controller.persistLifecycleSnapshot()

        let restoredConfiguration = LaunchConfiguration(arguments: ["--ui-tests", "--fixture=paged-history",
            "--draft-file=\(draftPath)"], environment: [:], bundle: [:])
        let restored = RundaleFixtureController(configuration: restoredConfiguration)
        restored.start()
        XCTAssertEqual(restored.state.transcript.count, 500)
        XCTAssertTrue(restored.state.isHistoricalWindow)

        for _ in 0..<20 where restored.state.hasOlderTranscript &&
            !restored.state.transcript.contains(where: { $0.id == earliest.id }) {
            await restored.loadOlderTranscript()
        }
        XCTAssertEqual(restored.state.transcript.first(where: { $0.id == earliest.id }), earliest)
        XCTAssertLessThanOrEqual(restored.state.transcript.count, 500)

        let anchor = TranscriptAnchor(itemID: earliest.id, offset: 12)
        restored.readHistory(anchor: anchor)
        let stopped = try await restored.submit("long stream")
        _ = await restored.step()
        _ = await restored.step()
        _ = await restored.stop()
        XCTAssertEqual(restored.state.viewport.anchor, anchor)
        XCTAssertFalse(restored.state.viewport.isFollowingNewest)
        XCTAssertTrue(restored.state.viewport.hasNewText)

        let completed = try await restored.submit("ask Peig about the old church")
        while await restored.adapter.isStreaming { _ = await restored.step() }
        restored.followNewest()
        XCTAssertTrue(restored.state.viewport.isFollowingNewest)
        XCTAssertEqual(restored.state.viewport.unreadCount, 0)
        XCTAssertTrue(restored.state.transcript.contains {
            $0.id == TranscriptItemID("\(stopped.attemptID.rawValue):command") && $0.content == "long stream"
        })
        XCTAssertEqual(
            restored.state.transcript.first(where: { $0.id == TranscriptItemID("\(completed.attemptID.rawValue):response") })?.content,
            "The old church? It stands beyond the alder trees, where the path bends toward the hill."
        )
    }

    func testHistoricalPageRelaunchInterruptsActiveStreamWithoutLosingItsArchiveTail() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("phase1-active-history-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let draftPath = folder.appendingPathComponent("draft.json").path
        let initial = LaunchConfiguration(arguments: ["--ui-tests", "--fixture=paged-history", "--reset-fixture",
            "--draft-file=\(draftPath)"], environment: [:], bundle: [:])
        let controller = RundaleFixtureController(configuration: initial)
        controller.start()
        let receipt = try await controller.submit("long stream")
        _ = await controller.step()
        _ = await controller.step()
        await controller.loadOlderTranscript()
        let anchor = TranscriptAnchor(itemID: controller.state.transcript.first?.id, offset: 9)
        controller.readHistory(anchor: anchor)
        await controller.persistLifecycleSnapshot()

        let restored = RundaleFixtureController(configuration: LaunchConfiguration(
            arguments: ["--ui-tests", "--fixture=paged-history", "--draft-file=\(draftPath)"],
            environment: [:], bundle: [:]
        ))
        restored.start()
        await restored.persistLifecycleSnapshot()
        XCTAssertEqual(restored.state.viewport.anchor, anchor)
        XCTAssertFalse(restored.state.viewport.isFollowingNewest)

        restored.followNewest()
        let partialID = TranscriptItemID("\(receipt.attemptID.rawValue):response")
        XCTAssertEqual(restored.state.transcript.first(where: { $0.id == partialID })?.content,
                       "The first part arrives.")
        XCTAssertEqual(restored.state.transcript.first(where: { $0.id == partialID })?.state, .interrupted)
    }

    func testLegacyWindowOnlySaveKeepsItsRetainedRowsWhenPagingAndFollowingNewest() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("phase1-legacy-history-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let draftPath = folder.appendingPathComponent("draft.json").path
        let sessionID = SessionID("legacy-history")
        let retained = (1...2).map { index in
            TranscriptItem(id: TranscriptItemID("legacy-\(index)"), kind: .narration,
                           content: "Retained legacy row \(index)", state: .committed,
                           lastEventSequence: EventSequence(UInt64(index)))
        }
        let state = SessionState(sessionID: sessionID, eventCursor: EventCursor(2), transcript: retained,
                                 hasOlderTranscript: true, transcriptCapacity: 500)
        let store = FixtureSessionStore(fileURL: folder.appendingPathComponent("phase1-session.json"))
        try store.save(state)

        let controller = RundaleFixtureController(configuration: LaunchConfiguration(
            arguments: ["--ui-tests", "--fixture=paged-history", "--draft-file=\(draftPath)"],
            environment: [:], bundle: [:]
        ))
        controller.start()
        await controller.loadOlderTranscript()
        XCTAssertEqual(controller.state.transcript, retained)
        XCTAssertFalse(controller.state.hasOlderTranscript)
        controller.followNewest()
        XCTAssertEqual(controller.state.transcript, retained)
    }

    func testThousandRowsRenderAndReachBothEndsWithinBudget() async {
        let controller = TranscriptCollectionViewController()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 375, height: 667))
        window.rootViewController = controller
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let rows = (0..<1000).map { index in
            PresentedTranscriptItem(id: "audit-row-\(index)", kind: .npcDialogue,
                text: "Authored row \(index). " + String(repeating: "The rain falls on the road. ", count: index % 4 + 1),
                speaker: "Peig", state: .committed, metadata: [:])
        }
        let start = Date()
        controller.update(items: rows, followsNewest: true, initialFollowsNewest: true, initialAnchor: nil)
        try? await Task.sleep(for: .milliseconds(500))
        controller.view.layoutIfNeeded()
        let scroll = try! XCTUnwrap(controller.view.subviews.compactMap { $0 as? UICollectionView }.first)
        XCTAssertEqual(scroll.numberOfItems(inSection: 0), 1000)
        XCTAssertTrue(scroll.indexPathsForVisibleItems.contains(IndexPath(item: 999, section: 0)))
        scroll.scrollToItem(at: IndexPath(item: 0, section: 0), at: .top, animated: false)
        controller.view.layoutIfNeeded()
        XCTAssertTrue(scroll.indexPathsForVisibleItems.contains(IndexPath(item: 0, section: 0)))
        scroll.scrollToItem(at: IndexPath(item: 999, section: 0), at: .bottom, animated: false)
        controller.view.layoutIfNeeded()
        XCTAssertTrue(scroll.indexPathsForVisibleItems.contains(IndexPath(item: 999, section: 0)))
        XCTAssertLessThan(Date().timeIntervalSince(start), 3,
                          "Audit budget for isolated 1000-row layout and two nonanimated endpoint jumps")
    }
}
