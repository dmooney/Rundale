import SwiftUI
import UIKit
import RundaleKit

struct ContentView: View {
    @ObservedObject var model: RundalePresentationModel
    @FocusState private var composerFocused: Bool
    @Environment(\.scenePhase) private var scenePhase
    @State private var followsNewest: Bool
    @State private var hasNewText: Bool
    @State private var hasAppeared = false
    @State private var lastStreamRevision = 0

    init(model: RundalePresentationModel) {
        self.model = model
        _followsNewest = State(initialValue: model.initialFollowsNewest)
        _hasNewText = State(
            initialValue: model.initialUnreadCount > 0 && !model.initialFollowsNewest
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            StatusHeader(model: model)
                .padding(.horizontal, 20)
                .padding(.top, 12)
                .padding(.bottom, 10)

            Divider()
                .overlay(RundaleTheme.rule)

            TranscriptView(
                model: model,
                followsNewest: $followsNewest,
                hasNewText: $hasNewText
            )

            if let message = model.submissionMessage {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(RundaleTheme.error)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 6)
                    .accessibilityIdentifier("composer.error")
            }

            CompletionStrip(model: model)
            ClarificationStrip(model: model)
            Composer(
                model: model,
                focused: $composerFocused
            )
        }
        .background(RundaleTheme.canvas)
        .foregroundStyle(RundaleTheme.ink)
        .tint(RundaleTheme.accent)
        .onAppear {
            guard !hasAppeared else { return }
            hasAppeared = true
            followsNewest = model.initialFollowsNewest
            hasNewText = model.initialUnreadCount > 0 && !followsNewest
            model.start()
            if model.launch.autoFocusComposer {
                composerFocused = true
            }
        }
        .onChange(of: model.streamRevision) { _, revision in
            guard revision != lastStreamRevision else { return }
            lastStreamRevision = revision
            if !followsNewest {
                hasNewText = true
            }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background || phase == .inactive {
                model.persistDraft()
                Task {
                    await model.persistLifecycleSnapshot()
                }
            }
        }
        .preferredColorScheme(model.launch.forceDarkAppearance ? .dark : nil)
    }
}

private struct StatusHeader: View {
    @ObservedObject var model: RundalePresentationModel
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Group {
            if model.launch.isUITesting {
                headerHost
                    .accessibilityValue(testingAccessibilityValue)
            } else {
                headerHost
            }
        }
    }

    private var headerHost: some View {
        headerContent
        .frame(maxWidth: .infinity, alignment: .leading)
        // Keep one stable host element for UI automation and VoiceOver while
        // retaining the compact semantic summary for the header region.
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Current place, \(model.header.location). \(model.header.timeOfDay). \(model.header.weather).")
        .accessibilityIdentifier("status.header")
    }

    private var headerContent: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(model.header.location.uppercased())
                .font(.system(.headline, design: .serif, weight: .semibold))
                .tracking(1.1)
                .accessibilityAddTraits(.isHeader)

            HStack(spacing: 8) {
                Label(model.header.timeOfDay, systemImage: "clock")
                Text("·")
                    .accessibilityHidden(true)
                Label(model.header.weather, systemImage: "cloud.rain")
            }
            .font(.caption)
            .foregroundStyle(RundaleTheme.secondaryInk)
            .labelStyle(.titleAndIcon)
        }
    }

    private var testingAccessibilityValue: String {
        "Dynamic type \(String(describing: dynamicTypeSize)); "
            + "\(colorScheme == .dark ? "dark" : "light") appearance."
    }
}

private struct TranscriptView: View {
    @ObservedObject var model: RundalePresentationModel
    @Binding var followsNewest: Bool
    @Binding var hasNewText: Bool
    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            NativeTranscriptScroller(
                items: model.transcript,
                initialFollowsNewest: model.initialFollowsNewest,
                initialAnchor: model.initialTranscriptAnchor,
                followsNewest: $followsNewest,
                hasNewText: $hasNewText,
                onRecall: model.recallCommand,
                onReadHistory: model.readHistory,
                onFollowNewest: model.followNewest
            )

