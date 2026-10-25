import Foundation
import RundaleBridge
import RundaleKit

/// The engine calls `RundaleEngineController` makes. `LimerickRuntime` is the
/// one the app opens; tests script their own.
public protocol EngineRuntime: SessionAdapter, AnyObject {
    func close() async throws
    func snapshotJSON() async throws -> Data
    func readEventPageBefore(before cursor: EventCursor, limit: Int) async throws -> LimerickEventPage
    func bugReport(description: String, build: String?) async throws -> String
    func pendingInvocation() async throws -> LimerickPendingInvocation?
    func resolve(_ invocation: LimerickPendingInvocation, output: Data) async throws -> Data
    func fail(
        _ invocation: LimerickPendingInvocation,
        kind: LimerickRuntimeFailureKind,
        message: String,
        reason: LimerickFailureReason?
    ) async throws -> Data
    func frame(_ invocation: LimerickPendingInvocation, sequence: UInt64, text: String) async throws -> Data
}

extension LimerickRuntime: EngineRuntime {}
