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

    /// `/new` is the host's command: it never reaches the session as a turn,
    /// waits for the current request to stop, and keeps the draft when the
    /// session cannot start a game (the fixture has only its one session).
    func testNewGameWaitsForTheRequestAndKeepsTheDraftWhenRefused() async {
        XCTAssertTrue(RundaleSessionPresenter.isNewGameCommand("  /NEW-GAME "))
        XCTAssertFalse(RundaleSessionPresenter.isNewGameCommand("/new please"))
        XCTAssertFalse(RundaleSessionPresenter.isNewGameCommand("/newer"))
        let presenter = presenter()
        presenter.start()
        await waitUntil { !presenter.transcript.isEmpty }

        presenter.draft = "long stream"
        presenter.submitDraft()
        await waitUntil { presenter.isStreaming }
        presenter.draft = "/new"
        presenter.submitDraft()
        XCTAssertEqual(presenter.submissionMessage, "Stop the current request before starting a new game.")
        XCTAssertEqual(presenter.draft, "/new")
        presenter.stop()
        await waitUntil { !presenter.isStreaming }

        let transcript = presenter.transcript
        presenter.submitDraft()
        await waitUntil {
            presenter.submissionMessage == "New games need the game engine; the fixture has one session."
        }
        XCTAssertEqual(presenter.draft, "/new")
        XCTAssertFalse(presenter.isStreaming)
        XCTAssertEqual(presenter.transcript, transcript, "no turn was submitted")
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

    /// #2329: a Retry tap with no retryable failure (the request recovered, or a
    /// newer one replaced it) is a no-op: no attempt starts and no error shows.
    func testRetryIsGoneAfterRecoveryAndAStaleTapDoesNothing() async {
        let presenter = presenter()
        presenter.start()
        await waitUntil { !presenter.transcript.isEmpty }

        presenter.draft = "long stream"
        presenter.noteDraftMutation()
        presenter.submitDraft()
        await waitUntil { presenter.isStreaming && presenter.draft.isEmpty }
        presenter.advanceFixture()
        await waitUntil { presenter.transcript.contains { $0.kind == .commandInterpreted } }
        presenter.advanceFixture()
        await waitUntil { presenter.transcript.contains { $0.isProvisional } }
        presenter.stop()
        await waitUntil { !presenter.isStreaming }
        XCTAssertTrue(presenter.showsRetry)

        // Retry, then let the fixture stream finish.
        presenter.retryLastFailed()
        await waitUntil { presenter.isStreaming }
        while presenter.isStreaming {
            presenter.advanceFixture()
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        XCTAssertFalse(presenter.showsRetry, "the recovered reply leaves nothing to retry")
        XCTAssertFalse(presenter.canRetry)

        let transcript = presenter.transcript
        presenter.retryLastFailed()
        try? await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertFalse(presenter.isStreaming, "a stale tap starts no attempt")
        XCTAssertNil(presenter.submissionMessage, "a stale tap reports no error")
        XCTAssertEqual(presenter.transcript, transcript)
    }
}