            if hasNewText && !followsNewest {
                Button {
                    followsNewest = true
                    hasNewText = false
                    model.followNewest()
                } label: {
                    Label("New text", systemImage: "arrow.down.circle.fill")
                        .font(.subheadline.weight(.semibold))
                        .padding(.horizontal, 13)
                        .padding(.vertical, 9)
                        .background(.thinMaterial, in: Capsule())
                        .overlay(Capsule().stroke(RundaleTheme.rule, lineWidth: 0.7))
                }
                .buttonStyle(.plain)
                .foregroundStyle(RundaleTheme.accent)
                .accessibilityHint("Scroll to the newest transcript entry")
                .accessibilityIdentifier("transcript.new-text")
                .padding(.trailing, 18)
                .padding(.bottom, 15)
            }
        }
    }
}

/// SwiftUI's lazy scroll geometry is intentionally not used as the source of
/// truth for transcript position. The native scroll view owns content offset,
/// viewport changes, and drag state while each reusable cell still renders the
/// SwiftUI transcript row already used by the client.
private struct NativeTranscriptScroller: UIViewControllerRepresentable {
    let items: [PresentedTranscriptItem]
    let initialFollowsNewest: Bool
    let initialAnchor: TranscriptAnchor?
    @Binding var followsNewest: Bool
    @Binding var hasNewText: Bool
    let onRecall: (String) -> Void
    let onReadHistory: (TranscriptAnchor?) -> Void
    let onFollowNewest: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIViewController(context: Context) -> TranscriptCollectionViewController {
        let controller = TranscriptCollectionViewController()
        context.coordinator.parent = self
        context.coordinator.connect(to: controller)
        controller.update(
            items: items,
            followsNewest: followsNewest,
            initialFollowsNewest: initialFollowsNewest,
            initialAnchor: initialAnchor
        )
        return controller
    }

    func updateUIViewController(_ controller: TranscriptCollectionViewController,
                                context: Context) {
        context.coordinator.parent = self
        context.coordinator.connect(to: controller)
        controller.update(
            items: items,
            followsNewest: followsNewest,
            initialFollowsNewest: initialFollowsNewest,
            initialAnchor: initialAnchor
        )
    }

    static func dismantleUIViewController(_ controller: TranscriptCollectionViewController,
                                          coordinator: Coordinator) {
        coordinator.disconnect(from: controller)
        controller.disconnect()
    }

    @MainActor
    final class Coordinator {
        var parent: NativeTranscriptScroller
        private weak var connectedController: TranscriptCollectionViewController?

        init(parent: NativeTranscriptScroller) {
            self.parent = parent
        }

        func connect(to controller: TranscriptCollectionViewController) {
            guard connectedController !== controller else { return }
            connectedController?.disconnect()
            connectedController = controller
            controller.onFollowModeChanged = { [weak self, weak controller] follows, anchor in
                guard let self, self.connectedController === controller else { return }
                if self.parent.followsNewest != follows {
                    self.parent.followsNewest = follows
                }
                if follows {
                    self.parent.hasNewText = false
                    self.parent.onFollowNewest()
                } else {
                    self.parent.onReadHistory(anchor)
                }
            }
            controller.onReadingAnchorChanged = { [weak self, weak controller] anchor in
                guard let self, self.connectedController === controller else { return }
                self.parent.onReadHistory(anchor)
            }
            controller.onRecall = { [weak self, weak controller] command in
                guard let self, self.connectedController === controller else { return }
                self.parent.onRecall(command)
            }
        }

        func disconnect(from controller: TranscriptCollectionViewController) {
            guard connectedController === controller else { return }
            controller.disconnect()
            connectedController = nil
        }
    }
}

@MainActor
private final class TranscriptCollectionView: UICollectionView {
    var accessibilityScrollDidFinish: (() -> Void)?

    override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        let didScroll = super.accessibilityScroll(direction)
        if didScroll {
            accessibilityScrollDidFinish?()
        }
        return didScroll
    }
}

