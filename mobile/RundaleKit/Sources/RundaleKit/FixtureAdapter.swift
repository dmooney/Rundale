import Foundation

public enum FixtureAdapterError: Error, Equatable, Sendable, LocalizedError {
    case emptyCommand
    case requestInProgress
    case requestAlreadyCommitted
    case requestNotRetryable
    case requestNotFound
    case noClarificationPending
    case invalidClarificationChoice
    case eventBufferOverflow

    public var errorDescription: String? {
        switch self {
        case .emptyCommand: return "Enter a command before sending."
        case .requestInProgress: return "A response is already in progress."
        case .requestAlreadyCommitted: return "That request has already completed."
        case .requestNotRetryable: return "Only an interrupted or failed request can be retried."
        case .requestNotFound: return "The request could not be found."
        case .noClarificationPending: return "No clarification is waiting for a choice."
        case .invalidClarificationChoice: return "That clarification choice is no longer available."
        case .eventBufferOverflow: return "The event stream fell behind; the session was refreshed."
        }
    }
}

public struct SubmissionReceipt: Codable, Equatable, Sendable {
    public let logicalRequestID: LogicalRequestID
    public let attemptID: ExecutionAttemptID
    public let commandEventID: SemanticEventID?
    public let accepted: Bool
    public let isRetry: Bool
    public let cursor: EventCursor

    public init(
        logicalRequestID: LogicalRequestID,
        attemptID: ExecutionAttemptID,
        commandEventID: SemanticEventID? = nil,
        accepted: Bool = true,
        isRetry: Bool = false,
        cursor: EventCursor
    ) {
        self.logicalRequestID = logicalRequestID
        self.attemptID = attemptID
        self.commandEventID = commandEventID
        self.accepted = accepted
        self.isRetry = isRetry
        self.cursor = cursor
    }
}

public struct StopReceipt: Codable, Equatable, Sendable {
    public enum Result: String, Codable, Sendable {
        case cancelled
        case noActiveRequest = "no_active_request"
        case alreadyCommitted = "already_committed"
    }

    public let result: Result
    public let logicalRequestID: LogicalRequestID?
    public let attemptID: ExecutionAttemptID?
    public let eventID: SemanticEventID?

    public init(result: Result, logicalRequestID: LogicalRequestID? = nil, attemptID: ExecutionAttemptID? = nil, eventID: SemanticEventID? = nil) {
        self.result = result
        self.logicalRequestID = logicalRequestID
        self.attemptID = attemptID
        self.eventID = eventID
    }
}

public struct FixtureStepResult: Equatable, Sendable {
    public let event: SemanticEvent?
    public let isFinished: Bool
    public let isAwaitingClarification: Bool

    public init(event: SemanticEvent?, isFinished: Bool = false, isAwaitingClarification: Bool = false) {
        self.event = event
        self.isFinished = isFinished
        self.isAwaitingClarification = isAwaitingClarification
    }
}

/// One deterministic step in a fixture stream. Reusing `itemKey` across
/// provisional steps updates one transcript item instead of creating a new
/// item per chunk.
public struct FixtureStep: Codable, Equatable, Sendable {
    public let kind: SemanticEventKind
    public let content: String?
    public let speaker: String?
    public let provisional: Bool
    public let streamSequence: UInt64?
    public let streamUpdate: StreamUpdate
    public let terminalOutcome: ResponseTerminalOutcome?
    public let clarification: ClarificationPrompt?
    public let metadata: [String: String]
    public let itemKey: String?

    public init(
        kind: SemanticEventKind,
        content: String? = nil,
        speaker: String? = nil,
        provisional: Bool = false,
        streamSequence: UInt64? = nil,
        streamUpdate: StreamUpdate = .replace,
        terminalOutcome: ResponseTerminalOutcome? = nil,
        clarification: ClarificationPrompt? = nil,
        metadata: [String: String] = [:],
        itemKey: String? = nil
    ) {
        self.kind = kind
        self.content = content
        self.speaker = speaker
        self.provisional = provisional
        self.streamSequence = streamSequence
        self.streamUpdate = streamUpdate
        self.terminalOutcome = terminalOutcome
        self.clarification = clarification
        self.metadata = metadata
        self.itemKey = itemKey
    }

