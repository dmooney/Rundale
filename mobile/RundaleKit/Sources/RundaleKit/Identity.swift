import Foundation

/// The version of the small semantic contract consumed by the native player.
///
/// The contract is deliberately separate from the save format and from the
/// future Parish Endpoint contract. A client can therefore reject a payload
/// it cannot safely interpret without treating it as a new game.
public struct PresentationContractVersion: Codable, Comparable, Hashable, Sendable {
    public let major: UInt16
    public let minor: UInt16

    public init(major: UInt16, minor: UInt16) {
        self.major = major
        self.minor = minor
    }

    public static let phase1 = PresentationContractVersion(major: 1, minor: 0)
    public static let current = phase1

    public static func < (lhs: Self, rhs: Self) -> Bool {
        (lhs.major, lhs.minor) < (rhs.major, rhs.minor)
    }

    /// Minor additions are readable by a newer client only when the major
    /// contract remains the same and the payload is no newer than this client.
    public var isSupportedByCurrentClient: Bool {
        major == Self.current.major && minor <= Self.current.minor
    }
}

public struct SessionID: RawRepresentable, Codable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.init(rawValue: rawValue)
    }

    public init() {
        self.init(rawValue: UUID().uuidString.lowercased())
    }

    public var description: String { rawValue }
}

public struct LogicalRequestID: RawRepresentable, Codable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.init(rawValue: rawValue)
    }

    public init() {
        self.init(rawValue: UUID().uuidString.lowercased())
    }

    public var description: String { rawValue }
}

public struct ExecutionAttemptID: RawRepresentable, Codable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.init(rawValue: rawValue)
    }

    public init() {
        self.init(rawValue: UUID().uuidString.lowercased())
    }

    public var description: String { rawValue }
}

public struct TranscriptItemID: RawRepresentable, Codable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.init(rawValue: rawValue)
    }

    public init() {
        self.init(rawValue: UUID().uuidString.lowercased())
    }

    public var description: String { rawValue }
}

public struct SemanticEventID: RawRepresentable, Codable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.init(rawValue: rawValue)
    }

    public init() {
        self.init(rawValue: UUID().uuidString.lowercased())
    }

    public var description: String { rawValue }
}

public struct DraftID: RawRepresentable, Codable, Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public init(_ rawValue: String) {
        self.init(rawValue: rawValue)
    }

    public init() {
        self.init(rawValue: UUID().uuidString.lowercased())
    }

    public var description: String { rawValue }
}

public struct EventSequence: Codable, Comparable, Hashable, Sendable, ExpressibleByIntegerLiteral {
    public let rawValue: UInt64

    public init(_ rawValue: UInt64) {
        self.rawValue = rawValue
    }

    public init(integerLiteral value: UInt64) {
        self.init(value)
    }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

public struct StateRevision: Codable, Comparable, Hashable, Sendable, ExpressibleByIntegerLiteral {
    public let rawValue: UInt64

    public init(_ rawValue: UInt64) {
        self.rawValue = rawValue
    }

    public init(integerLiteral value: UInt64) {
        self.init(value)
    }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

public struct EventCursor: Codable, Comparable, Hashable, Sendable, ExpressibleByIntegerLiteral {
    public let rawValue: UInt64

    public init(_ rawValue: UInt64) {
        self.rawValue = rawValue
    }

    public init(integerLiteral value: UInt64) {
        self.init(value)
    }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}