@MainActor
private final class TranscriptCollectionViewController: UIViewController,
                                                        UICollectionViewDataSource,
                                                        UICollectionViewDelegate {
    private struct LockedAnchor {
        let id: String
        let viewportOffset: CGFloat
    }

    private var items: [PresentedTranscriptItem] = []
    private var hasLoadedInitialItems = false
    private var isFollowingNewest = true
    private var anchorLock: LockedAnchor?
    private var isApplyingPosition = false
    private var lastBoundsSize: CGSize = .zero
    private var lastContentSize: CGSize = .zero

    var onFollowModeChanged: ((Bool, TranscriptAnchor?) -> Void)?
    var onReadingAnchorChanged: ((TranscriptAnchor?) -> Void)?
    var onRecall: ((String) -> Void)?

    private lazy var collectionView: TranscriptCollectionView = {
        let itemSize = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1),
            heightDimension: .estimated(44)
        )
        let item = NSCollectionLayoutItem(layoutSize: itemSize)
        let group = NSCollectionLayoutGroup.vertical(layoutSize: itemSize, subitems: [item])
        let section = NSCollectionLayoutSection(group: group)
        section.interGroupSpacing = 18
        section.contentInsets = NSDirectionalEdgeInsets(
            top: 20,
            leading: 20,
            bottom: 18,
            trailing: 20
        )
        let layout = UICollectionViewCompositionalLayout(section: section)
        let view = TranscriptCollectionView(frame: .zero, collectionViewLayout: layout)
        view.translatesAutoresizingMaskIntoConstraints = false
        view.backgroundColor = .clear
        view.showsVerticalScrollIndicator = false
        view.alwaysBounceVertical = true
        view.keyboardDismissMode = .interactive
        view.accessibilityIdentifier = "transcript"
        view.accessibilityLabel = "Transcript"
        view.dataSource = self
        view.delegate = self
        view.accessibilityScrollDidFinish = { [weak self] in
            self?.anchorLock = nil
            self?.updateFollowModeFromCurrentPosition()
            self?.persistReadingAnchor()
        }
        return view
    }()

    private lazy var cellRegistration = UICollectionView.CellRegistration<UICollectionViewCell, String> {
        [weak self] cell, indexPath, _ in
        guard let self, self.items.indices.contains(indexPath.item) else { return }
        let transcriptItem = self.items[indexPath.item]
        cell.contentConfiguration = UIHostingConfiguration {
            TranscriptEntry(item: transcriptItem) { [weak self] command in
                self?.onRecall?(command)
            }
        }
        .margins(.all, 0)
        .background(.clear)
        cell.backgroundConfiguration = UIBackgroundConfiguration.clear()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        // UIKit requires registrations to be created before a cell request;
        // evaluating this lazy value from inside `cellForItemAt` crashes.
        _ = cellRegistration
        view.backgroundColor = .clear
        view.addSubview(collectionView)
        NSLayoutConstraint.activate([
            collectionView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            collectionView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            collectionView.topAnchor.constraint(equalTo: view.topAnchor),
            collectionView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        collectionView.layoutIfNeeded()

        let boundsChanged = collectionView.bounds.size != lastBoundsSize
        let contentChanged = collectionView.contentSize != lastContentSize
        lastBoundsSize = collectionView.bounds.size
        lastContentSize = collectionView.contentSize

        guard !isApplyingPosition else { return }
        if isFollowingNewest {
            if (boundsChanged || contentChanged || hasLoadedInitialItems),
               !collectionView.isTracking,
               !collectionView.isDragging,
               !collectionView.isDecelerating {
                pinToNewest()
            }
        } else if let anchorLock {
            if !collectionView.isTracking,
               !collectionView.isDragging,
               !collectionView.isDecelerating {
                apply(anchorLock)
            }
        }
    }

    func update(items newItems: [PresentedTranscriptItem],
                followsNewest: Bool,
                initialFollowsNewest: Bool,
                initialAnchor: TranscriptAnchor?) {
        loadViewIfNeeded()

        if hasLoadedInitialItems, followsNewest != isFollowingNewest {
            if followsNewest {
                // An explicit New text action wins over any remaining drag
                // momentum. Cancel deceleration while delegate classification
                // is suppressed, then enter follow mode and pin immediately;
                // otherwise a final deceleration callback can flip the mode
                // back to history before the next layout pass.
                isApplyingPosition = true
                collectionView.setContentOffset(collectionView.contentOffset, animated: false)
                isApplyingPosition = false
                isFollowingNewest = true
                anchorLock = nil
                pinToNewest()
            } else {
                isFollowingNewest = false
            }
        }

        guard newItems != items || !hasLoadedInitialItems else {
            if followsNewest {
                view.setNeedsLayout()
            }
            return
        }

        let preservedAnchor = hasLoadedInitialItems && !isFollowingNewest
            ? currentAnchor(preferFullyVisible: true)
            : nil
        items = newItems
        collectionView.reloadData()
        collectionView.collectionViewLayout.invalidateLayout()

        if !hasLoadedInitialItems {
            hasLoadedInitialItems = true
            isFollowingNewest = initialFollowsNewest
            if !initialFollowsNewest,
               let initialAnchor,
               let itemID = initialAnchor.itemID?.rawValue {
                anchorLock = LockedAnchor(
                    id: itemID,
                    viewportOffset: CGFloat(initialAnchor.offset)
                )
            }
        } else if !isFollowingNewest, let preservedAnchor {
            if let itemID = preservedAnchor.itemID?.rawValue {
                anchorLock = LockedAnchor(
                    id: itemID,
                    viewportOffset: CGFloat(preservedAnchor.offset)
                )
            }
        }

        view.setNeedsLayout()
    }

    func collectionView(_ collectionView: UICollectionView,
                        numberOfItemsInSection section: Int) -> Int {
        items.count
    }

    func collectionView(_ collectionView: UICollectionView,
                        cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
        collectionView.dequeueConfiguredReusableCell(
            using: cellRegistration,
            for: indexPath,
            item: items[indexPath.item].id
        )
    }

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        anchorLock = nil
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard !isApplyingPosition,
              scrollView.isTracking || scrollView.isDragging || scrollView.isDecelerating else {
            return
        }

        updateFollowModeFromCurrentPosition()
    }

    func scrollViewDidEndDragging(_ scrollView: UIScrollView,
                                  willDecelerate decelerate: Bool) {
        if !decelerate {
            finishUserScrolling()
        }
    }

    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        finishUserScrolling()
    }

    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        updateFollowModeFromCurrentPosition()
        finishUserScrolling()
    }

    func disconnect() {
        onFollowModeChanged = nil
        onReadingAnchorChanged = nil
        onRecall = nil
        collectionView.delegate = nil
        collectionView.dataSource = nil
        collectionView.accessibilityScrollDidFinish = nil
    }

    private var distanceFromNewest: CGFloat {
        let maximumOffset = max(
            -collectionView.adjustedContentInset.top,
            collectionView.contentSize.height
                - collectionView.bounds.height
                + collectionView.adjustedContentInset.bottom
        )
        return max(0, maximumOffset - collectionView.contentOffset.y)
    }

    private func pinToNewest() {
        guard !items.isEmpty else { return }
        isApplyingPosition = true
        collectionView.scrollToItem(
            at: IndexPath(item: items.count - 1, section: 0),
            at: .bottom,
            animated: false
        )
        collectionView.layoutIfNeeded()
        let maximumOffset = max(
            -collectionView.adjustedContentInset.top,
            collectionView.contentSize.height
                - collectionView.bounds.height
                + collectionView.adjustedContentInset.bottom
        )
        collectionView.setContentOffset(
            CGPoint(x: collectionView.contentOffset.x, y: maximumOffset),
            animated: false
        )
        isApplyingPosition = false
        lastContentSize = collectionView.contentSize
    }

    private func apply(_ anchor: LockedAnchor) {
        guard let itemIndex = items.firstIndex(where: { $0.id == anchor.id }) else {
            anchorLock = nil
            return
        }

        isApplyingPosition = true
        let indexPath = IndexPath(item: itemIndex, section: 0)
        collectionView.scrollToItem(at: indexPath, at: .top, animated: false)
        collectionView.layoutIfNeeded()
        if let attributes = collectionView.layoutAttributesForItem(at: indexPath) {
            let y = attributes.frame.minY
                - anchor.viewportOffset
                - collectionView.adjustedContentInset.top
            collectionView.setContentOffset(
                CGPoint(x: collectionView.contentOffset.x, y: y),
                animated: false
            )
            collectionView.layoutIfNeeded()
        }
        isApplyingPosition = false
        lastContentSize = collectionView.contentSize
    }

    private func currentAnchor(preferFullyVisible: Bool) -> TranscriptAnchor? {
        let visibleTop = collectionView.contentOffset.y + collectionView.adjustedContentInset.top
        let attributes = collectionView.indexPathsForVisibleItems.compactMap {
            collectionView.layoutAttributesForItem(at: $0)
        }
        .sorted { $0.frame.minY < $1.frame.minY }

        let selected = attributes.first(where: {
            !preferFullyVisible || $0.frame.minY >= visibleTop - 0.5
        }) ?? attributes.first
        guard let selected,
              items.indices.contains(selected.indexPath.item) else { return nil }
        return TranscriptAnchor(
            itemID: TranscriptItemID(items[selected.indexPath.item].id),
            offset: Double(selected.frame.minY - visibleTop)
        )
    }

    private func persistReadingAnchor() {
        guard !isFollowingNewest else { return }
        let anchor = currentAnchor(preferFullyVisible: true)
        if let anchor, let itemID = anchor.itemID?.rawValue {
            anchorLock = LockedAnchor(
                id: itemID,
                viewportOffset: CGFloat(anchor.offset)
            )
        }
        onReadingAnchorChanged?(anchor)
    }

    private func finishUserScrolling() {
        if isFollowingNewest {
            pinToNewest()
        } else {
            persistReadingAnchor()
        }
    }

    private func updateFollowModeFromCurrentPosition() {
        let nearNewest = distanceFromNewest <= 28
        guard nearNewest != isFollowingNewest else { return }
        isFollowingNewest = nearNewest
        let anchor = nearNewest ? nil : currentAnchor(preferFullyVisible: true)
        onFollowModeChanged?(nearNewest, anchor)
    }
}