    public static func narrative(_ text: String, itemKey: String? = nil) -> Self {
        Self(kind: .narration, content: text, itemKey: itemKey)
    }

    public static func npcChunk(
        _ text: String,
        speaker: String,
        itemKey: String = "response",
        sequence: UInt64? = nil,
        update: StreamUpdate = .replace
    ) -> Self {
        Self(
            kind: .npcDialogue,
            content: text,
            speaker: speaker,
            provisional: true,
            streamSequence: sequence,
            streamUpdate: update,
            itemKey: itemKey
        )
    }

    public static func completed(_ outcome: ResponseTerminalOutcome = .succeeded) -> Self {
        Self(kind: .responseCompleted, terminalOutcome: outcome)
    }
}

public struct FixtureCommandPlan: Codable, Equatable, Sendable {
    public let matchingCommand: String
    public let steps: [FixtureStep]
    public let clarificationContinuation: [FixtureStep]

    public init(
        matchingCommand: String,
        steps: [FixtureStep],
        clarificationContinuation: [FixtureStep] = []
    ) {
        self.matchingCommand = matchingCommand
        self.steps = steps
        self.clarificationContinuation = clarificationContinuation
    }
}

public struct FixtureScript: Codable, Equatable, Sendable {
    public let initialSteps: [FixtureStep]
    public let commandPlans: [FixtureCommandPlan]
    public let defaultPlan: FixtureCommandPlan

    public init(
        initialSteps: [FixtureStep] = [],
        commandPlans: [FixtureCommandPlan] = [],
        defaultPlan: FixtureCommandPlan = FixtureCommandPlan(
            matchingCommand: "*",
            steps: [
                FixtureStep(kind: .commandInterpreted, content: "I’ll consider that.", itemKey: "interpretation"),
                FixtureStep(kind: .narration, content: "The fixture has no further detail.", itemKey: "result"),
                .completed()
            ]
        )
    ) {
        self.initialSteps = initialSteps
        self.commandPlans = commandPlans
        self.defaultPlan = defaultPlan
    }

    public func plan(for command: String) -> FixtureCommandPlan {
        let normalized = command.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return commandPlans.first {
            $0.matchingCommand.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == normalized
        } ?? defaultPlan
    }

