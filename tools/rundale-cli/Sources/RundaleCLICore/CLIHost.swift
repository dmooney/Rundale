import Foundation
import LimerickEndpointKit
import RundaleBridge
import RundaleKit
import RundaleSession

/// How a host session answers model calls.
public enum HostInference: Sendable {
    /// RundaleKit's deterministic fixture session; no engine.
    case fixture
    /// The production Endpoint transport with portable, host-supplied credentials.
    case live(any EndpointCredentialProvider, URL)
    /// The embedded engine with the scripted mock Endpoint the phase UI
    /// suites play against.
    case mock
    /// The embedded engine, replaying a recorded dialogue stream.
    case sseFixture(SSEFixtureEndpointTransport)
}

public struct HostOptions: Sendable {
    public var inference: HostInference
    /// The world the engine plays: the canonical mod, as the app ships it.
    public var modDirectory: URL
    /// Holds the save and the presentation projection.
    public var saveDirectory: URL
    /// Continue the game saved in `saveDirectory` instead of starting anew.
    public var requireExistingSave = false
    public var resume: Bool
    /// How long a reply's text must stay unchanged to be shown as a
    /// streaming frame. A delta replaced sooner is not printed.
    public var streamSettleNanoseconds: UInt64
    /// How long the screen must stay unchanged, with nothing in flight,
    /// before a turn counts as settled.
    public var quietNanoseconds: UInt64
    /// The longest a turn may take before the host reports it timed out.
    public var turnTimeoutNanoseconds: UInt64

    public init(inference: HostInference,
                modDirectory: URL,
                saveDirectory: URL,
                resume: Bool = false,
                streamSettleNanoseconds: UInt64 = 500_000_000,
                quietNanoseconds: UInt64 = 400_000_000,
                turnTimeoutNanoseconds: UInt64 = 60_000_000_000) {
        self.inference = inference
        self.modDirectory = modDirectory
        self.saveDirectory = saveDirectory
        self.resume = resume
        self.streamSettleNanoseconds = streamSettleNanoseconds
        self.quietNanoseconds = quietNanoseconds
        self.turnTimeoutNanoseconds = turnTimeoutNanoseconds
    }
}

/// Runs the app's session from the command line and prints the screen as Markdown frames.
///
/// It drives `RundaleSessionPresenter` the way the app's views do (setting
/// the draft, Send, Stop, Retry, a clarification choice) and reads frames
/// from the presenter only. It decides no game or session behaviour.
@MainActor
public final class CLIHost {
    public static let help = """
    Type a line to send it, as the composer does. Host commands:
      N or :choose N   pick choice N
      :stream TEXT     send TEXT and return once the reply starts streaming
      :stop            tap Stop
      :retry           tap Retry
      :restart         save, close the game, and open it again
      :frame           print the current frame
      :quit            save and close
    Lines starting with # are comments.
    """

    private let options: HostOptions
    private let output: (String) -> Void
    var presenter: RundaleSessionPresenter?
    var sessionController: (any RundaleSessionControlling)?
    var presentationChanged: (@MainActor () -> Void)?
    private var runtime: RuntimeHandle?
    private var frameNumber = 0
    private var lastChange = DispatchTime.now().uptimeNanoseconds
    private var lastStreamingText: String?

    /// Whether a turn ran past `turnTimeoutNanoseconds`.
    public private(set) var timedOut = false
    var lastCloseError: String?

    public init(options: HostOptions, output: @escaping (String) -> Void) {
        self.options = options
        self.output = output
    }

    /// Opens the game and prints its first frame.
    public func open() async {
        await open(reset: !options.resume)
    }

