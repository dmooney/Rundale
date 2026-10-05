import UIKit

extension Notification.Name {
    /// The player shook the phone. Beta builds report a bug (#2022).
    static let rundaleDeviceDidShake = Notification.Name("RundaleDeviceDidShake")
}

extension UIWindow {
    /// A shake travels up the responder chain from whatever has focus
    /// (usually the composer) to the window, so the window hears it while
    /// the player types.
    override open func motionEnded(_ motion: UIEvent.EventSubtype, with event: UIEvent?) {
        if motion == .motionShake {
            NotificationCenter.default.post(name: .rundaleDeviceDidShake, object: self)
        }
        super.motionEnded(motion, with: event)
    }
}