    public static let phase1 = FixtureScript(
        initialSteps: [
            FixtureStep(
                kind: .sceneChanged,
                content: "The crossroads",
                metadata: ["sceneID": "crossroads", "sceneName": "The crossroads"]
            ),
            FixtureStep(
                kind: .narration,
                content: "Rain darkens the road while the village settles into evening.",
                itemKey: "opening"
            )
        ],
        commandPlans: [
            FixtureCommandPlan(
                matchingCommand: "look around",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Looking around", itemKey: "interpretation"),
                    FixtureStep(kind: .narration, content: "A low stone wall borders the road.", itemKey: "result"),
                    .completed()
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "/look",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Reading the nearby scene", itemKey: "interpretation"),
                    FixtureStep(kind: .actionResult, content: "Crossroads · stone wall · lane to the east", itemKey: "result"),
                    .completed()
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "/people",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Listing nearby people", itemKey: "interpretation"),
                    FixtureStep(kind: .actionResult, content: "Nearby: Peig, Mícheál Connolly, and Róisín Connolly.", itemKey: "result"),
                    .completed()
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "/exits",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Checking the available ways on", itemKey: "interpretation"),
                    FixtureStep(kind: .actionResult, content: "East: the alder lane · South: the road toward the village", itemKey: "result"),
                    .completed()
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "/help",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Showing fixture commands", itemKey: "interpretation"),
                    FixtureStep(kind: .actionResult, content: "Try /look, /people, or /exits, or type a natural-language question.", itemKey: "result"),
                    .completed()
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "ask peig about the old church",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Asking Peig about the old church", itemKey: "interpretation"),
                    .npcChunk("The old church?", speaker: "Peig", sequence: 1),
                    .npcChunk("The old church? It stands beyond the alder trees,", speaker: "Peig", sequence: 2),
                    .npcChunk("The old church? It stands beyond the alder trees, where the path bends toward the hill.", speaker: "Peig", sequence: 3),
                    .completed()
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "ambiguous",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Trying to identify the person", itemKey: "interpretation"),
                    FixtureStep(
                        kind: .clarificationRequired,
                        content: "I’m not sure which Connolly you mean.",
                        clarification: ClarificationPrompt(
                            question: "Which Connolly do you mean?",
                            choices: [
                                ClarificationChoice(id: "micheal", label: "Mícheál Connolly", entityID: "npc-micheal"),
                                ClarificationChoice(id: "roisin", label: "Róisín Connolly", entityID: "npc-roisin")
                            ],
                            ),
                        itemKey: "clarification"
                    )
                ],
                clarificationContinuation: [
                    FixtureStep(kind: .actionResult, content: "You chose carefully; the conversation can continue.", itemKey: "result"),
                    .completed()
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "fail",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Preparing the request", itemKey: "interpretation"),
                    FixtureStep(kind: .error, content: "The fixture response failed before it could be applied.", itemKey: "error"),
                    .completed(.failed)
                ]
            ),
            FixtureCommandPlan(
                matchingCommand: "long stream",
                steps: [
                    FixtureStep(kind: .commandInterpreted, content: "Starting a long response", itemKey: "interpretation"),
                    FixtureStep(kind: .npcDialogue, content: "The first part arrives.", speaker: "Peig", provisional: true, streamSequence: 1, itemKey: "response"),
                    FixtureStep(kind: .npcDialogue, content: "The first part arrives. Then the road opens into rain and light.", speaker: "Peig", provisional: true, streamSequence: 2, itemKey: "response"),
                    FixtureStep(kind: .npcDialogue, content: "The first part arrives. Then the road opens into rain and light. At last, the whole thought is clear.", speaker: "Peig", provisional: true, streamSequence: 3, itemKey: "response"),
                    .completed()
                ]
            )
        ]
    )

    public static func longHistory(count: Int) -> FixtureScript {
        let safeCount = max(0, count)
        let history = (0..<safeCount).map {
            FixtureStep.narrative("Historical fixture entry \($0 + 1).", itemKey: "history-\($0)")
        }
        return FixtureScript(initialSteps: history, commandPlans: phase1.commandPlans, defaultPlan: phase1.defaultPlan)
    }
}

public protocol SessionAdapter: Sendable {
    func events(after cursor: EventCursor?) async -> AsyncThrowingStream<SemanticEvent, Error>
    func submit(
        text: String,
        draftID: DraftID?,
        logicalRequestID: LogicalRequestID?
    ) async throws -> SubmissionReceipt
    func retry(logicalRequestID: LogicalRequestID) async throws -> SubmissionReceipt
    func answerClarification(
        logicalRequestID: LogicalRequestID,
        choiceID: String
    ) async throws -> SubmissionReceipt
    func stop() async throws -> StopReceipt
}

