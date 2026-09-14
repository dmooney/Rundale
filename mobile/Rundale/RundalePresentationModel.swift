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
    @Published private(set) var accessibilityNotice: String? = nil
    @Published private(set) var uiTestCheckpoint = ""
    @Published var draft: String

    let launch: LaunchConfiguration
    private let session: any RundaleSessionControlling
    private var eventTask: Task<Void, Never>?
    private var draftRevision: UInt64 = 0
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
         session: (any RundaleSessionControlling)? = nil) {
        self.launch = launch
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
        isStreaming = state.activeRequestID != nil
        isFollowingNewest = state.viewport.isFollowingNewest
        lastAnnouncedEventID = self.session.lastEvent?.eventID
    }

    deinit {
        eventTask?.cancel()
    }

    func start() {
        guard eventTask == nil else { return }
        session.start()
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
        guard !command.isEmpty, !isStreaming, allowsSubmission else { return }

        submissionMessage = nil
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
                detail: $0.entityID == nil ? nil : "Nearby person"
            )
        }
    }

    func browseCompletions(_ trigger: String) {
        completionBrowser = completionBrowser == trigger ? nil : trigger
        completions = completionBrowser.map { query in
            session.suggestions(for: query).map {
                PresentedCompletion(id: $0.id, label: $0.label,
                                    insertion: $0.insertionText,
                                    detail: $0.entityID == nil
                                        ? (draft.isEmpty ? nil : "Replace draft") : "Address this person")
            }
        } ?? []
    }

    func selectCompletion(_ completion: PresentedCompletion) {
        guard let source = session.suggestions(for: completionBrowser ?? draft)
            .first(where: { $0.id == completion.id }) else {
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
        isStreaming = state.activeRequestID != nil
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
