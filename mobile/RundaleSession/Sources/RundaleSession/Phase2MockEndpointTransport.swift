import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import LimerickEndpointKit

/// The scripted Limerick Endpoint the phase UI suites play against
/// (`--phase2-mock`): an intent decision on the JSON route, and a
/// dialogue reply streamed in chunks. Lines can ask for an injected failure
/// ("busy once", "offline once", "disconnect once", "fail") or a slower
/// stream ("slow").
public final class Phase2MockEndpointTransport: EndpointTransport, @unchecked Sendable {
    /// Fault-injection state is transport-owned rather than request-owned so a
    /// retry of the same logical request can recover deterministically. Each
    /// mode is consumed once per logical request and a new attempt therefore
    /// receives the normal successful stream.
    private let injectedFailureLock = NSLock()
    private var consumedInjectedFailures = Set<String>()

    private final class TaskBox: @unchecked Sendable {
        var task: Task<Void, Never>?
        let lock = NSLock()

        func set(_ task: Task<Void, Never>) {
            lock.lock(); defer { lock.unlock() }
            self.task = task
        }

        func cancel() {
            lock.lock(); let task = self.task; lock.unlock()
            task?.cancel()
        }
    }

    public init() {}

    public func open(_ request: URLRequest) -> EndpointByteStream {
        let box = TaskBox()
        let requestURL = request.url
        let stream = AsyncThrowingStream<EndpointTransportEvent, Error> { continuation in
            let task = Task {
                do {
                    guard let requestURL else {
                        throw LimerickEndpointError.invalidURL
                    }
                    let identities = try Self.identities(
                        from: request.httpBody,
                        attemptID: request.value(forHTTPHeaderField: "X-Attempt-Id"),
                        url: requestURL
                    )
                    let input = identities.input.lowercased()
                    if identities.role == "player_intent" {
                        // "busy once" rate-limits the intent call on the first
                        // attempt only, so Retry can complete (issue 1825).
                        if input.contains("busy once"),
                           self.claimInjectedFailure(mode: "busy", requestID: identities.requestID) {
                            continuation.yield(.response(statusCode: 429, headers: ["content-type": "application/json"]))
                            continuation.yield(.bytes(try JSONSerialization.data(withJSONObject: [
                                "error": ["code": "RATE_LIMITED", "message": "The Endpoint test is rate limited."]
                            ])))
                            continuation.finish()
                            return
                        }
                        // The intent Endpoint is a decision and does not stream:
                        // its JSON route returns the output object, chosen from
                        // the options the engine offered, as Jev would answer.
                        continuation.yield(.response(statusCode: 200, headers: ["content-type": "application/json"]))
                        let decision = Self.talkDecision(for: identities.intentInput)
                        continuation.yield(.bytes(try JSONSerialization.data(withJSONObject: decision)))
                        continuation.finish()
                        return
                    }
                    continuation.yield(.response(statusCode: 200, headers: ["content-type": "text/event-stream; charset=utf-8"]))
                    if input.contains("offline once"),
                       self.claimInjectedFailure(mode: "offline", requestID: identities.requestID) {
                        throw URLError(.notConnectedToInternet)
                    }
                    if input.contains("fail") {
                        continuation.yield(.bytes(try Self.frame(
                            type: "error", sequence: 1, identities: identities,
                            error: [
                                "code": "PROVIDER_UNAVAILABLE",
                                "message": "The Endpoint test response failed."
                            ]
                        )))
                    } else {
                        let dialogue: String
                        let chunks: [String]
                        if identities.speakerName == "Róisín Connolly" {
                            dialogue = "I trade spun yarn in the village when the household work allows."
                            chunks = ["I trade spun yarn ", "in the village when ", "the household work allows."]
                        } else if identities.speakerName == "Mícheál Connolly" {
                            dialogue = "The wet ground has made moving cattle difficult this week."
                            chunks = ["The wet ground ", "has made moving cattle ", "difficult this week."]
                        } else {
                            dialogue = "The rain keeps the old road quiet. The old church stands beyond the alder trees."
                            chunks = ["The rain keeps ", "the old road quiet. ", "The old church stands beyond the alder trees."]
                        }
                        // Keep the fixture observably incremental so UI tests
                        // can assert the provisional Rust presentation before
                        // the terminal candidate arrives.
                        try await Self.pause(nanoseconds: 100_000_000)
                        for (index, chunk) in chunks.enumerated() {
                            continuation.yield(.bytes(try Self.frame(
                                type: "text_delta", sequence: index + 1, identities: identities, text: chunk
                            )))
                            if input.contains("disconnect once"),
                               index == 0,
                               self.claimInjectedFailure(mode: "disconnect", requestID: identities.requestID) {
                                throw URLError(.networkConnectionLost)
                            }
                            if index + 1 < chunks.count {
                                try await Self.pause(nanoseconds: input.contains("slow") ? 3_000_000_000 : 2_000_000_000)
                            }
                        }
                        continuation.yield(.bytes(try Self.frame(
                            type: "final", sequence: chunks.count + 1, identities: identities,
                            output: ["dialogue": dialogue]
                        )))
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            box.set(task)
            continuation.onTermination = { _ in task.cancel() }
        }
        return EndpointByteStream(events: stream, cancel: { box.cancel() })
    }

    private struct Identities {
        let requestID: String
        let attemptID: String
        let invocationID: String
        let role: String
        let input: String
        let speakerName: String
        let endpointVersion: Int
        /// The whole intent input, for choosing among its options.
        let intentInput: [String: Any]
    }

    private static func identities(from body: Data?, attemptID headerAttemptID: String?, url: URL) throws -> Identities {
        guard let body,
              let root = try JSONSerialization.jsonObject(with: body) as? [String: Any],
              let input = root["input"] as? [String: Any],
              let role = input["role"] as? String,
              let requestID = input["logicalRequestID"] as? String,
              let attemptID = headerAttemptID ?? input["attemptID"] as? String,
              let invocationID = input["idempotencyKey"] as? String,
              // The intent input carries the line as `line`; dialogue as `playerInput`.
              let playerInput = input[role == "player_intent" ? "line" : "playerInput"] as? String else {
            throw LimerickEndpointError.malformedEvent("mock request input is invalid")
        }
        let speakerName = (input["speaker"] as? [String: Any])?["displayName"] as? String
        guard role == "player_intent" || speakerName != nil else {
            throw LimerickEndpointError.malformedEvent("mock dialogue input has no speaker")
        }
        return Identities(
            requestID: requestID,
            attemptID: attemptID,
            invocationID: invocationID,
            role: role,
            input: playerInput,
            speakerName: speakerName ?? "",
            endpointVersion: Self.endpointVersion(from: url),
            intentInput: role == "player_intent" ? input : [:]
        )
    }

    /// A `rundale-intent` decision: speech, with the recipient chosen from the
    /// options the call offered the way Jev answers the canonical cases
    /// (design `docs/design/dynamic-npc-context.md` Appendix A). A word that
    /// fits several people present is that reference; a present person's
    /// first name opening the line, or after "ask", is that person; a line
    /// opening with another name is spoken to someone not present; "all",
    /// "everyone" or "both of you" is everyone; anything else is no one in
    /// particular. With one person present the only option is that person.
    public static func talkDecision(for input: [String: Any]) -> [String: Any] {
        func choice(_ option: String) -> [String: Any] {
            ["choice": option, "confidence": 0.99, "probabilities": [option: 0.99]]
        }
        func fold(_ text: String) -> String {
            text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
        }
        let line = fold(input["line"] as? String ?? "")
        let words = line.split { !$0.isLetter }.map(String.init)
        let present = input["present"] as? [[String: Any]] ?? []
        let offered = { (field: String) in !((input[field] as? [Any]) ?? []).isEmpty }
        func optionID(_ option: [String: Any]) -> String { option["optionId"] as? String ?? "" }
        var recipient = offered("noOne") ? "no_one" : present.first.map(optionID) ?? "no_one"
        let references = input["references"] as? [[String: Any]] ?? []
        let opening = words.first == "ask" ? words.dropFirst().first : words.first
        let notNames: Set = ["hello", "hi", "well", "now", "so", "sure", "good", "aye", "ah", "oh", "grand"]
        let opensWithName = (input["line"] as? String ?? "")
            .range(of: #"^\p{Lu}\p{L}+,"#, options: .regularExpression) != nil
            && !notNames.contains(words.first ?? "")
        if present.count >= 2 {
            if let reference = references.first(where: { words.contains(fold($0["phrase"] as? String ?? "")) }) {
                recipient = optionID(reference)
            } else if let person = present.first(where: {
                fold(($0["name"] as? String ?? "").split(separator: " ").first.map(String.init) ?? "") == opening
            }) {
                recipient = optionID(person)
            } else if words.contains("all") || words.contains("everyone") || line.contains("both of you") {
                recipient = "all"
            } else if opensWithName || words.first == "ask" && opening != "around" {
                recipient = "someone_else"
            }
        }
        return [
            "action": choice("talk"),
            "destination": choice("none"),
            "target": choice("none"),
            "recipient": choice(recipient),
            "mentioned": [String: Any](),
            "atmosphere": choice("none"),
        ]
    }

    private static func endpointVersion(from url: URL) -> Int {
        let components = url.pathComponents
        guard let versionsIndex = components.firstIndex(of: "versions"),
              versionsIndex + 1 < components.count,
              let version = Int(components[versionsIndex + 1]),
              version > 0 else {
            return 1
        }
        return version
    }

    private static func frame(
        type: String,
        sequence: Int,
        identities: Identities,
        text: String? = nil,
        output: [String: Any]? = nil,
        error: [String: Any]? = nil
    ) throws -> Data {
        let eventID = "\(identities.invocationID):\(sequence)"
        var payload: [String: Any] = [
            "contract_version": 1,
            "request_id": identities.requestID,
            "attempt_id": identities.attemptID,
            "invocation_id": identities.invocationID,
            "event_id": eventID,
            "sequence": sequence,
            "type": type,
            "endpoint_version": identities.endpointVersion
        ]
        if let text { payload["text"] = text }
        if let output { payload["output"] = output }
        if let error { payload["error"] = error }
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        var result = Data("event: \(type)\nid: \(eventID)\ndata: ".utf8)
        result.append(data)
        result.append(Data("\n\n".utf8))
        return result
    }

    private static func pause(nanoseconds: UInt64) async throws {
        try await Task.sleep(nanoseconds: nanoseconds)
    }

    private func claimInjectedFailure(mode: String, requestID: String) -> Bool {
        let key = "\(requestID):\(mode)"
        injectedFailureLock.lock()
        defer { injectedFailureLock.unlock() }
        return consumedInjectedFailures.insert(key).inserted
    }
}
