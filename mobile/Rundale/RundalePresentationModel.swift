import Combine
import Foundation
import RundaleKit

struct PresentedTranscriptItem: Identifiable, Equatable, Sendable {
    let id: String
    let kind: SemanticEventKind
    let text: String
    let speaker: String?
    let state: TranscriptItemState
    let metadata: [String: String]

    var isProvisional: Bool { state == .provisional }
    var isInterrupted: Bool {
        state == .interrupted || state == .cancelled || state == .failed
    }
}

struct PresentedHeader: Equatable, Sendable {
    let location: String
    let timeOfDay: String
    let weather: String
}

struct PresentedCompletion: Identifiable, Equatable, Sendable {
    let id: String
    let label: String
    let insertion: String
    let detail: String?
}

struct PresentedClarification: Equatable, Sendable {
    struct Option: Identifiable, Equatable, Sendable {
        let id: String
        let label: String
        let detail: String?
    }

    let prompt: String
    let options: [Option]
}

@MainActor
final class RundalePresentationModel: ObservableObject {
    @Published private(set) var header: PresentedHeader
    @Published private(set) var transcript: [PresentedTranscriptItem]
    @Published private(set) var completions: [PresentedCompletion] = []
    @Published private(set) var clarification: PresentedClarification?
    @Published private(set) var isStreaming = false
    @Published private(set) var isFollowingNewest: Bool
    @Published private(set) var streamRevision = 0
    @Published private(set) var submissionMessage: String?
    @Published private(set) var accessibilityNotice: String?
    @Published private(set) var uiTestCheckpoint = ""
    /// UI-test-only JSON log of every transcript-row state published to the
    /// view. A provisional row can last well under an XCUITest poll interval,
    /// and virtualized rows leave the accessibility tree once scrolled away on
    /// a small screen, so tests read this record instead of racing the poll or
    /// depending on the viewport.
    @Published private(set) var uiTestTranscriptTrace = "[]"
    /// What the last bug report did: sending, sent, or waiting to be sent.
    @Published private(set) var bugReportNotice: String?
    /// UI-test-only: the last report sent and its screenshot's size.
    @Published private(set) var uiTestBugReport = ""
    @Published var draft: String

    /// The command that sends a bug report in beta builds.
    static let bugCommandWord = "/bug"
    static let bugReportSendingNotice = "Sending the bug report…"
    static let bugReportQueuedNotice = "No connection. The bug report will be sent when you're back online."
    static let bugReportLaterNotice = "The bug report couldn't be sent just now. It will be tried again later."
    static let bugReportRejectedNotice = "The bug report could not be sent."
    static let bugReportUnavailableNotice = "Bug reporting is not set up in this build."
    static let bugReportSentNotice = "Bug report sent. Thank you."
    static let earlierBugReportSentNotice = "An earlier bug report was sent."

    let launch: LaunchConfiguration
    private let session: any RundaleSessionControlling
    private let bugReports: BugReportOutbox?
    private let captureScreenshot: @MainActor () -> Data?
    private var isSendingBugReports = false
    /// The report the player just made, whose outcome the notice reports,
    /// until it is sent, rejected, or left queued.
    private var announcedBugReportID: String?
    /// limerick-bug-report's description limit, in Unicode scalars.
    static let bugDescriptionLimit = 2_000
    private var eventTask: Task<Void, Never>?
    private var draftRevision: UInt64 = 0
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

    var initialFollowsNewest: Bool { session.initialFollowsNewest }
    var initialUnreadCount: Int { session.initialUnreadCount }
    var initialTranscriptAnchor: TranscriptAnchor? { session.state.viewport.anchor }

    init(launch: LaunchConfiguration,
         session: (any RundaleSessionControlling)? = nil,
         bugReports: BugReportOutbox? = nil,
         captureScreenshot: @escaping @MainActor () -> Data? = ScreenCapture.keyWindowPNG) {
        self.launch = launch
        self.bugReports = bugReports ?? Self.defaultBugReports(launch)
        self.captureScreenshot = captureScreenshot
        if let session {
            self.session = session
        } else if launch.phase2 {
            self.session = RundaleEngineController(configuration: launch)
        } else {
            self.session = RundaleFixtureController(configuration: launch)
        }
        let state = self.session.state
        presentedCursor = state.eventCursor
        header = self.session.currentHeader
        transcript = state.transcript.map(Self.presentedItem)
        draft = launch.initialDraft ?? self.session.restoredDraft()?.text ?? ""
        clarification = state.pendingClarification.map(Self.presentedClarification)
        isStreaming = Self.isWorking(state)
        isFollowingNewest = state.viewport.isFollowingNewest
        lastAnnouncedEventID = self.session.lastEvent?.eventID
    }

