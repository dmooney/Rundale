import Foundation

public enum CompletionKind: String, Codable, Sendable {
    case slashCommand = "slash_command"
    case npcReference = "npc_reference"
}

public struct CompletionItem: Codable, Equatable, Hashable, Identifiable, Sendable {
    public let id: String
    public let kind: CompletionKind
    public let label: String
    public let insertionText: String
    public let entityID: String?

    public init(
        id: String,
        kind: CompletionKind,
        label: String,
        insertionText: String,
        entityID: String? = nil
    ) {
        self.id = id
        self.kind = kind
        self.label = label
        self.insertionText = insertionText
        self.entityID = entityID
    }
}

public struct FixtureNPCReference: Codable, Equatable, Hashable, Sendable {
    public let id: String
    public let displayName: String
    public let aliases: [String]

    public init(id: String, displayName: String, aliases: [String] = []) {
        self.id = id
        self.displayName = displayName
        self.aliases = aliases
    }
}

/// Phase 1's completion registry is deliberately data-driven. The real
/// engine adapter can provide the same projection without changing composer
/// behavior or making display names authoritative identifiers.
public struct FixtureCompletionRegistry: Codable, Equatable, Sendable {
    public let slashCommands: [CompletionItem]
    public let nearbyNPCs: [FixtureNPCReference]

    public init(
        slashCommands: [CompletionItem] = FixtureCompletionRegistry.defaultSlashCommands,
        nearbyNPCs: [FixtureNPCReference] = [
            FixtureNPCReference(id: "npc-peig", displayName: "Peig", aliases: ["peig"]),
            FixtureNPCReference(id: "npc-micheal", displayName: "Mícheál Connolly", aliases: ["micheal", "michael"]),
            FixtureNPCReference(id: "npc-roisin", displayName: "Róisín Connolly", aliases: ["roisin"])
        ]
    ) {
        self.slashCommands = slashCommands
        self.nearbyNPCs = nearbyNPCs
    }

    public static let defaultSlashCommands: [CompletionItem] = [
        CompletionItem(id: "look", kind: .slashCommand, label: "/look", insertionText: "/look"),
        CompletionItem(id: "people", kind: .slashCommand, label: "/people", insertionText: "/people"),
        CompletionItem(id: "exits", kind: .slashCommand, label: "/exits", insertionText: "/exits"),
        CompletionItem(id: "help", kind: .slashCommand, label: "/help", insertionText: "/help")
    ]

    public static let phase1 = FixtureCompletionRegistry()

    public func suggestions(for text: String) -> [CompletionItem] {
        let token = currentTriggerToken(in: text)
        if token.hasPrefix("/") {
            let query = String(token.dropFirst()).lowercased()
            return slashCommands.filter {
                query.isEmpty || $0.label.dropFirst().lowercased().hasPrefix(query)
            }
        }
        if token.hasPrefix("@") {
            let query = String(token.dropFirst()).folding(options: [.diacriticInsensitive, .caseInsensitive], locale: .current)
            return nearbyNPCs.compactMap { npc in
                let names = [npc.displayName] + npc.aliases
                let matches = query.isEmpty || names.contains {
                    $0.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: .current).hasPrefix(query)
                }
                guard matches else { return nil }
                return CompletionItem(
                    id: npc.id,
                    kind: .npcReference,
                    label: npc.displayName,
                    insertionText: "@\(npc.displayName)",
                    entityID: npc.id
                )
            }
        }
        return []
    }

    /// Replaces the active trigger token and leaves the rest of the draft
    /// untouched. This keeps completion insertion predictable for multiline
    /// drafts and for text following a mention.
    public func applying(_ item: CompletionItem, to text: String) -> String {
        let token = currentTriggerToken(in: text)
        guard !token.isEmpty else { return text + item.insertionText }
        let tokenStart = text.index(text.endIndex, offsetBy: -token.count)
        return String(text[..<tokenStart]) + item.insertionText
    }

    private func currentTriggerToken(in text: String) -> String {
        let separators = CharacterSet.whitespacesAndNewlines
        var start = text.endIndex
        while start > text.startIndex {
            let previous = text.index(before: start)
            let scalarView = text[previous..<start].unicodeScalars
            if scalarView.contains(where: { separators.contains($0) }) { break }
            start = previous
        }
        return String(text[start...])
    }
}
