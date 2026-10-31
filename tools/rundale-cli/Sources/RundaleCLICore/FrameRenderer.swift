import Foundation
import RundaleKit
import RundaleSession

/// Why a frame was printed.
public enum FrameReason: String, Sendable {
    case newGame = "new game"
    case restored
    /// Asked for with `:frame`.
    case current
    case submitted
    case streaming
    case clarification
    case committed
    case stopped
    case failed
    case timedOut = "timed out"
}

/// Renders what the presenter shows as plain Markdown: the status header,
/// the transcript, the composer and any pending choices. It reads the
/// presenter only; every rule about what is shown lives in RundaleSession.
///
/// A frame is not app UI: it shows no layout, keyboard, scrolling or
/// accessibility behaviour.
@MainActor
public enum FrameRenderer {
    public static func render(_ presenter: RundaleSessionPresenter, number: Int, reason: FrameReason, inference: String? = nil) -> String {
        let mode = inference.map { " · \($0)" } ?? ""
        var blocks = ["## Frame \(number) · \(reason.rawValue) · \(HostPlatform.label)\(mode)"]
        let header = presenter.header
        blocks.append("**\(header.location)** · \(header.timeOfDay) · \(header.weather)")
        blocks.append("---")
        let rows = presenter.transcript.map(row)
        blocks.append(contentsOf: rows.isEmpty ? ["_No transcript yet._"] : rows)
        blocks.append("---")
        blocks.append(composer(presenter))
        if let message = presenter.submissionMessage {
            blocks.append("> **Notice:** \(message)")
        }
        if let clarification = presenter.clarification {
            var lines = ["**Choose:** \(clarification.prompt)", ""]
            for (index, option) in clarification.options.enumerated() {
                let detail = option.detail.map { " — \($0)" } ?? ""
                lines.append("\(index + 1). \(option.label)\(detail)")
            }
            blocks.append(lines.joined(separator: "\n"))
        }
        return blocks.joined(separator: "\n\n") + "\n"
    }

    static func row(_ item: PresentedTranscriptItem) -> String {
        let text: String
        switch item.kind {
        case .playerCommand:
            text = "› \(item.text)"
        case .sceneChanged:
            let title = item.metadata["sceneName"].flatMap { $0.isEmpty ? nil : "**\($0)**" }
            text = [title, item.text.isEmpty ? nil : emphasized(item.text)]
                .compactMap(\.self)
                .joined(separator: "\n")
        case .npcDialogue:
            let speaker = item.speaker.flatMap { $0.isEmpty ? nil : "**\($0):** " } ?? ""
            text = "\(speaker)“\(item.text)”"
        case .commandInterpreted, .progress, .responseCompleted:
            text = emphasized(item.text)
        case .clarificationSelected:
            text = "_Directed to \(item.text)._"
        case .error:
            text = "**!** \(item.text)"
        case .narration where item.metadata["source"] == "schedule":
            text = emphasized(item.text)
        default:
            text = item.text
        }
        // The app dims a reply it did not apply and notes it under the
        // reply; other rows say so only to VoiceOver. The error line that
        // ended the attempt is shown as it looks, without a note.
        if let note = item.unappliedReplyNote {
            return "\(text)\n_(\(note))_"
        }
        if item.isInterrupted, item.kind != .error {
            return "\(text) _(not applied)_"
        }
        if item.isProvisional {
            return "\(text) _(in progress)_"
        }
        return text
    }

    /// Italic, one paragraph at a time: Markdown emphasis cannot span a
    /// blank line.
    static func emphasized(_ text: String) -> String {
        text.components(separatedBy: "\n\n")
            .map { paragraph in
                let trimmed = paragraph.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.isEmpty ? trimmed : "_\(trimmed)_"
            }
            .joined(separator: "\n\n")
    }

    static func composer(_ presenter: RundaleSessionPresenter) -> String {
        let draft = presenter.draft.isEmpty ? "_empty_" : "`\(presenter.draft)`"
        var controls = [presenter.showsStop ? "[Stop]" : "[Send]"]
        if presenter.showsRetry {
            controls.append("[Retry]")
        }
        return "**Composer:** \(draft) · \(controls.joined(separator: " "))"
    }
}

/// Evidence names the process platform rather than implying a Linux run.
public enum HostPlatform {
    public static var label: String {
        #if os(Linux)
        "Rundale CLI on Linux"
        #else
        "Rundale CLI on macOS"
        #endif
    }
}
