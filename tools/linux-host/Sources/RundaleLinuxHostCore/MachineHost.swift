import Foundation
import RundaleSession

/// The private command adapter. All player actions go through the shared
/// presenter; this layer only manages process/session ownership and receipts.
@MainActor
public final class MachineHost {
    private let options: HostOptions
    private var host: LinuxHost?
    private var saveLock: HostSaveLock?
    private var history: [[String: Any]] = []
    private var lastSnapshot: Data?
    private var revision: UInt64 = 0
    private var persistence = "not_requested"
    private var persistenceMessage: String?

    public init(options: HostOptions) { self.options = options }

    public func command(_ name: String, args: [String: Any] = [:]) async throws -> [String: Any] {
        switch name {
        case "initialize": return ["protocolVersion": 1]
        case "new_game":
            try requireIdle()
            try await open(reset: true, replace: args["replaceExisting"] as? Bool ?? false)
        case "resume_session":
            try requireIdle()
            try await open(reset: false, replace: false)
        case "session_presentation": break
        case "shutdown", "close_session":
            if name == "close_session" { try requireIdle() }
            try await close()
        case "persist_session":
            try requireIdle()
            guard let host, let presenter = host.presenter else { throw HostCommandError.notOpen }
            await presenter.persistLifecycleSnapshot()
            persistenceMessage = host.sessionController?.persistenceError
            persistence = persistenceMessage == nil ? "saved" : "failed"
            if persistenceMessage != nil { throw HostCommandError.persistence }
        default:
            try perform(name, args: args)
        }
        return response()
    }

