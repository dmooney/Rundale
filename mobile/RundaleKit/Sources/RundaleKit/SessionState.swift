import Foundation

public struct Draft: Codable, Equatable, Sendable {
    public let id: DraftID
    public var text: String

    public init(id: DraftID = DraftID(), text: String = "") {
        self.id = id
        self.text = text
    }

    public static var empty: Draft { Draft() }
}

public enum TranscriptItemState: String, Codable, Sendable {
    /// A command has crossed the acceptance boundary and is part of history.
    case accepted
    /// Output is provisional and belongs to an active execution attempt.
    case provisional
    /// Output was validated and committed by the session authority.
    case committed
    /// Output was stopped/interrupted before a gameplay commit.
    case interrupted
    /// Output belongs to an attempt that failed before a gameplay commit.
    case failed
    /// Output was explicitly canceled before a gameplay commit.
    case cancelled
}

public struct TranscriptItem: Codable, Equatable, Identifiable, Sendable {
    public let id: TranscriptItemID
    public let kind: SemanticEventKind
    public var content: String
    public var speaker: String?
    public let logicalRequestID: LogicalRequestID?
    public let attemptID: ExecutionAttemptID?
    public var state: TranscriptItemState
    public var gameTime: Date?
    public var lastEventSequence: EventSequence
    public var metadata: [String: String]

    public init(
        id: TranscriptItemID,
        kind: SemanticEventKind,
        content: String,
        speaker: String? = nil,
        logicalRequestID: LogicalRequestID? = nil,
        attemptID: ExecutionAttemptID? = nil,
        state: TranscriptItemState,
        gameTime: Date? = nil,
        lastEventSequence: EventSequence,
        metadata: [String: String] = [:]
    ) {
        self.id = id
        self.kind = kind
        self.content = content
        self.speaker = speaker
        self.logicalRequestID = logicalRequestID
        self.attemptID = attemptID
        self.state = state
        self.gameTime = gameTime
        self.lastEventSequence = lastEventSequence
        self.metadata = metadata
    }
}

public struct TranscriptAnchor: Codable, Equatable, Sendable {
    public let itemID: TranscriptItemID?
    public let offset: Double

    public init(itemID: TranscriptItemID? = nil, offset: Double = 0) {
        self.itemID = itemID
        self.offset = offset
    }
}

public struct TranscriptViewport: Codable, Equatable, Sendable {
    public var anchor: TranscriptAnchor?
    public var isFollowingNewest: Bool
    public private(set) var unreadCount: Int

    public init(
        anchor: TranscriptAnchor? = nil,
        isFollowingNewest: Bool = true,
        unreadCount: Int = 0
    ) {
        self.anchor = anchor
        self.isFollowingNewest = isFollowingNewest
        self.unreadCount = max(0, unreadCount)
    }

    public var hasNewText: Bool { unreadCount > 0 }

    public mutating func receivedNewText() {
        guard !isFollowingNewest else {
            unreadCount = 0
            anchor = TranscriptAnchor(itemID: nil, offset: 0)
            return
        }
        unreadCount = min(Int.max - 1, unreadCount + 1)
    }

    public mutating func followNewest() {
        isFollowingNewest = true
        unreadCount = 0
        anchor = TranscriptAnchor(itemID: nil, offset: 0)
    }

    public mutating func readHistory(at anchor: TranscriptAnchor? = nil) {
        isFollowingNewest = false
        self.anchor = anchor
    }
}

public struct PendingClarification: Codable, Equatable, Sendable {
    public let requestID: LogicalRequestID
    public let attemptID: ExecutionAttemptID
    public let prompt: ClarificationPrompt

    public init(requestID: LogicalRequestID, attemptID: ExecutionAttemptID, prompt: ClarificationPrompt) {
        self.requestID = requestID
        self.attemptID = attemptID
        self.prompt = prompt
    }
}

public struct SceneSummary: Codable, Equatable, Sendable {
    public let id: String
    public let name: String
    public let detail: String?

    public init(id: String, name: String, detail: String? = nil) {
        self.id = id
        self.name = name
        self.detail = detail
    }
}

public enum RequestPhase: String, Codable, Sendable {
    case accepted
    case interpreting
    case awaitingClarification = "awaiting_clarification"
    case executing
    case validating
    case completed
    case failed
    case cancelled
    case interrupted

    public var isTerminal: Bool {
        switch self {
        case .completed, .failed, .cancelled, .interrupted:
            return true
        case .accepted, .interpreting, .awaitingClarification, .executing, .validating:
            return false
        }
    }

    public var canRetry: Bool {
        switch self {
        case .failed, .cancelled, .interrupted:
            return true
        case .accepted, .interpreting, .awaitingClarification, .executing, .validating, .completed:
            return false
        }
    }
}

