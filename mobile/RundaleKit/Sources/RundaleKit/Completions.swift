import Foundation

public enum CompletionKind: String, Codable, Sendable {
    case slashCommand = "slash_command"
    case npcReference = "npc_reference"
    /// A word after a slash command: a subcommand, a duration, or a name.
    case commandArgument = "command_argument"
}

public struct CompletionItem: Codable, Equatable, Hashable, Identifiable, Sendable {
    public let id: String
    public let kind: CompletionKind
    public let label: String
    public let insertionText: String
    public let entityID: String?
    /// A short line shown beside the label, when there is one.
    public let detail: String?

    public init(
        id: String,
        kind: CompletionKind,
        label: String,
        insertionText: String,
        entityID: String? = nil,
        detail: String? = nil
    ) {
        self.id = id
        self.kind = kind
        self.label = label
        self.insertionText = insertionText
        self.entityID = entityID
        self.detail = detail
    }
}

/// A slash command, or a word that may follow one, as the engine's command
/// registry describes it (`readModel.commandCompletions`). Completion walks
/// it a word at a time, so no command list is copied into the app.
public struct SlashCompletionWord: Codable, Equatable, Hashable, Sendable {
    public let word: String
    public let summary: String
    /// The words that may follow this one.
    public let next: [SlashCompletionWord]
    /// Whether a person's name follows instead (anyone in the world).
    public let takesNpc: Bool

    public init(word: String, summary: String, next: [SlashCompletionWord] = [], takesNpc: Bool = false) {
        self.word = word
        self.summary = summary
        self.next = next
        self.takesNpc = takesNpc
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
    /// Every command the session runs, with what may follow each. Typing
    /// `/` offers all of them.
    public let commands: [SlashCompletionWord]
    /// The short list the Commands button offers (product spec §5.4).
    public let advertisedCommands: [CompletionItem]
    public let nearbyNPCs: [FixtureNPCReference]
    /// Everyone in the world, for a command argument that takes a name.
    public let everyone: [FixtureNPCReference]

    public init(
        commands: [SlashCompletionWord] = FixtureCompletionRegistry.defaultCommands,
        advertised: [String]? = nil,
        nearbyNPCs: [FixtureNPCReference] = [
            FixtureNPCReference(id: "npc-peig", displayName: "Peig", aliases: ["peig"]),
            FixtureNPCReference(id: "npc-micheal", displayName: "Mícheál Connolly", aliases: ["micheal", "michael"]),
            FixtureNPCReference(id: "npc-roisin", displayName: "Róisín Connolly", aliases: ["roisin"])
        ],
        everyone: [FixtureNPCReference]? = nil
    ) {
        self.commands = commands
        let advertised = advertised ?? commands.map(\.word)
        self.advertisedCommands = commands
            .filter { advertised.contains($0.word) }
            .map(Self.commandItem)
        self.nearbyNPCs = nearbyNPCs
        self.everyone = everyone ?? nearbyNPCs
    }

    public static let defaultCommands: [SlashCompletionWord] = [
        SlashCompletionWord(word: "/look", summary: "Look around"),
        SlashCompletionWord(word: "/people", summary: "Who is here"),
        SlashCompletionWord(word: "/exits", summary: "Where you can go"),
        SlashCompletionWord(word: "/help", summary: "These commands")
    ]

    public static let phase1 = FixtureCompletionRegistry()

    public func suggestions(for text: String) -> [CompletionItem] {
        if let slash = slashCompletion(for: text) {
            return slash.items
        }
        let token = currentTriggerToken(in: text)
        if token.hasPrefix("@") {
            let query = Self.fold(String(token.dropFirst()))
            return nearbyNPCs.compactMap { npc in
                let names = [npc.displayName] + npc.aliases
                let matches = query.isEmpty || names.contains {
                    Self.fold($0).hasPrefix(query)
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
    /// drafts and for text following a mention. A slash command word
    /// replaces the word being typed (a whole name, spaces and all) and
    /// gains a trailing space, so the next words are offered at once.
    public func applying(_ item: CompletionItem, to text: String) -> String {
        if let slash = slashCompletion(for: text) {
            return String(text[..<slash.wordStart]) + item.insertionText + " "
        }
        let token = currentTriggerToken(in: text)
        guard !token.isEmpty else { return text + item.insertionText }
        let tokenStart = text.index(text.endIndex, offsetBy: -token.count)
        return String(text[..<tokenStart]) + item.insertionText
    }

    /// The words that may come next in a draft that starts with `/`, and
    /// where the word being typed begins. Each finished word (one followed
    /// by a space) must be one the registry offers at that point; a name
    /// argument runs to the end of the draft.
    private func slashCompletion(for text: String) -> (items: [CompletionItem], wordStart: String.Index)? {
        guard text.hasPrefix("/") else { return nil }
        var choices = commands
        var takesNpc = false
        var wordStart = text.startIndex
        var isCommand = true
        while true {
            let rest = text[wordStart...]
            if takesNpc {
                let query = Self.fold(String(rest))
                let items = everyone.filter { npc in
                    let name = Self.fold(npc.displayName)
                    return query.isEmpty || name.hasPrefix(query)
                        || name.split(separator: " ").contains { $0.hasPrefix(query) }
                }.map {
                    CompletionItem(id: $0.id, kind: .commandArgument, label: $0.displayName,
                                   insertionText: $0.displayName)
                }
                return (items, wordStart)
            }
            guard let space = rest.firstIndex(of: " ") else {
                let query = Self.fold(String(rest))
                let items = choices
                    .filter { Self.fold($0.word).hasPrefix(query) }
                    .map { isCommand ? Self.commandItem($0) : Self.argumentItem($0) }
                return (items, wordStart)
            }
            let typed = Self.fold(String(rest[..<space]))
            guard let chosen = choices.first(where: { Self.fold($0.word) == typed }) else {
                return ([], wordStart)
            }
            choices = chosen.next
            takesNpc = chosen.takesNpc
            isCommand = false
            wordStart = text.index(after: space)
        }
    }

    private static func commandItem(_ command: SlashCompletionWord) -> CompletionItem {
        CompletionItem(
            id: String(command.word.drop(while: { $0 == "/" })),
            kind: .slashCommand,
            label: command.word,
            insertionText: command.word
        )
    }

    private static func argumentItem(_ word: SlashCompletionWord) -> CompletionItem {
        CompletionItem(id: word.word, kind: .commandArgument, label: word.word,
                       insertionText: word.word, detail: word.summary)
    }

    /// Lowercased without diacritics, so `micheal` matches Mícheál.
    private static func fold(_ text: String) -> String {
        text.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
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
