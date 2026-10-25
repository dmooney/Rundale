import Foundation
import LimerickEndpointKit
import RundaleKit

public struct PresentedTranscriptItem: Identifiable, Equatable, Sendable {
    public let id: String
    public let kind: SemanticEventKind
    public let text: String
    public let speaker: String?
    public let state: TranscriptItemState
    public let metadata: [String: String]

    public init(id: String, kind: SemanticEventKind, text: String, speaker: String?,
                state: TranscriptItemState, metadata: [String: String]) {
        self.id = id
        self.kind = kind
        self.text = text
        self.speaker = speaker
        self.state = state
        self.metadata = metadata
    }

    public var isProvisional: Bool { state == .provisional }
    public var isInterrupted: Bool {
        state == .interrupted || state == .cancelled || state == .failed
    }

    /// Shown under a reply the game did not apply (stopped, interrupted or
    /// failed), so it is not mistaken for a reply the game applied.
    public var unappliedReplyNote: String? {
        kind == .npcDialogue && isInterrupted ? "Not applied" : nil
    }
}

public struct PresentedHeader: Equatable, Sendable {
    public let location: String
    public let timeOfDay: String
    public let weather: String

    public init(location: String, timeOfDay: String, weather: String) {
        self.location = location
        self.timeOfDay = timeOfDay
        self.weather = weather
    }
}

public struct PresentedCompletion: Identifiable, Equatable, Sendable {
    public let id: String
    public let label: String
    public let insertion: String
    public let detail: String?

    public init(id: String, label: String, insertion: String, detail: String?) {
        self.id = id
        self.label = label
        self.insertion = insertion
        self.detail = detail
    }
}

public struct PresentedClarification: Equatable, Sendable {
    public struct Option: Identifiable, Equatable, Sendable {
        public let id: String
        public let label: String
        public let detail: String?
        /// What VoiceOver reads instead of the label, when the engine gives one.
        public var accessibilityLabel: String?

        public init(id: String, label: String, detail: String?, accessibilityLabel: String? = nil) {
            self.id = id
            self.label = label
            self.detail = detail
            self.accessibilityLabel = accessibilityLabel
        }
    }

    public let prompt: String
    public let options: [Option]

    public init(prompt: String, options: [Option]) {
        self.prompt = prompt
        self.options = options
    }
}

/// What the player sees and can do, projected from a session: the header,
/// transcript, composer draft and its controls, completions and pending
/// clarification. The app's `RundalePresentationModel` publishes it to
/// SwiftUI; every rule lives here, so any other host shows the same state.
@MainActor
public final class RundaleSessionPresenter {
    /// Called before each published value changes, as `@Published` calls
    /// `objectWillChange`.
    public var willChange: (@MainActor () -> Void)?

    public private(set) var header: PresentedHeader {
        willSet { willChange?() }
    }

    public private(set) var transcript: [PresentedTranscriptItem] {
        willSet { willChange?() }
    }

    public private(set) var completions: [PresentedCompletion] = [] {
        willSet { willChange?() }
    }

    public private(set) var clarification: PresentedClarification? {
        willSet { willChange?() }
    }

    public private(set) var isStreaming = false {
        willSet { willChange?() }
    }

    public private(set) var isFollowingNewest: Bool {
        willSet { willChange?() }
    }

    public private(set) var streamRevision = 0 {
        willSet { willChange?() }
    }

    public private(set) var submissionMessage: String? {
        willSet { willChange?() }
    }

    public private(set) var accessibilityNotice: String? {
        willSet { willChange?() }
    }

    /// UI-test-only JSON log of every transcript-row state published to the
    /// view. A provisional row can last well under an XCUITest poll interval,
    /// and virtualized rows leave the accessibility tree once scrolled away on
    /// a small screen, so tests read this record instead of racing the poll or
    /// depending on the viewport.
    public private(set) var uiTestTranscriptTrace = "[]" {
        willSet { willChange?() }
    }

    /// What the last bug report did: sending, sent, or waiting to be sent.
    public private(set) var bugReportNotice: String? {
        willSet { willChange?() }
    }

    /// UI-test-only: the last report sent and its screenshot's size.
    public private(set) var uiTestBugReport = "" {
        willSet { willChange?() }
    }

