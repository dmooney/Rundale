import SwiftUI

/// Native rendering of ChatPanel.svelte's drawing, rotating triquetra.
/// This is transient request activity, never a journal entry or game fact.
struct WaitingAnimation: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @State private var startedAt = Date()

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30,
                                paused: reduceMotion || scenePhase != .active)) { context in
            let elapsed = reduceMotion ? 0 : context.date.timeIntervalSince(startedAt)
            HStack(spacing: 8) {
                knot(elapsed: elapsed)
                    .frame(width: 32, height: 32)
                    .accessibilityHidden(true)
                Text("Having a think…")
                    .font(.caption)
                    .foregroundStyle(RundaleTheme.secondaryInk)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Generating response")
        .accessibilityIdentifier("composer.waiting")
    }

    private func knot(elapsed: TimeInterval) -> some View {
        // Same three circular arcs and 6 s / 2.4 s / 3 s cycles as the web UI.
        let length = 56 * Double.pi
        let phase = max(0, elapsed - 0.4).truncatingRemainder(dividingBy: 3) / 3
        let circleAmount = reduceMotion ? 1 : min(1, min(phase / 0.3, (1 - phase) / 0.3))
        return ZStack {
            Circle()
                .trim(from: 0, to: circleAmount)
                .stroke(RundaleTheme.accent, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .frame(width: 32, height: 32)
                .position(x: 50, y: 50)
            Triquetra()
                .stroke(RundaleTheme.accent, style: StrokeStyle(
                    lineWidth: 3, lineCap: .round, lineJoin: .round,
                    dash: reduceMotion ? [] : [length * 2 / 3, length / 3],
                    dashPhase: -elapsed / 2.4 * length
                ))
        }
        .frame(width: 100, height: 100)
        .rotationEffect(.degrees(elapsed / 6 * 360))
        .scaleEffect(0.32)
        .frame(width: 32, height: 32)
    }
}

private struct Triquetra: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: 50, y: 22))
        path.addArc(center: CGPoint(x: 74.25, y: 36), radius: 28,
                    startAngle: .degrees(210), endAngle: .degrees(90), clockwise: true)
        path.addArc(center: CGPoint(x: 50, y: 78), radius: 28,
                    startAngle: .degrees(330), endAngle: .degrees(210), clockwise: true)
        path.addArc(center: CGPoint(x: 25.75, y: 36), radius: 28,
                    startAngle: .degrees(90), endAngle: .degrees(-30), clockwise: true)
        path.closeSubpath()
        return path
    }
}