    /// Runs one input line. Returns false after `:quit`.
    @discardableResult
    public func perform(_ line: String) async -> Bool {
        let line = line.trimmingCharacters(in: .whitespaces)
        guard !line.isEmpty, !line.hasPrefix("#") else { return true }
        output("> **Input:** `\(line)`\n")
        guard let presenter else { return false }
        if let choice = Int(line) ?? line.commandArgument(":choose").flatMap({ Int($0) }) {
            await choose(choice, presenter: presenter)
            return true
        }
        switch line {
        case ":stop":
            guard presenter.showsStop else {
                output("_Stop is not offered._\n")
                return true
            }
            presenter.stop()
            await settle(after: .stopped)
        case ":retry":
            guard presenter.showsRetry else {
                output("_Retry is not offered._\n")
                return true
            }
            presenter.retryLastFailed()
            await settle(after: nil)
        case ":restart":
            await close()
            await open(reset: false)
        case ":frame":
            emit(.current, presenter: presenter)
        case ":quit":
            await close()
            return false
        case ":help":
            output("```\n\(Self.help)\n```\n")
        default:
            if let text = line.commandArgument(":stream") {
                submit(text, presenter: presenter)
                await settle(after: nil, returningWhenStreaming: true)
            } else if line.hasPrefix(":") {
                output("_Unknown host command. `:help` lists them._\n")
            } else {
                submit(line, presenter: presenter)
                await settle(after: nil)
            }
        }
        return true
    }

    /// Stops a reply still in flight, as the app does when it leaves the
    /// foreground, saves the presentation, then closes the engine.
    public func close() async {
        guard let presenter else { return }
        if presenter.showsStop {
            presenter.stop()
            await settle(after: .stopped)
        }
        await presenter.persistLifecycleSnapshot()
        lastCloseError = sessionController?.persistenceError
        self.presenter = nil
        sessionController = nil
        do {
            try await runtime?.close()
        } catch {
            lastCloseError = "The engine did not close cleanly."
            output("_The engine did not close cleanly._\n")
        }
        runtime = nil
    }