public struct RequestAttempt: Codable, Equatable, Sendable {
    public let id: ExecutionAttemptID
    public let originalText: String
    public var phase: RequestPhase
    public var terminalOutcome: ResponseTerminalOutcome?
    public var provisionalItemIDs: [TranscriptItemID]
    public var startedAt: EventSequence
    public var terminalEventID: SemanticEventID?
    public var committedStateRevision: StateRevision?

    public init(
        id: ExecutionAttemptID,
        originalText: String,
        phase: RequestPhase = .accepted,
        terminalOutcome: ResponseTerminalOutcome? = nil,
        provisionalItemIDs: [TranscriptItemID] = [],
        startedAt: EventSequence,
        terminalEventID: SemanticEventID? = nil,
        committedStateRevision: StateRevision? = nil
    ) {
        self.id = id
        self.originalText = originalText
        self.phase = phase
        self.terminalOutcome = terminalOutcome
        self.provisionalItemIDs = provisionalItemIDs
        self.startedAt = startedAt
        self.terminalEventID = terminalEventID
        self.committedStateRevision = committedStateRevision
    }
}

public struct RequestRecord: Codable, Equatable, Sendable {
    public let id: LogicalRequestID
    public let originalText: String
    public let acceptedCommandItemID: TranscriptItemID?
    public var attempts: [RequestAttempt]
    public var currentAttemptID: ExecutionAttemptID?
    public var phase: RequestPhase
    public var terminalOutcome: ResponseTerminalOutcome?
    public var committedStateRevision: StateRevision?

    public init(
        id: LogicalRequestID,
        originalText: String,
        acceptedCommandItemID: TranscriptItemID? = nil,
        attempts: [RequestAttempt] = [],
        currentAttemptID: ExecutionAttemptID? = nil,
        phase: RequestPhase = .accepted,
        terminalOutcome: ResponseTerminalOutcome? = nil,
        committedStateRevision: StateRevision? = nil
    ) {
        self.id = id
        self.originalText = originalText
        self.acceptedCommandItemID = acceptedCommandItemID
        self.attempts = attempts
        self.currentAttemptID = currentAttemptID
        self.phase = phase
        self.terminalOutcome = terminalOutcome
        self.committedStateRevision = committedStateRevision
    }

    public var currentAttempt: RequestAttempt? {
        guard let currentAttemptID else { return nil }
        return attempts.first { $0.id == currentAttemptID }
    }

    public var hasCommittedGameplay: Bool {
        // A successful terminal outcome is emitted only after the authority's
        // commit boundary. Treat it as settled even if an older or malformed
        // snapshot omitted the optional revision, so late callbacks cannot
        // reopen the request and apply the action twice.
        terminalOutcome == .succeeded
    }
}

public struct CommandHistoryEntry: Codable, Equatable, Hashable, Identifiable, Sendable {
    public let id: LogicalRequestID
    public let text: String
    public let commandItemID: TranscriptItemID?

    public init(id: LogicalRequestID, text: String, commandItemID: TranscriptItemID? = nil) {
        self.id = id
        self.text = text
        self.commandItemID = commandItemID
    }
}

public struct StreamProgress: Codable, Equatable, Hashable, Sendable {
    public let itemID: TranscriptItemID
    public let attemptID: ExecutionAttemptID?
    public let sequence: UInt64

    public init(itemID: TranscriptItemID, attemptID: ExecutionAttemptID?, sequence: UInt64) {
        self.itemID = itemID
        self.attemptID = attemptID
        self.sequence = sequence
    }
}

public struct SessionState: Codable, Equatable, Sendable {
    public let contractVersion: PresentationContractVersion
    public let sessionID: SessionID
    public private(set) var stateRevision: StateRevision
    public private(set) var eventCursor: EventCursor
    public private(set) var transcript: [TranscriptItem]
    public private(set) var hasOlderTranscript: Bool
    public private(set) var draft: Draft
    public private(set) var requests: [RequestRecord]
    public private(set) var commandHistory: [CommandHistoryEntry]
    public private(set) var pendingClarification: PendingClarification?
    public private(set) var scene: SceneSummary?
    public private(set) var viewport: TranscriptViewport
    public private(set) var activeRequestID: LogicalRequestID?
    public private(set) var lastError: String?
    public let transcriptCapacity: Int
    public let processedEventCapacity: Int
    public private(set) var processedEventIDs: [SemanticEventID]
    public private(set) var streamProgress: [StreamProgress]

