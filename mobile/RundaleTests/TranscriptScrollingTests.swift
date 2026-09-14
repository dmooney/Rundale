import XCTest
import UIKit
import RundaleKit
@testable import Rundale

@MainActor
final class TranscriptScrollingTests: XCTestCase {
    func testTouchWithoutLeavingBottomDoesNotDisableFollowing() {
        let controller = TranscriptCollectionViewController()
        controller.loadViewIfNeeded()
        var changes: [Bool] = []
        controller.onFollowModeChanged = { follows, _ in changes.append(follows) }
        let scroll = controller.view.subviews.compactMap { $0 as? UICollectionView }.first!
        controller.scrollViewWillBeginDragging(scroll)
        controller.scrollViewDidEndDragging(scroll, willDecelerate: false)
        XCTAssertEqual(changes, [], "A touch without scrolling away must keep following new output")
    }

    func testGrowingHostedRowsAndKeyboardResizeKeepTheActualTailVisible() async {
        let controller = TranscriptCollectionViewController()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 375, height: 667))
        window.rootViewController = controller
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let scroll = controller.view.subviews.compactMap { $0 as? UICollectionView }.first!
        var items = (0..<30).map { row("row-\($0)", text: "An earlier line in the conversation.") }
        for count in [1, 8, 25, 45] {
            let streamed = row("stream-\(count)", text: String(repeating: "More of Peig's reply arrives.\n", count: count))
            controller.update(items: items + [streamed], followsNewest: true,
                              initialFollowsNewest: true, initialAnchor: nil)
            controller.view.frame.size.height = count == 1 ? 600 : 280
            controller.view.setNeedsLayout()
            // Allow UIHostingConfiguration's later self-sizing pass to finish.
            try? await Task.sleep(for: .milliseconds(150))
            controller.view.layoutIfNeeded()
            let bottom = max(-scroll.adjustedContentInset.top,
                             scroll.contentSize.height - scroll.bounds.height + scroll.adjustedContentInset.bottom)
            XCTAssertEqual(scroll.contentOffset.y, bottom, accuracy: 2)
            XCTAssertTrue(scroll.indexPathsForVisibleItems.contains(IndexPath(item: items.count, section: 0)))
            items.append(streamed)
        }
    }

    private func row(_ id: String, text: String) -> PresentedTranscriptItem {
        PresentedTranscriptItem(id: id, kind: .npcDialogue, text: text, speaker: "Peig Hannigan",
                                state: .committed, metadata: [:])
    }
}