    func open(reset: Bool) async {
        let launch = launchConfiguration(reset: reset)
        let session: any RundaleSessionControlling
        switch options.inference {
        case .fixture:
            session = RundaleFixtureController(configuration: launch)
        case .mock, .sseFixture, .live:
            let handle = RuntimeHandle()
            let requireExistingSave = options.requireExistingSave
            runtime = handle
            session = RundaleEngineController(
                configuration: launch,
                credentials: {
                    if case let .live(credentials, _) = self.options.inference { return credentials }
                    return StaticEndpointCredentialProvider(EndpointCredentials(
                        authorizationToken: "rundale-cli", appCheckToken: "rundale-cli"
                    ))
                },
                modDirectory: options.modDirectory,
                log: { _ in },
                transport: transport(),
                openRuntime: { payload in
                    var openPayload = payload
                    if requireExistingSave {
                        var object = try JSONSerialization.jsonObject(with: payload) as? [String: Any] ?? [:]
                        object["require_existing"] = true
                        openPayload = try JSONSerialization.data(withJSONObject: object)
                    }
                    let runtime = try LimerickRuntime.openResume(payload: openPayload)
                    handle.set(runtime)
                    return runtime
                }
            )
        }
        sessionController = session
        let presenter = RundaleSessionPresenter(
            launch: launch,
            session: session,
            bugReports: nil,
            captureScreenshot: { nil },
            deviceDescription: HostPlatform.label
        )
        presenter.willChange = { [weak self] in
            self?.lastChange = DispatchTime.now().uptimeNanoseconds
            // Presenter notifications precede mutation. Read only after this
            // main-actor update has completed, never a half-updated value.
            Task { @MainActor [weak self] in self?.presentationChanged?() }
        }
        self.presenter = presenter
        // Let the shared projection reader reject a corrupt projection before
        // the engine opens or any recovery policy can change the save.
        if options.requireExistingSave, session.persistenceError != nil { return }
        presenter.start()
        // The engine opens off the main actor. Opening runs no model call,
        // so the game is ready once its first snapshot (or an error) shows.
        let deadline = DispatchTime.now().uptimeNanoseconds + options.turnTimeoutNanoseconds
        while presenter.transcript.isEmpty, presenter.submissionMessage == nil,
              DispatchTime.now().uptimeNanoseconds < deadline {
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        guard !presenter.transcript.isEmpty || presenter.submissionMessage != nil else {
            timedOut = true
            emit(.timedOut, presenter: presenter)
            return
        }
        await settle(after: reset ? .newGame : .restored)
    }

    private func launchConfiguration(reset: Bool) -> LaunchConfiguration {
        let projection = options.saveDirectory.appendingPathComponent("projection.json")
        var arguments = ["rundale-cli", "--draft-file=\(projection.path)"]
        var environment: [String: String] = [:]
        if case .fixture = options.inference {
            arguments.append("--fixture=standard")
        } else {
            arguments.append("--phase2")
            // Every Endpoint call goes to the injected transport; the origin
            // only has to be a valid HTTPS URL.
            if case let .live(_, url) = options.inference {
                environment["RUNDALE_ENDPOINT_BASE_URL"] = url.absoluteString
                environment["RUNDALE_ENDPOINT_ORGANIZATION"] = "limerick-demo"
            } else {
                environment["RUNDALE_ENDPOINT_BASE_URL"] = "https://endpoints.rundale-cli.invalid"
            }
        }
        if reset {
            arguments.append("--reset-fixture")
        }
        return LaunchConfiguration(arguments: arguments, environment: environment, bundle: [:])
    }

    private func transport() -> any EndpointTransport {
        if case .live = options.inference { return URLSessionEndpointTransport() }
        if case let .sseFixture(transport) = options.inference {
            return transport
        }
        return Phase2MockEndpointTransport()
    }

    private func submit(_ text: String, presenter: RundaleSessionPresenter) {
        presenter.draft = text
        presenter.noteDraftMutation()
        presenter.submitDraft()
        emit(.submitted, presenter: presenter)
    }

    private func choose(_ number: Int, presenter: RundaleSessionPresenter) async {
        guard let options = presenter.clarification?.options,
              options.indices.contains(number - 1) else {
            output("_No choice \(number) is offered._\n")
            return
        }
        presenter.selectClarification(options[number - 1])
        // The answer is applied asynchronously; wait until the question has
        // gone before waiting for the reply.
        let deadline = DispatchTime.now().uptimeNanoseconds + self.options.turnTimeoutNanoseconds
        while presenter.clarification != nil, DispatchTime.now().uptimeNanoseconds < deadline {
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        await settle(after: nil)
    }

    /// Waits until nothing is in flight and the screen has stopped changing,
    /// printing a streaming frame each time a reply's text holds still, then
    /// the settled frame.
    private func settle(after action: FrameReason?, returningWhenStreaming: Bool = false) async {
        lastStreamingText = nil
        var streamingText: String?
        var streamingSince = DispatchTime.now().uptimeNanoseconds
        let deadline = DispatchTime.now().uptimeNanoseconds + options.turnTimeoutNanoseconds
        while let presenter {
            try? await Task.sleep(nanoseconds: 20_000_000)
            let now = DispatchTime.now().uptimeNanoseconds
            let text = presenter.transcript.last(where: \.isProvisional).map(\.text)
            if text != streamingText {
                streamingText = text
                streamingSince = now
            } else if let text, !text.isEmpty, text != lastStreamingText,
                      now - streamingSince >= options.streamSettleNanoseconds {
                lastStreamingText = text
                emit(.streaming, presenter: presenter)
                if returningWhenStreaming { return }
            }
            if !presenter.isStreaming, now - lastChange >= options.quietNanoseconds {
                emit(settledReason(action, presenter: presenter), presenter: presenter)
                return
            }
            if now >= deadline {
                timedOut = true
                emit(.timedOut, presenter: presenter)
                return
            }
        }
    }

    private func settledReason(_ action: FrameReason?, presenter: RundaleSessionPresenter) -> FrameReason {
        if let action { return action }
        if presenter.clarification != nil { return .clarification }
        return presenter.showsRetry ? .failed : .committed
    }

    private func emit(_ reason: FrameReason, presenter: RundaleSessionPresenter) {
        frameNumber += 1
        output(FrameRenderer.render(presenter, number: frameNumber, reason: reason))
    }
}

/// The engine the controller opened, so the host can close it before
/// opening the save again.
private final class RuntimeHandle: @unchecked Sendable {
    private let lock = NSLock()
    private var runtime: LimerickRuntime?

    func set(_ runtime: LimerickRuntime) {
        lock.lock()
        defer { lock.unlock() }
        self.runtime = runtime
    }

    func close() async throws {
        try await take()?.close()
    }

    private func take() -> LimerickRuntime? {
        lock.lock()
        defer { lock.unlock() }
        let runtime = runtime
        self.runtime = nil
        return runtime
    }
}

private extension String {
    /// The text after `command` and a space, when the line is that command.
    func commandArgument(_ command: String) -> String? {
        guard hasPrefix(command + " ") else { return nil }
        return String(dropFirst(command.count + 1)).trimmingCharacters(in: .whitespaces)
    }
}