    public init(
        sessionID: SessionID = SessionID(),
        contractVersion: PresentationContractVersion = .current,
        stateRevision: StateRevision = StateRevision(0),
        eventCursor: EventCursor = EventCursor(0),
        transcript: [TranscriptItem] = [],
        hasOlderTranscript: Bool = false,
        draft: Draft = .empty,
        requests: [RequestRecord] = [],
        commandHistory: [CommandHistoryEntry] = [],
        pendingClarification: PendingClarification? = nil,
        scene: SceneSummary? = nil,
        viewport: TranscriptViewport = TranscriptViewport(),
        activeRequestID: LogicalRequestID? = nil,
        lastError: String? = nil,
        transcriptCapacity: Int = 500,
        processedEventCapacity: Int = 1_024,
        processedEventIDs: [SemanticEventID] = [],
        streamProgress: [StreamProgress] = []
    ) {
        self.contractVersion = contractVersion
        self.sessionID = sessionID
        self.stateRevision = stateRevision
        self.eventCursor = eventCursor
        self.transcriptCapacity = max(1, transcriptCapacity)
        self.processedEventCapacity = max(1, processedEventCapacity)
        self.transcript = Array(transcript.suffix(max(1, transcriptCapacity)))
        self.hasOlderTranscript = hasOlderTranscript || transcript.count > max(1, transcriptCapacity)
        self.draft = draft
        self.requests = requests
        self.commandHistory = commandHistory
        self.pendingClarification = pendingClarification
        self.scene = scene
        self.viewport = viewport
        self.activeRequestID = activeRequestID
        self.lastError = lastError
        self.processedEventIDs = Array(processedEventIDs.suffix(max(1, processedEventCapacity)))
        self.streamProgress = streamProgress
    }

    public func request(for id: LogicalRequestID) -> RequestRecord? {
        requests.first { $0.id == id }
    }

    public var activeRequest: RequestRecord? {
        guard let activeRequestID else { return nil }
        return request(for: activeRequestID)
    }

    public var canSubmit: Bool { activeRequestID == nil }

    // These mutation helpers remain internal to the reducer. Keeping state
    // mutation in one place prevents a UI callback from becoming a second
    // authority for request or transcript transitions.
    mutating func updateDraft(_ draft: Draft) {
        self.draft = draft
    }

    mutating func updateViewport(_ viewport: TranscriptViewport) {
        self.viewport = viewport
    }

    mutating func setTranscript(_ transcript: [TranscriptItem], hasOlder: Bool) {
        self.transcript = Array(transcript.suffix(transcriptCapacity))
        self.hasOlderTranscript = hasOlder || transcript.count > transcriptCapacity
    }

    mutating func upsertTranscriptItem(_ item: TranscriptItem) {
        if let index = transcript.firstIndex(where: { $0.id == item.id }) {
            transcript[index] = item
        } else {
            transcript.append(item)
            if transcript.count > transcriptCapacity {
                transcript.removeFirst(transcript.count - transcriptCapacity)
                hasOlderTranscript = true
            }
        }
    }

    mutating func updateTranscriptItem(id: TranscriptItemID, _ update: (inout TranscriptItem) -> Void) {
        guard let index = transcript.firstIndex(where: { $0.id == id }) else { return }
        update(&transcript[index])
    }

    mutating func updateStateRevision(_ revision: StateRevision) {
        if revision > stateRevision { stateRevision = revision }
    }

    mutating func updateCursor(_ cursor: EventCursor) {
        if cursor > eventCursor { eventCursor = cursor }
    }

    mutating func recordProcessedEvent(_ eventID: SemanticEventID) {
        processedEventIDs.append(eventID)
        if processedEventIDs.count > processedEventCapacity {
            processedEventIDs.removeFirst(processedEventIDs.count - processedEventCapacity)
        }
    }

    mutating func trimProcessedEvents(to capacity: Int) {
        let safeCapacity = max(1, capacity)
        if processedEventIDs.count > safeCapacity {
            processedEventIDs.removeFirst(processedEventIDs.count - safeCapacity)
        }
    }

    mutating func replaceRequests(_ requests: [RequestRecord]) {
        self.requests = requests
        activeRequestID = requests.last(where: { !$0.phase.isTerminal })?.id
    }

    mutating func replaceRequest(_ record: RequestRecord) {
        if let index = requests.firstIndex(where: { $0.id == record.id }) {
            requests[index] = record
        } else {
            requests.append(record)
        }
        activeRequestID = requests.last(where: { !$0.phase.isTerminal })?.id
    }

    mutating func updatePendingClarification(_ value: PendingClarification?) {
        pendingClarification = value
    }

    mutating func updateScene(_ value: SceneSummary?) {
        scene = value
    }

    mutating func updateError(_ value: String?) {
        lastError = value
    }

    mutating func addHistory(_ entry: CommandHistoryEntry) {
        commandHistory.removeAll { $0.id == entry.id }
        commandHistory.append(entry)
    }

    mutating func updateStreamProgress(_ progress: StreamProgress) {
        streamProgress.removeAll { $0.itemID == progress.itemID }
        streamProgress.append(progress)
    }

    func latestStreamProgress(for itemID: TranscriptItemID) -> StreamProgress? {
        streamProgress.first { $0.itemID == itemID }
    }
}