    public var draft: String {
        willSet { willChange?() }
    }

    /// The command that sends a bug report in beta builds.
    public static let bugCommandWord = "/bug"
    public static let bugReportSendingNotice = "Sending the bug report…"
    public static let bugReportQueuedNotice = "No connection. The bug report will be sent when you're back online."
    public static let bugReportLaterNotice = "The bug report couldn't be sent just now. It will be tried again later."
    public static let bugReportRejectedNotice = "The bug report could not be sent."
    public static let bugReportUnavailableNotice = "Bug reporting is not set up in this build."
    public static let bugReportSentNotice = "Bug report sent. Thank you."
    public static let earlierBugReportSentNotice = "An earlier bug report was sent."

    public let launch: LaunchConfiguration
    private let session: any RundaleSessionControlling
    private let bugReports: BugReportOutbox?
    private let captureScreenshot: @MainActor () -> Data?
    private let deviceDescription: String
    private var isSendingBugReports = false
    /// The report the player just made, whose outcome the notice reports,
    /// until it is sent, rejected, or left queued.
    private var announcedBugReportID: String?
    /// limerick-bug-report's description limit, in Unicode scalars.
    public static let bugDescriptionLimit = 2_000
    private var eventTask: Task<Void, Never>?
    private var draftRevision: UInt64 = 0
    /// The last terminal event whose returned line refilled the composer.
    private var lastComposerReturnEventID: SemanticEventID?
    private var transcriptTraceEntries: [UITestTranscriptTraceEntry] = []
    private var transcriptTraceLastState: [String: UITestTranscriptTraceEntry] = [:]
    private let transcriptTraceOrigin = Date()
    private var completionBrowser: String?
    private var activeSourceDraftID: DraftID?
    private var lifecycleGeneration: UInt64 = 0
    private var allowsSubmission = true
    private var isLoadingOlderTranscript = false
    private var lastAnnouncedEventID: SemanticEventID?
    private var lifecycleTask: Task<Void, Never>?
    private var presentedCursor = EventCursor(0)

    public var initialFollowsNewest: Bool { session.initialFollowsNewest }
    public var initialUnreadCount: Int { session.initialUnreadCount }
    public var initialTranscriptAnchor: TranscriptAnchor? { session.state.viewport.anchor }

    /// - Parameters:
    ///   - bugReports: where `/bug` and a shake queue reports; `nil` when
    ///     this build cannot send them.
    ///   - captureScreenshot: a PNG of what the player sees, for a report.
    ///   - deviceDescription: the device and OS a report names.
    public init(launch: LaunchConfiguration,
                session: any RundaleSessionControlling,
                bugReports: BugReportOutbox?,
                captureScreenshot: @escaping @MainActor () -> Data?,
                deviceDescription: String) {
        self.launch = launch
        self.bugReports = bugReports
        self.captureScreenshot = captureScreenshot
        self.deviceDescription = deviceDescription
        self.session = session
        let state = self.session.state
        presentedCursor = state.eventCursor
        header = self.session.currentHeader
        transcript = state.transcript.map(Self.presentedItem)
        draft = launch.initialDraft ?? self.session.restoredDraft()?.text ?? ""
        clarification = state.pendingClarification.map(Self.presentedClarification)
        isStreaming = Self.isWorking(state)
        isFollowingNewest = state.viewport.isFollowingNewest
        lastAnnouncedEventID = self.session.lastEvent?.eventID
        lastComposerReturnEventID = self.session.lastEvent?.eventID
    }

    deinit {
        eventTask?.cancel()
    }

    public func start() {
        guard eventTask == nil else { return }
        session.start()
        sendQueuedBugReports(announcing: nil)
        eventTask = Task { [weak self, session] in
            // The controller signals on the main actor after every state
            // change. Polling is intentionally absent: fixture tests advance
            // the adapter explicitly and normal launch schedules its
            // authored stream in the controller.
            for await _ in session.stateChanges() {
                guard !Task.isCancelled else { return }
                guard let owner = self else { return }
                owner.refreshFromSession()
            }
        }
        refreshFromSession()
    }

