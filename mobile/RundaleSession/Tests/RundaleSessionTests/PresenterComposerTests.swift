import Foundation
import RundaleKit
import XCTest
@testable import RundaleSession

/// What the composer offers as a fixture request runs, is stopped and is
/// retried. The screen and any other host read these from the presenter.
@MainActor
final class PresenterComposerTests: XCTestCase {
    private var directory: URL!

    override func setUp() async throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("PresenterComposerTests-\(UUID().uuidString)")
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func presenter() -> RundaleSessionPresenter {
        let launch = LaunchConfiguration(
            arguments: ["--ui-tests", "--fixture=standard", "--reset-fixture",
                        "--draft-file=\(directory.appendingPathComponent("phase1-draft.json").path)"],
            environment: [:], bundle: [:]
        )
        return RundaleSessionPresenter(
            launch: launch, session: RundaleFixtureController(configuration: launch), bugReports: nil,
            captureScreenshot: { nil }, deviceDescription: "test"
        )
    }

    func testSendStopAndRetryFollowTheRequest() async {
        let presenter = presenter()
        presenter.start()
        await waitUntil { !presenter.transcript.isEmpty }
        XCTAssertFalse(presenter.canSubmitDraft, "an empty draft has nothing to send")
        presenter.draft = " \n "
        XCTAssertFalse(presenter.canSubmitDraft, "nor does one of whitespace")
        XCTAssertFalse(presenter.showsStop)
        XCTAssertFalse(presenter.showsRetry)

        presenter.draft = "long stream"
        presenter.noteDraftMutation()
        XCTAssertTrue(presenter.canSubmitDraft)
        presenter.submitDraft()
        await waitUntil { presenter.isStreaming && presenter.draft.isEmpty }
        XCTAssertTrue(presenter.showsStop)
        XCTAssertTrue(presenter.showsFixtureStep, "a hand-stepped fixture offers Next while it streams")
        XCTAssertFalse(presenter.showsRetry)

        // The fixture interprets the line, then streams Peig's reply.
        presenter.advanceFixture()
        await waitUntil { presenter.transcript.contains { $0.kind == .commandInterpreted } }
        presenter.advanceFixture()
        await waitUntil { presenter.transcript.contains { $0.isProvisional } }
        presenter.stop()
        await waitUntil { !presenter.isStreaming }
        XCTAssertFalse(presenter.showsStop)
        XCTAssertFalse(presenter.showsFixtureStep)
        XCTAssertTrue(presenter.showsRetry)
        XCTAssertTrue(presenter.transcript.contains { $0.unappliedReplyNote == "Not applied" },
                      "\(presenter.transcript)")

        presenter.retryLastFailed()
        await waitUntil { presenter.isStreaming }
        XCTAssertTrue(presenter.showsStop)
        XCTAssertFalse(presenter.showsRetry)
    }
}