    deinit {
        eventTask?.cancel()
    }

    func start() {
        guard eventTask == nil else { return }
        session.start()
        sendQueuedBugReports(announcing: nil)
        eventTask = Task { [weak self, session] in
            // The controller publishes state on the main actor after every
            // semantic event. Polling is intentionally absent: fixture tests
            // advance the adapter explicitly and normal launch schedules its
            // authored stream in the controller.
            for await _ in session.statePublisher.values {
                guard !Task.isCancelled else { return }
                guard let owner = self else { return }
                owner.refreshFromSession()
            }
        }
        refreshFromSession()
    }

    func stop() {
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
    func handleBackgrounding() {
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

    func handleForegrounding() {
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

    func advanceFixture() {
        guard launch.manualStream else { return }
        Task {
            _ = await session.step()
            refreshFromSession()
        }
    }

    func submitDraft() {
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
    static func bugDescription(in command: String) -> String? {
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
    func reportBug(description: String = "", clearingDraft sourceDraft: String? = nil) {
        guard launch.allowsBugReports else { return }
        guard let bugReports else {
            bugReportNotice = Self.bugReportUnavailableNotice
            return
        }
        let screenshot = captureScreenshot()
        let sourceRevision = draftRevision
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
                    device: ScreenCapture.deviceDescription,
                    screenshot: screenshot?.base64EncodedString()
                )
                try bugReports.enqueue(pending)
                if let sourceDraft, draftRevision == sourceRevision, draft == sourceDraft {
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

    private static func defaultBugReports(_ launch: LaunchConfiguration) -> BugReportOutbox? {
        guard launch.allowsBugReports else { return nil }
        let directory = BugReportOutbox.defaultDirectory(isUITesting: launch.isUITesting)
        if launch.isUITesting {
            try? FileManager.default.removeItem(at: directory)
            return BugReportOutbox(directory: directory, transport: RecordingBugReportTransport())
        }
        guard let url = launch.bugReportURL else { return nil }
        return BugReportOutbox(
            directory: directory,
            transport: HTTPBugReportTransport(
                baseURL: url,
                credentials: FirebaseEndpointCredentialAdapter(provider: FirebaseEndpointCredentialProvider())
            )
        )
    }

    func dismissBugReportNotice() {
        bugReportNotice = nil
    }

    var canRetry: Bool {
        session.state.requests.contains { $0.phase.canRetry }
    }

    func retryLastFailed() {
        guard !isStreaming, allowsSubmission else { return }
        let retryGeneration = lifecycleGeneration
        let retryRequestID = session.state.requests.last(where: { $0.phase.canRetry })?.id
        isStreaming = true
        if launch.isUITesting {
            uiTestCheckpoint = ""
        }
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
                if launch.isUITesting {
                    uiTestCheckpoint = "Retry accepted and persisted"
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

    func noteDraftMutation() {
        draftRevision &+= 1
        if isStreaming {
            activeSourceDraftID = nil
        }
        session.updateDraft(draft)
        if let error = session.persistDraft(draft) {
            submissionMessage = error
        }
    }

    func followNewest() {
        session.followNewest()
    }

    func readHistory(anchor: TranscriptAnchor? = nil) {
        session.readHistory(anchor: anchor)
    }

    func loadOlderTranscript() {
        guard !isLoadingOlderTranscript, session.state.hasOlderTranscript else { return }
        isLoadingOlderTranscript = true
        Task {
            await session.loadOlderTranscript()
            isLoadingOlderTranscript = false
            refreshFromSession()
        }
    }

    func persistDraft() {
        if session.state.draft.text != draft {
            session.updateDraft(draft)
        }
        if let error = session.persistDraft(draft) {
            submissionMessage = error
        }
    }

    func persistLifecycleSnapshot() async {
        await session.persistLifecycleSnapshot()
        refreshFromSession()
    }

    func recallCommand(_ command: String) {
        guard !command.isEmpty else { return }
        draft = command
        completionBrowser = nil
        completions = []
    }

    func refreshCompletions() {
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
    var offersCommands: Bool { !session.advertisedCommands.isEmpty }

    func browseCompletions(_ trigger: String) {
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

    func selectCompletion(_ completion: PresentedCompletion) {
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

    func selectClarification(_ option: PresentedClarification.Option) {
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
                PresentedClarification.Option(id: $0.id, label: $0.label, detail: nil)
            }
        )
    }
}

/// One published transcript-row state in the UI-test trace.
struct UITestTranscriptTraceEntry: Codable, Equatable {
    let row: String
    let kind: String
    let state: String
    let text: String
    let milliseconds: Int
}