    public func stop() {
        Task {
            do {
                _ = try await session.stop()
                refreshFromSession()
            } catch {
                // Keep the current authoritative state visible while
                // surfacing the failure. A failed stop must not masquerade as
                // an idle request and invite an unsafe retry.
                submissionMessage = error.localizedDescription
                refreshFromSession()
            }
        }
    }

    /// iOS may suspend an app without allowing an in-flight streaming task to
    /// finish. Close the active attempt through the session's serialized Stop
    /// boundary before taking the lifecycle snapshot so a resumed app sees a
    /// retryable interrupted request rather than a request that is still
    /// implicitly in flight.
    public func handleBackgrounding() {
        allowsSubmission = false
        lifecycleGeneration &+= 1
        session.setInferenceAllowed(false)
        let backgroundRequestID = session.state.activeRequestID
        let priorLifecycleTask = lifecycleTask
        lifecycleTask = Task { [weak self] in
            await priorLifecycleTask?.value
            guard let self else { return }
            if backgroundRequestID != nil,
               session.state.activeRequestID == backgroundRequestID {
                do {
                    _ = try await session.stop()
                } catch {
                    submissionMessage = error.localizedDescription
                }
            }
            await session.persistLifecycleSnapshot()
            refreshFromSession()
        }
    }

    public func handleForegrounding() {
        lifecycleGeneration &+= 1
        let foregroundGeneration = lifecycleGeneration
        let pendingLifecycleTask = lifecycleTask
        lifecycleTask = Task { [weak self] in
            await pendingLifecycleTask?.value
            guard let self, lifecycleGeneration == foregroundGeneration else { return }
            allowsSubmission = true
            session.setInferenceAllowed(true)
            refreshFromSession()
            sendQueuedBugReports(announcing: nil)
            lifecycleTask = nil
        }
    }

    public func advanceFixture() {
        guard launch.manualStream else { return }
        Task {
            _ = await session.step()
            refreshFromSession()
        }
    }

    public func submitDraft() {
        let command = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        // `/bug` is the app's own command: it reports, even while a reply
        // streams, and never reaches the engine as a turn.
        if launch.allowsBugReports, let description = Self.bugDescription(in: command) {
            reportBug(description: description, clearingDraft: draft)
            return
        }
        guard !command.isEmpty, !isStreaming, allowsSubmission else { return }

        submissionMessage = nil
        bugReportNotice = nil
        let sourceDraftID = session.state.draft.id
        activeSourceDraftID = sourceDraftID
        let sourceDraftRevision = draftRevision
        let sourceDraft = draft
        let submissionGeneration = lifecycleGeneration
        isStreaming = true

        Task {
            do {
                // A scene transition can happen after the button callback but
                // before the actor call starts. Do not begin new inference
                // while the app is inactive; the draft remains durable for a
                // later foreground submission.
                guard allowsSubmission, lifecycleGeneration == submissionGeneration else {
                    isStreaming = false
                    return
                }
                let receipt = try await session.submit(command)
                if receipt.accepted {
                    // A newly accepted command is the player's return to the
                    // live conversation. Rejoin the newest tail even when the
                    // player had been reading older history; rejected or
                    // failed submissions leave that deliberate viewport alone.
                    session.followNewest()
                }
                // Acceptance is durable even if the scene transition races
                // the receipt. Clear only the exact draft that crossed the
                // boundary; newer typing remains the next command.
                if receipt.accepted,
                   draftRevision == sourceDraftRevision,
                   draft == sourceDraft {
                    draft = ""
                    draftRevision &+= 1
                    if let error = session.persistDraft("") {
                        submissionMessage = error
                    }
                    activeSourceDraftID = nil
                }
                if lifecycleGeneration != submissionGeneration {
                    // The request crossed acceptance while the app was being
                    // suspended. Resolve it through the same serialized stop
                    // boundary only if it is still the request we accepted;
                    // a foreground submission must never be stopped by this
                    // older task.
                    if session.state.activeRequestID == receipt.logicalRequestID {
                        _ = try? await session.stop()
                    }
                    refreshFromSession()
                    return
                }
                refreshFromSession()
            } catch {
                submissionMessage = error.localizedDescription
                isStreaming = false
                refreshFromSession()
            }
        }
    }

