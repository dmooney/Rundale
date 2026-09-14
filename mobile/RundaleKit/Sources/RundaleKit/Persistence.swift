import Foundation

public struct FixtureSaveFormatVersion: Codable, Comparable, Hashable, Sendable {
    public let major: UInt16
    public let minor: UInt16

    public init(major: UInt16, minor: UInt16) {
        self.major = major
        self.minor = minor
    }

    public static let current = FixtureSaveFormatVersion(major: 1, minor: 0)

    public static func < (lhs: Self, rhs: Self) -> Bool {
        (lhs.major, lhs.minor) < (rhs.major, rhs.minor)
    }

    public var isSupported: Bool {
        major == Self.current.major && minor <= Self.current.minor
    }
}

public struct SessionSnapshot: Codable, Equatable, Sendable {
    public let formatVersion: FixtureSaveFormatVersion
    public let savedAt: Date
    public let state: SessionState

    public init(
        state: SessionState,
        savedAt: Date = Date(),
        formatVersion: FixtureSaveFormatVersion = .current
    ) {
        self.formatVersion = formatVersion
        self.savedAt = savedAt
        self.state = state
    }

    private enum CodingKeys: String, CodingKey {
        case formatVersion
        case savedAt
        case state
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let formatVersion = try container.decode(FixtureSaveFormatVersion.self, forKey: .formatVersion)
        guard formatVersion.isSupported else {
            throw DecodingError.dataCorruptedError(
                forKey: .formatVersion,
                in: container,
                debugDescription: "Unsupported fixture save format \(formatVersion.major).\(formatVersion.minor)"
            )
        }
        self.formatVersion = formatVersion
        self.savedAt = try container.decode(Date.self, forKey: .savedAt)
        self.state = try container.decode(SessionState.self, forKey: .state)
        guard state.contractVersion.isSupportedByCurrentClient else {
            throw DecodingError.dataCorruptedError(
                forKey: .state,
                in: container,
                debugDescription: "Unsupported presentation contract in fixture save"
            )
        }
    }
}

public enum FixturePersistenceError: Error, Equatable, Sendable, LocalizedError {
    case missingFile(URL)
    case directoryCreationFailed(URL)

    public var errorDescription: String? {
        switch self {
        case let .missingFile(url): return "No fixture save exists at \(url.path)."
        case let .directoryCreationFailed(url): return "Could not create the fixture save directory at \(url.path)."
        }
    }
}

/// Explicit-path fixture persistence. It uses Foundation's atomic write
/// option and never replaces a file until the new encoded snapshot is ready.
/// A decoding/version error leaves the original bytes untouched so callers can
/// surface a compatibility error without silently starting a new session.
public struct FixtureSessionStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL) {
        self.fileURL = fileURL
    }

    public func save(_ state: SessionState, savedAt: Date = Date()) throws {
        try save(SessionSnapshot(state: state, savedAt: savedAt))
    }

    public func save(_ snapshot: SessionSnapshot) throws {
        guard ensureParentDirectory() else {
            throw FixturePersistenceError.directoryCreationFailed(fileURL.deletingLastPathComponent())
        }
        let data = try FixtureJSON.encode(snapshot)
        try data.write(to: fileURL, options: [.atomic])
    }

    public func restoreSnapshot() throws -> SessionSnapshot {
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            throw FixturePersistenceError.missingFile(fileURL)
        }
        // Read/decode before any write. This is the important recovery
        // property for malformed or newer saves.
        let data = try Data(contentsOf: fileURL)
        return try FixtureJSON.decode(SessionSnapshot.self, from: data)
    }

    public func restore() throws -> SessionState {
        try restoreSnapshot().state
    }

    public func remove() throws {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        try FileManager.default.removeItem(at: fileURL)
    }

    private func ensureParentDirectory() -> Bool {
        let directory = fileURL.deletingLastPathComponent()
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            return true
        } catch {
            return false
        }
    }
}

/// Serializes asynchronous session snapshot writes by a host-owned
/// generation. The generation is captured at the main-actor boundary before
/// the write is launched; an older completion can therefore never overwrite a
/// newer snapshot merely because it reaches the file system later.
public actor FixtureSessionSnapshotWriter {
    public let store: FixtureSessionStore

    private var lastWrittenGeneration: UInt64?

    public init(store: FixtureSessionStore) {
        self.store = store
    }

    /// Saves `state` only when `generation` is newer than the last successful
    /// write. Equal and older generations are intentionally ignored, including
    /// when their event cursors happen to be equal. A nil result means either
    /// that the save succeeded or that it was safely superseded; failures are
    /// returned as user-presentable text to match the fixture host boundary.
    public func save(state: SessionState, generation: UInt64) async -> String? {
        if let lastWrittenGeneration, generation <= lastWrittenGeneration {
            return nil
        }

        do {
            try store.save(state)
            lastWrittenGeneration = generation
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}

/// Drafts are lightweight and independent from gameplay completion. A host
/// can save this file at lifecycle/debounce boundaries without pretending an
/// unaccepted command changed the world.
public struct FixtureDraftStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL) {
        self.fileURL = fileURL
    }

    public func save(_ draft: Draft) throws {
        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try FixtureJSON.encode(draft)
        try data.write(to: fileURL, options: [.atomic])
    }

    public func restore() throws -> Draft? {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return nil }
        let data = try Data(contentsOf: fileURL)
        return try FixtureJSON.decode(Draft.self, from: data)
    }

    public func remove() throws {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        try FileManager.default.removeItem(at: fileURL)
    }
}
