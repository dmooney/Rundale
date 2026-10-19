import RundaleKit
import XCTest
@testable import Rundale

final class UnappliedReplyTests: XCTestCase {
    func testOnlyRepliesTheGameDidNotApplyCarryTheNote() {
        for state in [TranscriptItemState.interrupted, .cancelled, .failed] {
            XCTAssertEqual(item(.npcDialogue, state).unappliedReplyNote, "Not applied", "\(state)")
        }
        for state in [TranscriptItemState.provisional, .committed] {
            XCTAssertNil(item(.npcDialogue, state).unappliedReplyNote, "\(state)")
        }
        // The failure line itself says what happened; it is not a reply.
        XCTAssertNil(item(.error, .failed).unappliedReplyNote)
    }

    private func item(_ kind: SemanticEventKind, _ state: TranscriptItemState) -> PresentedTranscriptItem {
        PresentedTranscriptItem(id: "row", kind: kind, text: "The wet ground", speaker: "Mícheál",
                                state: state, metadata: [:])
    }
}
