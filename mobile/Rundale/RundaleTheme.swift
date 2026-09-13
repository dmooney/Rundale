import SwiftUI

enum RundaleTheme {
    static let canvas = Color(light: Color(red: 0.975, green: 0.963, blue: 0.932),
                              dark: Color(red: 0.075, green: 0.073, blue: 0.068))
    static let ink = Color(light: Color(red: 0.14, green: 0.13, blue: 0.11),
                           dark: Color(red: 0.92, green: 0.90, blue: 0.85))
    static let secondaryInk = Color(light: Color(red: 0.38, green: 0.35, blue: 0.29),
                                    dark: Color(red: 0.68, green: 0.65, blue: 0.58))
    static let rule = Color(light: Color(red: 0.79, green: 0.75, blue: 0.67),
                            dark: Color(red: 0.26, green: 0.25, blue: 0.22))
    static let accent = Color(light: Color(red: 0.30, green: 0.24, blue: 0.16),
                              dark: Color(red: 0.78, green: 0.67, blue: 0.45))
    static let error = Color(light: Color(red: 0.63, green: 0.16, blue: 0.12),
                             dark: Color(red: 0.98, green: 0.48, blue: 0.40))
}

private extension Color {
    init(light: Color, dark: Color) {
        self.init(UIColor { traits in
            traits.userInterfaceStyle == .dark ? UIColor(dark) : UIColor(light)
        })
    }
}
