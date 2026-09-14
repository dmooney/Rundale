import Foundation

public enum SemanticEventKind: String, Codable, CaseIterable, Sendable {
    case sceneChanged = "scene_changed"
    case playerCommand = "player_command"
    case commandInterpreted = "command_interpreted"
    case narration
    case npcDialogue = "npc_dialogue"
    case actionResult = "action_result"
    case clarificationRequired = "clarification_required"
    case clarificationSelected = "clarification_selected"
    case progress
    case error
    case responseCompleted = "response_completed"
}

public enum ResponseTerminalOutcome: String, Codable, Sendable {
    case succeeded
    case cancelled
    case interrupted
    case failed
}

public enum StreamUpdate: String, Codable, Sendable {
    /// The payload is the complete current text for the item.
    case replace
    /// The payload is appended to the current item text.
    case append
}

public struct ClarificationChoice: Codable, Equatable, Hashable, Identifiable, Sendable {
    public let id: String
    public let label: String
    public let entityID: String?

    public init(id: String, label: String, entityID: String? = nil) {
        self.id = id
        self.label = label
        self.entityID = entityID
    }
}

public struct ClarificationPrompt: Codable, Equatable, Hashable, Sendable {
    public let question: String
    public let choices: [ClarificationChoice]

    public init(question: String, choices: [ClarificationChoice]) {
        self.question = question
        self.choices = choices
    }
}

/// A bounded, presentation-oriented event. It intentionally does not expose
/// engine objects or provider response shapes.
public struct SemanticEvent: Codable, Equatable, Hashable, Identifiable, Sendable {
    public let contractVersion: PresentationContractVersion
    public let eventID: SemanticEventID
    public let sessionID: SessionID
    public let sequence: EventSequence
    public let gameTime: Date?
    public let kind: SemanticEventKind
    public let content: String?
    public let speaker: String?
    public let logicalRequestID: LogicalRequestID?
    public let attemptID: ExecutionAttemptID?
    public let transcriptItemID: TranscriptItemID?
    public let provisional: Bool
    public let streamSequence: UInt64?
    public let streamUpdate: StreamUpdate
    public let terminalOutcome: ResponseTerminalOutcome?
    public let accepted: Bool
    public let sourceDraftID: DraftID?
    public let stateRevision: StateRevision?
    public let clarification: ClarificationPrompt?
    public let metadata: [String: String]

    public var id: SemanticEventID { eventID }

    public init(
        contractVersion: PresentationContractVersion = .current,
        eventID: SemanticEventID = SemanticEventID(),
        sessionID: SessionID,
        sequence: EventSequence,
        gameTime: Date? = nil,
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
        stateRevision: StateRevision? = nil,
        clarification: ClarificationPrompt? = nil,
        metadata: [String: String] = [:]
    ) {
        self.contractVersion = contractVersion
        self.eventID = eventID
        self.sessionID = sessionID
        self.sequence = sequence
        self.gameTime = gameTime
        self.kind = kind
        self.content = content
        self.speaker = speaker
        self.logicalRequestID = logicalRequestID
        self.attemptID = attemptID
        self.transcriptItemID = transcriptItemID
        self.provisional = provisional
        self.streamSequence = streamSequence
        self.streamUpdate = streamUpdate
        self.terminalOutcome = terminalOutcome
        self.accepted = accepted
        self.sourceDraftID = sourceDraftID
        self.stateRevision = stateRevision
        self.clarification = clarification
        self.metadata = metadata
    }

    public func encoded(using encoder: JSONEncoder = FixtureJSON.encoder()) throws -> Data {
        try encoder.encode(self)
    }

    public init(decoding data: Data, using decoder: JSONDecoder = FixtureJSON.decoder()) throws {
        self = try decoder.decode(Self.self, from: data)
    }

    private enum CodingKeys: String, CodingKey {
        case contractVersion
        case eventID
        case sessionID
        case sequence
        case gameTime
        case kind
        case content
        case speaker
        case logicalRequestID
        case attemptID
        case transcriptItemID
        case provisional
        case streamSequence
        case streamUpdate
        case terminalOutcome
        case accepted
        case sourceDraftID
        case stateRevision
        case clarification
        case metadata
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let version = try container.decode(PresentationContractVersion.self, forKey: .contractVersion)
        guard version.isSupportedByCurrentClient else {
            throw DecodingError.dataCorruptedError(
                forKey: .contractVersion,
                in: container,
                debugDescription: "Unsupported presentation contract version \(version.major).\(version.minor)"
            )
        }

        contractVersion = version
        eventID = try container.decode(SemanticEventID.self, forKey: .eventID)
        sessionID = try container.decode(SessionID.self, forKey: .sessionID)
        sequence = try container.decode(EventSequence.self, forKey: .sequence)
        gameTime = try container.decodeIfPresent(Date.self, forKey: .gameTime)
        kind = try container.decode(SemanticEventKind.self, forKey: .kind)
        content = try container.decodeIfPresent(String.self, forKey: .content)
        speaker = try container.decodeIfPresent(String.self, forKey: .speaker)
        logicalRequestID = try container.decodeIfPresent(LogicalRequestID.self, forKey: .logicalRequestID)
        attemptID = try container.decodeIfPresent(ExecutionAttemptID.self, forKey: .attemptID)
        transcriptItemID = try container.decodeIfPresent(TranscriptItemID.self, forKey: .transcriptItemID)
        provisional = try container.decodeIfPresent(Bool.self, forKey: .provisional) ?? false
        streamSequence = try container.decodeIfPresent(UInt64.self, forKey: .streamSequence)
        streamUpdate = try container.decodeIfPresent(StreamUpdate.self, forKey: .streamUpdate) ?? .replace
        terminalOutcome = try container.decodeIfPresent(ResponseTerminalOutcome.self, forKey: .terminalOutcome)
        accepted = try container.decodeIfPresent(Bool.self, forKey: .accepted) ?? false
        sourceDraftID = try container.decodeIfPresent(DraftID.self, forKey: .sourceDraftID)
        stateRevision = try container.decodeIfPresent(StateRevision.self, forKey: .stateRevision)
        clarification = try container.decodeIfPresent(ClarificationPrompt.self, forKey: .clarification)
        metadata = try container.decodeIfPresent([String: String].self, forKey: .metadata) ?? [:]
    }
}

/// Shared JSON settings for semantic fixture files and snapshots.
public enum FixtureJSON {
    public static func encoder() -> JSONEncoder {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }

    public static func decoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }

    public static func encode<T: Encodable>(_ value: T) throws -> Data {
        try encoder().encode(value)
    }

    public static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        try decoder().decode(type, from: data)
    }
}