    /// The description after `/bug`, or `nil` when `command` is not `/bug`.
    public static func bugDescription(in command: String) -> String? {
        let trimmed = command.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.lowercased().hasPrefix(bugCommandWord) else { return nil }
        let rest = trimmed.dropFirst(bugCommandWord.count)
        guard rest.first.map(\.isWhitespace) ?? true else { return nil }
        return rest.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Sends a bug report to `limerick-bug-report` (#2022), whose private
    /// inbox the `/bug-triage` skill turns into GitHub issues where needed:
    /// from `/bug`, or from shaking the phone. The screenshot is
    /// taken first, so it shows what the player saw. A typed `/bug` draft is
    /// cleared once the report is queued, unless the player has changed it
    /// since; a shake leaves the draft alone. A report that cannot be sent
    /// now waits on disk and is sent at the next launch or foreground.
    public func reportBug(description: String = "", clearingDraft sourceDraft: String? = nil) {
        guard launch.allowsBugReports else { return }
        guard let bugReports else {
            bugReportNotice = Self.bugReportUnavailableNotice
            return
        }
        let screenshot = captureScreenshot()
        bugReportNotice = Self.bugReportSendingNotice
        Task {
            do {
                // The service refuses a longer description; a refused report
                // would be dropped, so keep what fits.
                let description = String(String.UnicodeScalarView(
                    description.unicodeScalars.prefix(Self.bugDescriptionLimit)
                ))
                let pending = PendingBugReport(
                    reportID: UUID().uuidString,
                    description: description,
                    report: try await session.bugReport(description: description),
                    build: launch.buildDescription,
                    device: deviceDescription,
                    screenshot: screenshot?.base64EncodedString()
                )
                try bugReports.enqueue(pending)
                if let sourceDraft,
                   draft.trimmingCharacters(in: .whitespacesAndNewlines)
                   == sourceDraft.trimmingCharacters(in: .whitespacesAndNewlines) {
                    draft = ""
                    draftRevision &+= 1
                    completionBrowser = nil
                    completions = []
                    session.updateDraft("")
                    if let error = session.persistDraft("") {
                        submissionMessage = error
                    }
                }
                sendQueuedBugReports(announcing: pending.reportID)
            } catch {
                bugReportNotice = "The bug report could not be made. \(error.localizedDescription)"
            }
        }
    }

    /// Sends every queued report, oldest first, stopping at the first that
    /// must wait. `announcing` names the report the player just made, whose
    /// outcome the notice reports; a send already under way picks it up.
    private func sendQueuedBugReports(announcing reportID: String?) {
        if let reportID { announcedBugReportID = reportID }
        guard let bugReports, !isSendingBugReports else { return }
        isSendingBugReports = true
        Task {
            defer { isSendingBugReports = false }
            var attempted = Set<String>()
            while let next = bugReports.pending.first(where: { !attempted.contains($0.reportID) }) {
                attempted.insert(next.reportID)
                // Read after each send returns: the player may make a new
                // report, and so change which one is announced, meanwhile.
                var isAnnounced: Bool { next.reportID == announcedBugReportID }
                do {
                    try await bugReports.send(next)
                    if isAnnounced {
                        announcedBugReportID = nil
                        announceBugReport(Self.bugReportSentNotice)
                    } else if announcedBugReportID == nil {
                        announceBugReport(Self.earlierBugReportSentNotice)
                    }
                    if launch.isUITesting {
                        uiTestBugReport = Self.uiTestSummary(next)
                    }
                } catch BugReportSendError.rejected {
                    if isAnnounced {
                        announcedBugReportID = nil
                        announceBugReport(Self.bugReportRejectedNotice)
                    }
                } catch {
                    if announcedBugReportID != nil {
                        announcedBugReportID = nil
                        let offline = (error as? BugReportSendError) == .offline
                        announceBugReport(offline ? Self.bugReportQueuedNotice : Self.bugReportLaterNotice)
                    }
                    return
                }
            }
        }
    }

    private func announceBugReport(_ notice: String) {
        bugReportNotice = notice
        accessibilityNotice = notice
    }

    private static func uiTestSummary(_ report: PendingBugReport) -> String {
        let bytes = report.screenshot.flatMap { Data(base64Encoded: $0) }?.count ?? 0
        return "screenshot bytes: \(bytes)\n\(report.report)"
    }

    /// The outbox a launch sends reports through: in memory for UI tests,
    /// `limerick-bug-report` with `credentials` otherwise, and none when the
    /// build offers no reports or names no service.
    public static func defaultBugReports(
        _ launch: LaunchConfiguration,
        credentials: () -> any LimerickEndpointKit.EndpointCredentialProvider
    ) -> BugReportOutbox? {
        guard launch.allowsBugReports else { return nil }
        let directory = BugReportOutbox.defaultDirectory(isUITesting: launch.isUITesting)
        if launch.isUITesting {
            try? FileManager.default.removeItem(at: directory)
            return BugReportOutbox(directory: directory, transport: RecordingBugReportTransport())
        }
        guard let url = launch.bugReportURL else { return nil }
        return BugReportOutbox(
            directory: directory,
            transport: HTTPBugReportTransport(baseURL: url, credentials: credentials())
        )
    }

    public func dismissBugReportNotice() {
        bugReportNotice = nil
    }

    public var canRetry: Bool {
        session.state.requests.contains { $0.phase.canRetry }
    }

    /// The draft has something to send.
    public var canSubmitDraft: Bool {
        !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The composer offers Stop in place of Send while the engine works.
    public var showsStop: Bool { isStreaming }

    /// Retry is offered for a failed or interrupted request, once nothing is
    /// in flight.
    public var showsRetry: Bool { canRetry && !isStreaming }

    /// UI tests that step a fixture stream by hand get a Next control while
    /// it streams.
    public var showsFixtureStep: Bool { launch.isUITesting && launch.manualStream && isStreaming }

    public func retryLastFailed() {
        guard !isStreaming, allowsSubmission else { return }
        let retryGeneration = lifecycleGeneration
        let retryRequestID = session.state.requests.last(where: { $0.phase.canRetry })?.id
        isStreaming = true
        Task {
            do {
                guard allowsSubmission, lifecycleGeneration == retryGeneration else {
                    isStreaming = false
                    return
                }
                try await session.retryLastFailed()
                if lifecycleGeneration != retryGeneration {
                    if let retryRequestID,
                       session.state.activeRequestID == retryRequestID {
                        _ = try? await session.stop()
                    }
                    refreshFromSession()
                    return
                }
                submissionMessage = nil
                refreshFromSession()
            } catch {
                submissionMessage = error.localizedDescription
                isStreaming = false
                refreshFromSession()
            }
        }
    }

    public func noteDraftMutation() {
        draftRevision &+= 1
        if isStreaming {
            activeSourceDraftID = nil
        }
        session.updateDraft(draft)
        if let error = session.persistDraft(draft) {
            submissionMessage = error
        }
    }

    public func followNewest() {
        session.followNewest()
    }

    public func readHistory(anchor: TranscriptAnchor? = nil) {
        session.readHistory(anchor: anchor)
    }

    public func loadOlderTranscript() {
        guard !isLoadingOlderTranscript, session.state.hasOlderTranscript else { return }
        isLoadingOlderTranscript = true
        Task {
            await session.loadOlderTranscript()
            isLoadingOlderTranscript = false
            refreshFromSession()
        }
    }

    public func persistDraft() {
        if session.state.draft.text != draft {
            session.updateDraft(draft)
        }
        if let error = session.persistDraft(draft) {
            submissionMessage = error
        }
    }

    public func persistLifecycleSnapshot() async {
        await session.persistLifecycleSnapshot()
        refreshFromSession()
    }

    public func recallCommand(_ command: String) {
        guard !command.isEmpty else { return }
        draft = command
        completionBrowser = nil
        completions = []
    }

    public func refreshCompletions() {
        completionBrowser = nil
        completions = session.suggestions(for: draft).map {
            PresentedCompletion(
                id: $0.id,
                label: $0.label,
                insertion: $0.insertionText,
                detail: $0.detail ?? ($0.entityID == nil ? nil : "Nearby person")
            )
        }
    }

    /// Whether the session advertises any slash commands for the Commands
    /// button.
    public var offersCommands: Bool { !session.advertisedCommands.isEmpty }

    public func browseCompletions(_ trigger: String) {
        completionBrowser = completionBrowser == trigger ? nil : trigger
        completions = completionBrowser.map { query in
            browsed(query).map {
                PresentedCompletion(id: $0.id, label: $0.label,
                                    insertion: $0.insertionText,
                                    detail: $0.entityID == nil
                                        ? (draft.isEmpty ? nil : "Replace draft") : "Address this person")
            }
        } ?? []
    }

    public func selectCompletion(_ completion: PresentedCompletion) {
        let sources = completionBrowser.map(browsed) ?? session.suggestions(for: draft)
        guard let source = sources.first(where: { $0.id == completion.id }) else {
            return
        }
        if let completionBrowser {
            draft = completionBrowser == "@"
                ? source.insertionText + " " + draft
                : source.insertionText
        } else {
            draft = session.insert(source, into: draft)
        }
        completionBrowser = nil
        completions = []
    }

    /// What a shortcut button offers: the advertised commands for `/`
    /// (typing `/` offers every command), nearby people for `@`.
    private func browsed(_ trigger: String) -> [CompletionItem] {
        trigger == "/" ? session.advertisedCommands : session.suggestions(for: trigger)
    }

    public func selectClarification(_ option: PresentedClarification.Option) {
        guard allowsSubmission else { return }
        let clarificationGeneration = lifecycleGeneration
        let clarificationRequestID = session.state.pendingClarification?.requestID
        Task {
            do {
                guard allowsSubmission, lifecycleGeneration == clarificationGeneration else { return }
                try await session.answerClarification(choiceID: option.id)
                if lifecycleGeneration != clarificationGeneration {
                    if let clarificationRequestID,
                       session.state.activeRequestID == clarificationRequestID {
                        _ = try? await session.stop()
                    }
                    refreshFromSession()
                    return
                }
                refreshFromSession()
            } catch {
                submissionMessage = error.localizedDescription
            }
        }
    }

    /// The engine is working on the active request. A request waiting on the
    /// player's clarification choice stays active in the engine but is not
    /// work: the question is the player's turn, and new input replaces it.
    private static func isWorking(_ state: SessionState) -> Bool {
        guard let active = state.activeRequestID else { return false }
        return state.pendingClarification?.requestID != active
    }

    private func refreshFromSession() {
        let state = session.state
        let nextHeader = session.currentHeader
        if nextHeader != header { header = nextHeader }
        let previousRevision = streamRevision
        updateTranscript(from: state.transcript)
        if state.eventCursor > presentedCursor,
           !state.viewport.isFollowingNewest,
           streamRevision == previousRevision {
            // A paged history window intentionally omits new tail rows. Its
            // arrival must still expose the control that returns to newest.
            streamRevision &+= 1
        }
        presentedCursor = state.eventCursor
        clarification = state.pendingClarification.map(Self.presentedClarification)
        isStreaming = Self.isWorking(state)
        if state.viewport.isFollowingNewest != isFollowingNewest {
            isFollowingNewest = state.viewport.isFollowingNewest
        }
        if let persistenceError = session.persistenceError {
            submissionMessage = persistenceError
        }
        publishAccessibilityNotice(for: session.lastEvent)

        // SessionReducer performs the authoritative acceptance check using
        // sourceDraftID and command text. Mirror that accepted event locally
        // only when it still refers to the current draft.
        if let event = session.lastEvent,
           event.kind == .playerCommand,
           event.accepted,
           event.sourceDraftID == activeSourceDraftID,
           session.state.draft.id == activeSourceDraftID,
           let content = event.content,
           draft == content {
            draft = ""
            draftRevision &+= 1
            if let error = session.persistDraft("") {
                submissionMessage = error
            }
            activeSourceDraftID = nil
        }

        // A request that sent nothing because the person chosen had left
        // hands its line back (design §3.4). It refills an empty composer
        // only, so it never overwrites what the player has started typing.
        if let event = session.lastEvent,
           event.eventID != lastComposerReturnEventID,
           event.kind == .responseCompleted,
           let returned = event.metadata["composerText"] {
            lastComposerReturnEventID = event.eventID
            if draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                draft = returned
                noteDraftMutation()
            }
        }
    }

    private func publishAccessibilityNotice(for event: SemanticEvent?) {
        guard let event, event.eventID != lastAnnouncedEventID else { return }
        lastAnnouncedEventID = event.eventID
        switch event.kind {
        case .responseCompleted:
            switch event.terminalOutcome {
            case .succeeded:
                accessibilityNotice = "Response complete."
            case .failed:
                accessibilityNotice = "Response failed. Retry is available."
            case .interrupted, .cancelled:
                accessibilityNotice = "Response interrupted. Retry is available."
            case nil:
                accessibilityNotice = "Response complete."
            }
        case .clarificationRequired:
            if let question = session.state.pendingClarification?.prompt.question {
                accessibilityNotice = "Clarification needed. \(question)"
            } else {
                accessibilityNotice = "Clarification needed."
            }
        case .error:
            accessibilityNotice = event.content ?? "The response encountered an error."
        default:
            break
        }
    }

    private func updateTranscript(from incoming: [TranscriptItem]) {
        publishTranscript(from: incoming)
        if launch.isUITesting { recordTranscriptTrace() }
    }

    private func recordTranscriptTrace() {
        var changed = false
        let elapsed = Int(Date().timeIntervalSince(transcriptTraceOrigin) * 1000)
        for item in transcript {
            let entry = UITestTranscriptTraceEntry(
                row: item.id,
                kind: item.kind.rawValue,
                state: item.state.rawValue,
                text: item.text,
                milliseconds: elapsed
            )
            if let previous = transcriptTraceLastState[item.id],
               previous.state == entry.state, previous.text == entry.text {
                continue
            }
            transcriptTraceLastState[item.id] = entry
            transcriptTraceEntries.append(entry)
            changed = true
        }
        guard changed else { return }
        if transcriptTraceEntries.count > 400 {
            transcriptTraceEntries.removeFirst(transcriptTraceEntries.count - 400)
        }
        if let data = try? JSONEncoder().encode(transcriptTraceEntries) {
            uiTestTranscriptTrace = String(bytes: data, encoding: .utf8) ?? ""
        }
    }

    private func publishTranscript(from incoming: [TranscriptItem]) {
        guard !incoming.isEmpty else {
            if !transcript.isEmpty {
                transcript = []
                streamRevision &+= 1
            }
            return
        }

        if transcript.isEmpty {
            transcript = incoming.map(Self.presentedItem)
            streamRevision &+= 1
            return
        }

        if incoming.count >= transcript.count,
           incoming.prefix(transcript.count).enumerated().allSatisfy({ index, item in
               item.id.rawValue == transcript[index].id
           }) {
            // Finalization can replace provisional text or mark an earlier
            // row interrupted while also appending a terminal row. Stable
            // identity alone does not mean the preceding rows are unchanged.
            var next = transcript
            for (index, item) in incoming.enumerated() {
                let presented = Self.presentedItem(item)
                if index == next.count {
                    next.append(presented)
                } else if next[index] != presented {
                    next[index] = presented
                }
            }
            if next != transcript {
                transcript = next
                streamRevision &+= 1
            }
            return
        }

        let rebuilt = incoming.map(Self.presentedItem)
        if rebuilt != transcript {
            transcript = rebuilt
            streamRevision &+= 1
        }
    }

    private static func presentedItem(_ item: TranscriptItem) -> PresentedTranscriptItem {
        PresentedTranscriptItem(
            id: item.id.rawValue,
            kind: item.kind,
            text: item.content,
            speaker: item.speaker,
            state: item.state,
            metadata: item.metadata
        )
    }

    private static func presentedClarification(_ value: PendingClarification) -> PresentedClarification {
        PresentedClarification(
            prompt: value.prompt.question,
            options: value.prompt.choices.map {
                PresentedClarification.Option(
                    id: $0.id, label: $0.label, detail: nil, accessibilityLabel: $0.accessibilityLabel
                )
            }
        )
    }
}

/// One published transcript-row state in the UI-test trace.
public struct UITestTranscriptTraceEntry: Codable, Equatable {
    public let row: String
    public let kind: String
    public let state: String
    public let text: String
    public let milliseconds: Int

    public init(row: String, kind: String, state: String, text: String, milliseconds: Int) {
        self.row = row
        self.kind = kind
        self.state = state
        self.text = text
        self.milliseconds = milliseconds
    }
}
