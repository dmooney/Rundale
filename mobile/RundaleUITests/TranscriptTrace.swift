import XCTest

/// One transcript-row state the app published, read from the UI-test-only
/// `uitest.transcriptTrace` element.
///
/// Use it for rows that can leave the accessibility tree (virtualized rows
/// scrolled away on a small screen) or that change faster than an XCUITest
/// poll (brief provisional chunks). Keep on-screen queries for what must be
/// visible: the newest row, the header, and controls.
struct TranscriptTraceEntry: Decodable, CustomStringConvertible {
    let row: String
    let kind: String
    let state: String
    let text: String
    let milliseconds: Int

    var description: String { "\(kind)/\(state)@\(milliseconds)ms: \(text)" }
}

extension XCUIApplication {
    /// Every published row state in order; empty if the trace is unavailable.
    func transcriptTrace() -> [TranscriptTraceEntry] {
        let element = descendants(matching: .any)["uitest.transcriptTrace"]
        guard element.waitForExistence(timeout: 3),
              let value = element.value as? String,
              let data = value.data(using: .utf8),
              let entries = try? JSONDecoder().decode([TranscriptTraceEntry].self, from: data)
        else { return [] }
        return entries
    }

    /// The latest published state of each row, in order of first appearance.
    func transcriptRows() -> [TranscriptTraceEntry] {
        var order: [String] = []
        var latest: [String: TranscriptTraceEntry] = [:]
        for entry in transcriptTrace() {
            if latest[entry.row] == nil { order.append(entry.row) }
            latest[entry.row] = entry
        }
        return order.compactMap { latest[$0] }
    }

    /// Polls the trace until a row's latest state satisfies `predicate`.
    func waitForTranscriptRow(timeout: TimeInterval,
                              where predicate: (TranscriptTraceEntry) -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if transcriptRows().contains(where: predicate) { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.25))
        } while Date() < deadline
        return false
    }

    /// Committed NPC dialogue rows, whether or not they are on screen.
    func committedDialogueRows() -> [TranscriptTraceEntry] {
        transcriptRows().filter { $0.kind == "npc_dialogue" && $0.state == "committed" }
    }

    /// Player command rows whose text contains `text`, on screen or not.
    func playerCommandRows(containing text: String) -> [TranscriptTraceEntry] {
        transcriptRows().filter { $0.kind == "player_command" && $0.text.contains(text) }
    }
}
