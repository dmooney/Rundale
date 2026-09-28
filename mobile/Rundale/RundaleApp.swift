import SwiftUI

@main
struct RundaleApp: App {
    @StateObject private var model: RundalePresentationModel

    init() {
        let launch = LaunchConfiguration()
        _model = StateObject(wrappedValue: RundalePresentationModel(launch: launch))
    }

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
        }
    }
}
