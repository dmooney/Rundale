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
    @Published private(set) var streamRevision = 0
    @Published private(set) var submissionMessage: String?
    @Published private(set) var uiTestCheckpoint = ""
    @Published var draft: String

    let launch: LaunchConfiguration
    private let session: any RundaleSessionControlling
    private var eventTask: Task<Void, Never>?
    private var draftRevision: UInt64 = 0
    private var activeSourceDraftID: DraftID?

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
        header = self.session.currentHeader
        transcript = state.transcript.map(Self.presentedItem)
        draft = launch.initialDraft ?? self.session.restoredDraft()?.text ?? ""
        clarification = state.pendingClarification.map(Self.presentedClarification)
        isStreaming = state.activeRequestID != nil
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

    func advanceFixture() {
        guard launch.manualStream else { return }
        Task {
            _ = await session.step()
            refreshFromSession()
        }
    }

    func submitDraft() {
        let command = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !command.isEmpty, !isStreaming else { return }

        submissionMessage = nil
        session.updateDraft(draft)
        let sourceDraftID = session.state.draft.id
        activeSourceDraftID = sourceDraftID
        let sourceDraftRevision = draftRevision
        let sourceDraft = draft
        isStreaming = true

        Task {
            do {
                let receipt = try await session.submit(command)
                // The durable receipt also covers a batch whose acceptance
                // and completion arrive before the publisher is observed.
                // Keep a revision/text guard at that delivery boundary:
                // edits typed while submit is in flight belong to the next
                // draft and must not be erased.
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
        guard !isStreaming else { return }
        isStreaming = true
        if launch.isUITesting {
            uiTestCheckpoint = ""
        }
        Task {
            do {
                try await session.retryLastFailed()
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

    func persistDraft() {
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
        completions = []
    }

    func refreshCompletions() {
        completions = session.suggestions(for: draft).map {
            PresentedCompletion(
                id: $0.id,
                label: $0.label,
                insertion: $0.insertionText,
                detail: $0.entityID == nil ? nil : "Nearby person"
            )
        }
    }

    func selectCompletion(_ completion: PresentedCompletion) {
        guard let source = session.suggestions(for: draft).first(where: { $0.id == completion.id }) else {
            return
        }
        draft = session.insert(source, into: draft)
        completions = []
    }

    func selectClarification(_ option: PresentedClarification.Option) {
        Task {
            do {
                try await session.answerClarification(choiceID: option.id)
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
        updateTranscript(from: state.transcript)
        clarification = state.pendingClarification.map(Self.presentedClarification)
        isStreaming = state.activeRequestID != nil
        if let persistenceError = session.persistenceError {
            submissionMessage = persistenceError
        }

        // SessionReducer performs the authoritative acceptance check using
        // sourceDraftID and command text. Mirror that accepted event locally
        // only when it still refers to the current draft.
        if let event = session.lastEvent,
           event.kind == .playerCommand,
           event.accepted,
           event.sourceDraftID == activeSourceDraftID,
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
