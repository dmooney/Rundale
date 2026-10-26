import Combine
import Foundation
import RundaleKit
import RundaleSession

/// Publishes `RundaleSessionPresenter` to SwiftUI. The presenter holds every
/// rule; this wrapper only forwards its values and actions, and sends
/// `objectWillChange` before each change the presenter makes.
@MainActor
final class RundalePresentationModel: ObservableObject {
    static let bugCommandWord = RundaleSessionPresenter.bugCommandWord
    static let bugReportSendingNotice = RundaleSessionPresenter.bugReportSendingNotice
    static let bugReportQueuedNotice = RundaleSessionPresenter.bugReportQueuedNotice
    static let bugReportLaterNotice = RundaleSessionPresenter.bugReportLaterNotice
    static let bugReportRejectedNotice = RundaleSessionPresenter.bugReportRejectedNotice
    static let bugReportUnavailableNotice = RundaleSessionPresenter.bugReportUnavailableNotice
    static let bugReportSentNotice = RundaleSessionPresenter.bugReportSentNotice
    static let earlierBugReportSentNotice = RundaleSessionPresenter.earlierBugReportSentNotice
    static let bugDescriptionLimit = RundaleSessionPresenter.bugDescriptionLimit

    static func bugDescription(in command: String) -> String? {
        RundaleSessionPresenter.bugDescription(in: command)
    }

    let presenter: RundaleSessionPresenter

    init(launch: LaunchConfiguration,
         session: (any RundaleSessionControlling)? = nil,
         bugReports: BugReportOutbox? = nil,
         captureScreenshot: @escaping @MainActor () -> Data? = ScreenCapture.keyWindowPNG) {
        let bugReports = bugReports ?? RundaleSessionPresenter.defaultBugReports(launch) {
            FirebaseEndpointCredentialAdapter(provider: FirebaseEndpointCredentialProvider())
        }
        let session: any RundaleSessionControlling = session
            ?? (launch.phase2
                ? RundaleEngineController(configuration: launch)
                : RundaleFixtureController(configuration: launch))
        presenter = RundaleSessionPresenter(
            launch: launch,
            session: session,
            bugReports: bugReports,
            captureScreenshot: captureScreenshot,
            deviceDescription: ScreenCapture.deviceDescription
        )
        presenter.willChange = { [weak self] in self?.objectWillChange.send() }
    }

    var launch: LaunchConfiguration { presenter.launch }
    var header: PresentedHeader { presenter.header }
    var transcript: [PresentedTranscriptItem] { presenter.transcript }
    var completions: [PresentedCompletion] { presenter.completions }
    var clarification: PresentedClarification? { presenter.clarification }
    var isStreaming: Bool { presenter.isStreaming }
    var isFollowingNewest: Bool { presenter.isFollowingNewest }
    var streamRevision: Int { presenter.streamRevision }
    var submissionMessage: String? { presenter.submissionMessage }
    var accessibilityNotice: String? { presenter.accessibilityNotice }
    var uiTestTranscriptTrace: String { presenter.uiTestTranscriptTrace }
    var bugReportNotice: String? { presenter.bugReportNotice }
    var uiTestBugReport: String { presenter.uiTestBugReport }
    var draft: String {
        get { presenter.draft }
        set { presenter.draft = newValue }
    }

    var initialFollowsNewest: Bool { presenter.initialFollowsNewest }
    var initialUnreadCount: Int { presenter.initialUnreadCount }
    var initialTranscriptAnchor: TranscriptAnchor? { presenter.initialTranscriptAnchor }
    var canRetry: Bool { presenter.canRetry }
    var canSubmitDraft: Bool { presenter.canSubmitDraft }
    var showsStop: Bool { presenter.showsStop }
    var showsRetry: Bool { presenter.showsRetry }
    var showsFixtureStep: Bool { presenter.showsFixtureStep }
    var offersCommands: Bool { presenter.offersCommands }

    func start() { presenter.start() }
    func stop() { presenter.stop() }
    func handleBackgrounding() { presenter.handleBackgrounding() }
    func handleForegrounding() { presenter.handleForegrounding() }
    func advanceFixture() { presenter.advanceFixture() }
    func submitDraft() { presenter.submitDraft() }

    func reportBug(description: String = "", clearingDraft sourceDraft: String? = nil) {
        presenter.reportBug(description: description, clearingDraft: sourceDraft)
    }

    func dismissBugReportNotice() { presenter.dismissBugReportNotice() }
    func retryLastFailed() { presenter.retryLastFailed() }
    func noteDraftMutation() { presenter.noteDraftMutation() }
    func followNewest() { presenter.followNewest() }
    func readHistory(anchor: TranscriptAnchor? = nil) { presenter.readHistory(anchor: anchor) }
    func loadOlderTranscript() { presenter.loadOlderTranscript() }
    func persistDraft() { presenter.persistDraft() }
    func persistLifecycleSnapshot() async { await presenter.persistLifecycleSnapshot() }
    func recallCommand(_ command: String) { presenter.recallCommand(command) }
    func refreshCompletions() { presenter.refreshCompletions() }
    func browseCompletions(_ trigger: String) { presenter.browseCompletions(trigger) }
    func selectCompletion(_ completion: PresentedCompletion) { presenter.selectCompletion(completion) }
    func selectClarification(_ option: PresentedClarification.Option) { presenter.selectClarification(option) }
}