    private func perform(_ name: String, args: [String: Any]) throws {
        guard let host, let presenter = host.presenter else { throw HostCommandError.notOpen }
        switch name {
        case "submit_input":
            try requireIdle()
            guard let text = args["text"] as? String, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                throw HostCommandError.invalidArguments
            }
            presenter.draft = text
            presenter.noteDraftMutation()
            guard presenter.canSubmitDraft else { throw HostCommandError.unavailable }
            presenter.submitDraft()
            persistence = "changed"
        case "answer_clarification":
            try requireIdle()
            guard let question = args["questionID"] as? String,
                  question == questionID(),
                  let choice = args["choiceID"] as? String,
                  let option = presenter.clarification?.options.first(where: { $0.id == choice }) else {
                throw HostCommandError.staleChoice
            }
            presenter.selectClarification(option)
            persistence = "changed"
        case "stop":
            guard presenter.showsStop else { throw HostCommandError.unavailable }
            presenter.stop()
        case "retry":
            try requireIdle()
            guard presenter.showsRetry else { throw HostCommandError.unavailable }
            presenter.retryLastFailed()
            persistence = "changed"
        default: throw HostCommandError.unknownCommand
        }
    }

    private func requireIdle() throws {
        if host?.presenter?.isStreaming == true { throw HostCommandError.busy }
    }

    private func open(reset: Bool, replace: Bool) async throws {
        // An open host must be explicitly closed before another context opens.
        guard host == nil else { throw HostCommandError.alreadyOpen }
        let lock = try HostSaveLock(directory: options.saveDirectory)
        let files = try FileManager.default.contentsOfDirectory(atPath: options.saveDirectory.path)
            .filter { $0 != ".session-host.lock" }
        guard !reset || replace || files.isEmpty else { throw HostCommandError.existingSave }
        let projection = options.saveDirectory.appendingPathComponent("projection.json")
        guard reset || FileManager.default.fileExists(atPath: projection.path) else { throw HostCommandError.missingSave }
        if !reset, case .fixture = options.inference {
            // A fixture has only a projection; engine modes require both files.
        } else if !reset, !FileManager.default.fileExists(atPath: options.saveDirectory.appendingPathComponent("phase2.sqlite").path) {
            throw HostCommandError.missingSave
        }
        var candidateOptions = options
        candidateOptions.requireExistingSave = !reset
        let candidate = LinuxHost(options: candidateOptions, output: { _ in })
        saveLock = lock
        host = candidate
        history = []
        lastSnapshot = nil
        persistenceMessage = nil
        persistence = reset ? "changed" : "restored"
        candidate.presentationChanged = { [weak self] in self?.capture() }
        await candidate.open(reset: reset)
        guard !candidate.timedOut, candidate.presenter?.transcript.isEmpty == false,
              candidate.sessionController?.persistenceError == nil,
              candidate.presenter?.submissionMessage == nil else {
            await candidate.close()
            host = nil
            saveLock = nil
            throw HostCommandError.openFailed
        }
    }

    private func close() async throws {
        guard let host else { return }
        await host.close()
        persistenceMessage = host.lastCloseError
        persistence = persistenceMessage == nil ? "saved" : "failed"
        self.host = nil
        saveLock = nil
        history = []
        lastSnapshot = nil
        if persistenceMessage != nil { throw HostCommandError.persistence }
    }

    private func questionID() -> String? {
        guard let pending = host?.sessionController?.state.pendingClarification else { return nil }
        return pending.requestID.rawValue + ":" + pending.attemptID.rawValue + ":" + pending.prompt.question.data(using: .utf8)!.base64EncodedString()
    }

    private func capture() {
        var snapshot = presentation()
        guard let data = try? JSONSerialization.data(withJSONObject: snapshot, options: [.sortedKeys]),
              data != lastSnapshot else { return }
        lastSnapshot = data
        revision &+= 1
        snapshot["revision"] = revision
        history.append(snapshot)
        // Retain streaming evidence without an unbounded event log. Limit
        // by both count and encoded bytes, below the adapter's 4 MiB cap.
        while history.count > 32 || historyBytes() > 2_000_000 { history.removeFirst() }
    }

    private func historyBytes() -> Int {
        (try? JSONSerialization.data(withJSONObject: history).count) ?? 0
    }

    private func response() -> [String: Any] {
        capture()
        var current = presentation()
        current["revision"] = revision
        let markdown = host?.presenter.map { FrameRenderer.render($0, number: Int(revision), reason: .current, inference: inferenceName) }
            ?? "## \(HostPlatform.label) · \(inferenceName)\n\nNo session is open.\n"
        return ["markdown": markdown, "presentation": current, "history": history]
    }

    private var inferenceName: String {
        switch options.inference {
        case .fixture: "fixture"
        case .mock: "mock"
        case .sseFixture: "sse"
        case .live: "live"
        }
    }

    private func presentation() -> [String: Any] {
        var value: [String: Any] = [
            "platform": HostPlatform.label, "inference": inferenceName,
            "sessionOpen": host != nil, "availableActions": ["new_game", "resume_session", "session_presentation"],
            "persistence": ["status": persistence, "message": persistenceMessage as Any? ?? NSNull()]
        ]
        guard let presenter = host?.presenter else { return value }
        var actions = ["session_presentation"]
        if presenter.isStreaming {
            if presenter.showsStop { actions.append("stop") }
        } else {
            actions += ["submit_input", "persist_session", "close_session"]
            if presenter.showsRetry { actions.append("retry") }
            if presenter.clarification != nil { actions.append("answer_clarification") }
        }
        value["availableActions"] = actions
        value["header"] = ["location": presenter.header.location,
                           "timeOfDay": presenter.header.timeOfDay, "weather": presenter.header.weather]
        value["draft"] = presenter.draft
        value["isStreaming"] = presenter.isStreaming
        value["controlPending"] = presenter.isDispatching
        let state = host?.sessionController?.state
        let latestRequest = state?.requests.last
        let requestFailure = latestRequest.flatMap { request in
            request.phase == .failed
                ? state?.transcript.last(where: { $0.logicalRequestID == request.id && $0.kind == .error })?.content
                : nil
        }
        value["failure"] = (presenter.submissionMessage ?? requestFailure) as Any? ?? NSNull()
        if let request = latestRequest {
            value["latestRequest"] = ["requestID": request.id.rawValue, "phase": request.phase.rawValue]
        }
        value["transcript"] = presenter.transcript.map { item -> [String: Any] in
            ["id": item.id, "kind": item.kind.rawValue, "text": item.text,
             "speaker": item.speaker as Any? ?? NSNull(), "state": item.state.rawValue,
             "isProvisional": item.isProvisional]
        }
        if let clarification = presenter.clarification, let question = questionID() {
            value["clarification"] = ["questionID": question, "prompt": clarification.prompt,
                                      "options": clarification.options.map { ["choiceID": $0.id, "label": $0.label] }]
        } else {
            value["clarification"] = NSNull()
        }
        if let error = host?.sessionController?.persistenceError {
            value["persistence"] = ["status": "failed", "message": error]
        }
        return value
    }
}

public enum HostCommandError: String, Error {
    case notOpen, busy, alreadyOpen, existingSave, missingSave, openFailed
    case invalidArguments, unavailable, staleChoice, unknownCommand, persistence, saveLocked
}