/// Deterministic Phase 1 adapter. It has no clock, network, or game engine;
/// callers explicitly call `step()` to advance the stream.
public actor FixtureSessionAdapter: SessionAdapter {
    /// A consumer that stops draining events must not retain an unbounded
    /// amount of presentation data. Overflow is explicit so the owner can
    /// reconcile from the adapter journal and resubscribe from a fresh cursor.
    public static let eventBufferCapacity = 256

    public let sessionID: SessionID
    public let script: FixtureScript

    private struct ActiveRequest {
        let logicalRequestID: LogicalRequestID
        let attemptID: ExecutionAttemptID
        let originalText: String
        let plan: FixtureCommandPlan
        var steps: [FixtureStep]
        var nextStepIndex: Int
        var waitingForClarification: Bool
    }

    private var activeRequest: ActiveRequest?
    private var requestInputs: [LogicalRequestID: (text: String, plan: FixtureCommandPlan)] = [:]
    private var terminalOutcomes: [LogicalRequestID: ResponseTerminalOutcome] = [:]
    private var eventLog: [SemanticEvent] = []
    private var subscribers: [UUID: AsyncThrowingStream<SemanticEvent, Error>.Continuation] = [:]
    private var initialStepsEmitted = false
    private var nextSequenceValue: UInt64

    public init(sessionID: SessionID = SessionID(), script: FixtureScript = .phase1) {
        self.sessionID = sessionID
        self.script = script
        self.nextSequenceValue = 0
    }

    /// Rebinds a fixture adapter to a previously persisted presentation
    /// snapshot. The adapter does not replay committed opening events, which
    /// avoids duplicate transcript entries after relaunch. An accepted active
    /// request is surfaced once as an interrupted terminal event so the
    /// reducer can make the restored request retryable.
    public init(script: FixtureScript = .phase1, restoring state: SessionState) {
        self.sessionID = state.sessionID
        self.script = script
        self.nextSequenceValue = state.eventCursor.rawValue
        self.initialStepsEmitted = true

        var restoredInputs: [LogicalRequestID: (text: String, plan: FixtureCommandPlan)] = [:]
        var restoredOutcomes: [LogicalRequestID: ResponseTerminalOutcome] = [:]
        for request in state.requests {
            restoredInputs[request.id] = (text: request.originalText, plan: script.plan(for: request.originalText))
            if let terminalOutcome = request.terminalOutcome {
                restoredOutcomes[request.id] = terminalOutcome
            }
        }
        self.requestInputs = restoredInputs

        if let activeRequest = state.activeRequest,
           let attempt = activeRequest.currentAttempt {
            let sequence = state.eventCursor.rawValue + 1
            self.nextSequenceValue = sequence
            // The interruption is a locally synthesized terminal decision.
            // Record it in the adapter's request index as well as the event
            // log so a restored request is immediately retryable once the
            // reducer consumes that event.
            restoredOutcomes[activeRequest.id] = .interrupted
            self.eventLog = [SemanticEvent(
                eventID: SemanticEventID("\(state.sessionID.rawValue):event:\(sequence)"),
                sessionID: state.sessionID,
                sequence: EventSequence(sequence),
                kind: .responseCompleted,
                content: "Interrupted before completion.",
                logicalRequestID: activeRequest.id,
                attemptID: attempt.id,
                transcriptItemID: TranscriptItemID("\(attempt.id.rawValue):restored-interruption"),
                terminalOutcome: .interrupted,
                metadata: ["restored": "true"]
            )]
        } else {
            self.eventLog = []
        }
        self.terminalOutcomes = restoredOutcomes
    }

    public var isStreaming: Bool { activeRequest != nil }
    public var activeRequestID: LogicalRequestID? { activeRequest?.logicalRequestID }
    public var activeAttemptID: ExecutionAttemptID? { activeRequest?.attemptID }
    // `eventLog` intentionally omits already-persisted events after restore,
    // so the cursor must include the restored baseline even when no new
    // event has been synthesized yet.
    public var currentCursor: EventCursor { EventCursor(nextSequenceValue) }

    public func allEvents() -> [SemanticEvent] {
        ensureOpeningSteps()
        return eventLog
    }

    /// Returns a bounded chronological page that ends before `cursor`. The
    /// caller can turn the returned semantic events into transcript items via
    /// the same reducer path as live delivery.
    public func history(before cursor: EventCursor? = nil, limit: Int = 100) -> [SemanticEvent] {
        ensureOpeningSteps()
        let safeLimit = max(1, limit)
        let upperBound = cursor?.rawValue ?? UInt64.max
        return Array(eventLog
            .filter { $0.sequence.rawValue < upperBound }
            .suffix(safeLimit))
    }

    public func events(after cursor: EventCursor? = nil) -> AsyncThrowingStream<SemanticEvent, Error> {
        ensureOpeningSteps()
        let (stream, continuation) = AsyncThrowingStream<SemanticEvent, Error>.makeStream(
            bufferingPolicy: .bufferingOldest(Self.eventBufferCapacity)
        )
        let subscriptionID = UUID()
        continuation.onTermination = { @Sendable [weak self] _ in
            guard let self else { return }
            Task { await self.removeSubscriber(subscriptionID) }
        }
        subscribers[subscriptionID] = continuation

        let lowerBound = cursor?.rawValue ?? 0
        for event in eventLog where event.sequence.rawValue > lowerBound {
            guard deliver(event, to: subscriptionID, continuation: continuation) else { break }
        }
        return stream
    }

    public func finishEventStream() {
        let current = subscribers
        subscribers.removeAll()
        for continuation in current.values { continuation.finish() }
    }

    public func submit(
        text: String,
        draftID: DraftID? = nil,
        logicalRequestID: LogicalRequestID? = nil
    ) async throws -> SubmissionReceipt {
        ensureOpeningSteps()
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw FixtureAdapterError.emptyCommand
        }
        guard activeRequest == nil else { throw FixtureAdapterError.requestInProgress }

        let requestID = logicalRequestID ?? LogicalRequestID()
        if terminalOutcomes[requestID] == .succeeded {
            throw FixtureAdapterError.requestAlreadyCommitted
        }
        let attemptID = ExecutionAttemptID()
        let plan = requestInputs[requestID]?.plan ?? script.plan(for: text)
        requestInputs[requestID] = (text: text, plan: plan)
        activeRequest = ActiveRequest(
            logicalRequestID: requestID,
            attemptID: attemptID,
            originalText: text,
            plan: plan,
            steps: plan.steps,
            nextStepIndex: 0,
            waitingForClarification: false
        )

        let commandItemID = TranscriptItemID("\(attemptID.rawValue):command")
        let commandEvent = makeEvent(
            kind: .playerCommand,
            content: text,
            logicalRequestID: requestID,
            attemptID: attemptID,
            transcriptItemID: commandItemID,
            accepted: true,
            sourceDraftID: draftID
        )
        emit(commandEvent)
        return SubmissionReceipt(
            logicalRequestID: requestID,
            attemptID: attemptID,
            commandEventID: commandEvent.eventID,
            cursor: currentCursor
        )
    }

    public func retry(logicalRequestID: LogicalRequestID) async throws -> SubmissionReceipt {
        ensureOpeningSteps()
        guard activeRequest == nil else { throw FixtureAdapterError.requestInProgress }
        guard let input = requestInputs[logicalRequestID] else { throw FixtureAdapterError.requestNotFound }
        guard terminalOutcomes[logicalRequestID] != .succeeded else { throw FixtureAdapterError.requestAlreadyCommitted }
        guard terminalOutcomes[logicalRequestID] != nil else { throw FixtureAdapterError.requestNotRetryable }

        let attemptID = ExecutionAttemptID()
        activeRequest = ActiveRequest(
            logicalRequestID: logicalRequestID,
            attemptID: attemptID,
            originalText: input.text,
            plan: input.plan,
            steps: input.plan.steps,
            nextStepIndex: 0,
            waitingForClarification: false
        )
        terminalOutcomes[logicalRequestID] = nil

        let progress = makeEvent(
            kind: .progress,
            content: "Retrying the request…",
            logicalRequestID: logicalRequestID,
            attemptID: attemptID,
            metadata: ["retry": "true"]
        )
        emit(progress)
        return SubmissionReceipt(
            logicalRequestID: logicalRequestID,
            attemptID: attemptID,
            accepted: true,
            isRetry: true,
            cursor: currentCursor
        )
    }

    public func answerClarification(
        logicalRequestID: LogicalRequestID,
        choiceID: String
    ) async throws -> SubmissionReceipt {
        ensureOpeningSteps()
        guard var active = activeRequest,
              active.logicalRequestID == logicalRequestID,
              active.waitingForClarification,
              let clarification = active.steps[safe: active.nextStepIndex - 1]?.clarification,
              let choice = clarification.choices.first(where: { $0.id == choiceID }) else {
            if activeRequest == nil { throw FixtureAdapterError.noClarificationPending }
            throw FixtureAdapterError.invalidClarificationChoice
        }

        let selected = makeEvent(
            kind: .clarificationSelected,
            content: choice.label,
            logicalRequestID: logicalRequestID,
            attemptID: active.attemptID,
            transcriptItemID: TranscriptItemID("\(active.attemptID.rawValue):clarification"),
            metadata: ["choiceID": choice.id, "entityID": choice.entityID ?? ""]
        )
        emit(selected)
        active.steps = active.plan.clarificationContinuation
        active.nextStepIndex = 0
        active.waitingForClarification = false
        activeRequest = active
        return SubmissionReceipt(
            logicalRequestID: logicalRequestID,
            attemptID: active.attemptID,
            accepted: true,
            cursor: currentCursor
        )
    }

    public func stop() async -> StopReceipt {
        ensureOpeningSteps()
        guard let active = activeRequest else {
            return StopReceipt(result: .noActiveRequest)
        }
        activeRequest = nil
        let event = makeEvent(
            kind: .responseCompleted,
            logicalRequestID: active.logicalRequestID,
            attemptID: active.attemptID,
            terminalOutcome: .cancelled
        )
        terminalOutcomes[active.logicalRequestID] = .cancelled
        emit(event)
        return StopReceipt(
            result: .cancelled,
            logicalRequestID: active.logicalRequestID,
            attemptID: active.attemptID,
            eventID: event.eventID
        )
    }

    /// Emit one queued fixture step. No timing or sleep is involved, making
    /// Stop and late/duplicate event tests deterministic.
    public func step() async -> FixtureStepResult {
        ensureOpeningSteps()
        guard var active = activeRequest else { return FixtureStepResult(event: nil, isFinished: true) }
        guard !active.waitingForClarification else {
            return FixtureStepResult(event: nil, isAwaitingClarification: true)
        }
        guard active.nextStepIndex < active.steps.count else {
            let completion = makeEvent(
                kind: .responseCompleted,
                logicalRequestID: active.logicalRequestID,
                attemptID: active.attemptID,
                terminalOutcome: .succeeded
            )
            terminalOutcomes[active.logicalRequestID] = .succeeded
            activeRequest = nil
            emit(completion)
            return FixtureStepResult(event: completion, isFinished: true)
        }

        let index = active.nextStepIndex
        let fixtureStep = active.steps[index]
        active.nextStepIndex += 1
        let itemKey = fixtureStep.itemKey ?? (fixtureStep.provisional ? "response" : "item-\(index)")
        let itemID = TranscriptItemID("\(active.attemptID.rawValue):\(itemKey)")
        let streamSequence = fixtureStep.provisional
            ? (fixtureStep.streamSequence ?? UInt64(index + 1))
            : nil
        let event = makeEvent(
            kind: fixtureStep.kind,
            content: fixtureStep.content,
            speaker: fixtureStep.speaker,
            logicalRequestID: active.logicalRequestID,
            attemptID: active.attemptID,
            transcriptItemID: fixtureStep.kind == .responseCompleted ? nil : itemID,
            provisional: fixtureStep.provisional,
            streamSequence: streamSequence,
            streamUpdate: fixtureStep.streamUpdate,
            terminalOutcome: fixtureStep.terminalOutcome,
            clarification: fixtureStep.clarification,
            metadata: fixtureStep.metadata
        )
        emit(event)

        if fixtureStep.kind == .clarificationRequired {
            active.waitingForClarification = true
            activeRequest = active
            return FixtureStepResult(event: event, isAwaitingClarification: true)
        }

        if fixtureStep.kind == .responseCompleted {
            let outcome = fixtureStep.terminalOutcome ?? .succeeded
            terminalOutcomes[active.logicalRequestID] = outcome
            activeRequest = nil
            return FixtureStepResult(event: event, isFinished: true)
        }

        activeRequest = active
        return FixtureStepResult(event: event)
    }

    public func runUntilFinished() async -> [SemanticEvent] {
        ensureOpeningSteps()
        var emitted: [SemanticEvent] = []
        while activeRequest != nil {
            let result = await step()
            if let event = result.event { emitted.append(event) }
            if result.isAwaitingClarification { break }
        }
        return emitted
    }

    private func ensureOpeningSteps() {
        guard !initialStepsEmitted else { return }
        initialStepsEmitted = true
        for (index, step) in script.initialSteps.enumerated() {
            let itemID = TranscriptItemID("opening:\(step.itemKey ?? "item-\(index)")")
            emit(makeEvent(
                kind: step.kind,
                content: step.content,
                speaker: step.speaker,
                transcriptItemID: step.kind == .responseCompleted ? nil : itemID,
                provisional: step.provisional,
                streamSequence: step.streamSequence,
                streamUpdate: step.streamUpdate,
                terminalOutcome: step.terminalOutcome,
                clarification: step.clarification,
                metadata: step.metadata
            ))
        }
    }

    private func makeEvent(
        kind: SemanticEventKind,
        content: String? = nil,
        speaker: String? = nil,
        logicalRequestID: LogicalRequestID? = nil,
        attemptID: ExecutionAttemptID? = nil,
        transcriptItemID: TranscriptItemID? = nil,
        provisional: Bool = false,
        streamSequence: UInt64? = nil,
        streamUpdate: StreamUpdate = .replace,
        terminalOutcome: ResponseTerminalOutcome? = nil,
        accepted: Bool = false,
        sourceDraftID: DraftID? = nil,
        clarification: ClarificationPrompt? = nil,
        metadata: [String: String] = [:]
    ) -> SemanticEvent {
        nextSequenceValue &+= 1
        let sequence = EventSequence(nextSequenceValue)
        let stateRevision = terminalOutcome == .succeeded ? StateRevision(sequence.rawValue) : nil
        return SemanticEvent(
            eventID: SemanticEventID("\(sessionID.rawValue):event:\(sequence.rawValue)"),
            sessionID: sessionID,
            sequence: sequence,
            kind: kind,
            content: content,
            speaker: speaker,
            logicalRequestID: logicalRequestID,
            attemptID: attemptID,
            transcriptItemID: transcriptItemID,
            provisional: provisional,
            streamSequence: streamSequence,
            streamUpdate: streamUpdate,
            terminalOutcome: terminalOutcome,
            accepted: accepted,
            sourceDraftID: sourceDraftID,
            stateRevision: stateRevision,
            clarification: clarification,
            metadata: metadata
        )
    }

    private func emit(_ event: SemanticEvent) {
        eventLog.append(event)
        for (subscriptionID, continuation) in Array(subscribers) {
            _ = deliver(event, to: subscriptionID, continuation: continuation)
        }
    }

    @discardableResult
    private func deliver(
        _ event: SemanticEvent,
        to subscriptionID: UUID,
        continuation: AsyncThrowingStream<SemanticEvent, Error>.Continuation
    ) -> Bool {
        switch continuation.yield(event) {
        case .enqueued:
            return true
        case .dropped:
            subscribers.removeValue(forKey: subscriptionID)
            continuation.finish(throwing: FixtureAdapterError.eventBufferOverflow)
            return false
        case .terminated:
            subscribers.removeValue(forKey: subscriptionID)
            return false
        @unknown default:
            subscribers.removeValue(forKey: subscriptionID)
            continuation.finish(throwing: FixtureAdapterError.eventBufferOverflow)
            return false
        }
    }

    private func removeSubscriber(_ id: UUID) {
        subscribers.removeValue(forKey: id)
    }
}

private extension Array {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