private struct TranscriptEntry: View {
    let item: PresentedTranscriptItem
    let onRecall: (String) -> Void

    var body: some View {
        Group {
            if isPlayerCommand {
                Button {
                    onRecall(item.text)
                } label: {
                    renderedContent
                }
                .buttonStyle(.plain)
                .accessibilityHint("Copy this command into the draft for editing")
            } else {
                renderedContent
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityIdentifier("transcript.item.\(item.id)")
    }

    private var isPlayerCommand: Bool {
        item.kind == .playerCommand
    }

    @ViewBuilder
    private var renderedContent: some View {
        Group {
            switch item.kind {
            case .sceneChanged:
                sceneTransition
            case .playerCommand:
                playerCommand
            case .commandInterpreted:
                interpretation
            case .npcDialogue:
                npcDialogue
            case .actionResult:
                deterministic
            case .error:
                statusLine(RundaleTheme.error)
            case .progress:
                statusLine(RundaleTheme.secondaryInk)
            case .responseCompleted:
                statusLine(RundaleTheme.secondaryInk)
            case .clarificationRequired, .clarificationSelected:
                statusLine(RundaleTheme.secondaryInk)
            default:
                narration
            }
        }
    }

    private var accessibilityLabel: String {
        var parts = [kindLabel]
        if let speaker = item.speaker, !speaker.isEmpty {
            parts.append(speaker)
        }
        parts.append(item.text)
        if item.isInterrupted {
            parts.append("Interrupted; not applied")
        } else if item.isProvisional {
            parts.append("In progress")
        }
        return parts.joined(separator: ". ")
    }

    private var kindLabel: String {
        switch item.kind {
        case .playerCommand: return "Player command"
        case .commandInterpreted: return "Interpretation"
        case .npcDialogue: return "Dialogue"
        case .actionResult: return "Result"
        case .sceneChanged: return "Scene"
        case .error: return "Error"
        case .progress: return "Progress"
        case .responseCompleted: return "Response"
        case .clarificationRequired, .clarificationSelected: return "Clarification"
        default: return "Narration"
        }
    }

    private var sceneTransition: some View {
        VStack(alignment: .leading, spacing: 8) {
            Rectangle()
                .fill(RundaleTheme.rule)
                .frame(height: 1)
            Text(item.text.uppercased())
                .font(.system(.subheadline, design: .serif, weight: .semibold))
                .tracking(1.35)
            Rectangle()
                .fill(RundaleTheme.rule)
                .frame(height: 1)
        }
        .padding(.vertical, 2)
    }

    private var narration: some View {
        Text(item.text)
            .font(.system(.body, design: .serif))
            .lineSpacing(4)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var npcDialogue: some View {
        VStack(alignment: .leading, spacing: 5) {
            if let speaker = item.speaker, !speaker.isEmpty {
                Text(speaker.uppercased())
                    .font(.caption.weight(.bold))
                    .tracking(1.05)
                    .foregroundStyle(RundaleTheme.secondaryInk)
            }
            Text("“\(item.text)”")
                .font(.system(.body, design: .serif))
                .italic()
                .lineSpacing(4)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var playerCommand: some View {
        Text("> \(item.text)")
            .font(.system(.body, design: .monospaced))
            .foregroundStyle(RundaleTheme.accent)
            .lineSpacing(3)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var interpretation: some View {
        Text("↳ \(item.text)")
            .font(.system(.subheadline, design: .monospaced))
            .foregroundStyle(RundaleTheme.secondaryInk)
            .padding(.leading, 8)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var deterministic: some View {
        Text(item.text)
            .font(.system(.subheadline, design: .monospaced))
            .foregroundStyle(RundaleTheme.secondaryInk)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func statusLine(_ color: Color) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Circle()
                .fill(color)
                .frame(width: 5, height: 5)
                .accessibilityHidden(true)
            Text(item.text)
                .font(.footnote)
                .foregroundStyle(color)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct CompletionStrip: View {
    @ObservedObject var model: RundalePresentationModel

    var body: some View {
        if !model.completions.isEmpty {
            ScrollView(.horizontal) {
                HStack(spacing: 8) {
                    ForEach(model.completions) { completion in
                        Button {
                            model.selectCompletion(completion)
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(completion.label)
                                    .font(.subheadline.weight(.medium))
                                if let detail = completion.detail {
                                    Text(detail)
                                        .font(.caption)
                                        .foregroundStyle(RundaleTheme.secondaryInk)
                                }
                            }
                            .padding(.horizontal, 11)
                            .padding(.vertical, 8)
                            .background(RundaleTheme.canvas.opacity(0.92), in: RoundedRectangle(cornerRadius: 9))
                            .overlay(RoundedRectangle(cornerRadius: 9).stroke(RundaleTheme.rule, lineWidth: 0.8))
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("completion.\(completion.id)")
                    }
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 7)
            }
            .scrollIndicators(.hidden)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Completions")
            .accessibilityIdentifier("composer.completions")
        }
    }
}

private struct ClarificationStrip: View {
    @ObservedObject var model: RundalePresentationModel

    var body: some View {
        if let clarification = model.clarification {
            VStack(alignment: .leading, spacing: 8) {
                Text(clarification.prompt)
                    .font(.system(.subheadline, design: .serif))
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(clarification.options) { option in
                    Button {
                        model.selectClarification(option)
                    } label: {
                        HStack {
                            Text(option.label)
                                .font(.subheadline.weight(.medium))
                            Spacer(minLength: 8)
                            if let detail = option.detail {
                                Text(detail)
                                    .font(.caption)
                                    .foregroundStyle(RundaleTheme.secondaryInk)
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 9)
                        .background(RundaleTheme.canvas, in: RoundedRectangle(cornerRadius: 9))
                        .overlay(RoundedRectangle(cornerRadius: 9).stroke(RundaleTheme.rule, lineWidth: 0.8))
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("clarification.option.\(option.id)")
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 8)
            .background(RundaleTheme.rule.opacity(0.12))
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Clarification")
            .accessibilityIdentifier("clarification")
        }
    }
}

private struct Composer: View {
    @ObservedObject var model: RundalePresentationModel
    @FocusState.Binding var focused: Bool

    var body: some View {
        VStack(spacing: 8) {
            HStack(alignment: .bottom, spacing: 8) {
                // A vertical-axis TextField keeps the native text-entry
                // surface content-sized: it starts at one line, grows with
                // multiline input, and scrolls internally after five lines.
                // That keeps a blank composer compact on small iPhones while
                // preserving return, selection, dictation, and paste.
                TextField("What do you do?", text: $model.draft, axis: .vertical)
                    .font(.system(.body, design: .serif))
                    .lineLimit(1...5)
                    .textFieldStyle(.plain)
                    .focused($focused)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(RundaleTheme.canvas, in: RoundedRectangle(cornerRadius: 12))
                    .overlay(
                        RoundedRectangle(cornerRadius: 12)
                            .stroke(RundaleTheme.rule, lineWidth: 0.9)
                    )
                    .accessibilityLabel("Command draft")
                    .accessibilityHint("Enter a multiline command")
                    .accessibilityIdentifier("composer.input")
                    .onChange(of: model.draft) { _, _ in
                        model.noteDraftMutation()
                        model.refreshCompletions()
                    }

                VStack(spacing: 7) {
                    if model.isStreaming {
                        Button {
                            model.stop()
                            focused = true
                        } label: {
                            Image(systemName: "stop.fill")
                                .frame(width: 42, height: 42)
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(RundaleTheme.error)
                        .accessibilityLabel("Stop response")
                        .accessibilityIdentifier("composer.stop")
                    } else {
                        Button {
                            model.submitDraft()
                            focused = true
                        } label: {
                            Image(systemName: "arrow.up")
                                .frame(width: 42, height: 42)
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(RundaleTheme.accent)
                        .disabled(model.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        .accessibilityLabel("Send command")
                        .accessibilityIdentifier("composer.send")
                    }

                    if model.hasHistory {
                        Button {
                            model.recallPreviousCommand()
                            focused = true
                        } label: {
                            Image(systemName: "clock.arrow.circlepath")
                                .frame(width: 42, height: 32)
                        }
                        .buttonStyle(.bordered)
                        .accessibilityLabel("Recall previous command")
                        .accessibilityIdentifier("composer.history")
                    }
                }
            }

            HStack(spacing: 13) {
                Text("@ people")
                    .accessibilityHidden(true)
                Text("/ commands")
                    .accessibilityHidden(true)
                Spacer()
                if model.launch.isUITesting && model.launch.manualStream && model.isStreaming {
                    Button("Next") {
                        model.advanceFixture()
                    }
                    .font(.caption.weight(.semibold))
                    .buttonStyle(.bordered)
                    .accessibilityLabel("Advance fixture stream")
                    .accessibilityValue(model.uiTestCheckpoint)
                    .accessibilityIdentifier("fixture.step")
                }
                if model.canRetry && !model.isStreaming {
                    Button("Retry") {
                        model.retryLastFailed()
                        focused = true
                    }
                    .font(.caption.weight(.semibold))
                    .buttonStyle(.bordered)
                    .accessibilityLabel("Retry failed response")
                    .accessibilityIdentifier("composer.retry")
                }
            }
            .font(.caption)
            .foregroundStyle(RundaleTheme.secondaryInk)
            .padding(.horizontal, 4)
        }
        .padding(.horizontal, 16)
        .padding(.top, 10)
        .padding(.bottom, 10)
        .background(RundaleTheme.canvas)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("composer")
    }
}
