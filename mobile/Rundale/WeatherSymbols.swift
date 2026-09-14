extension PresentedHeader {
    /// Match the engine's Weather display values; unknown prose remains text-only.
    var weatherSymbol: String? {
        let isNight = timeOfDay == "Night" || timeOfDay == "Midnight"
        switch weather {
        case "Clear": return isNight ? "moon.stars" : "sun.max"
        case "Partly Cloudy": return isNight ? "cloud.moon" : "cloud.sun"
        case "Overcast": return "cloud"
        case "Light Rain", "Rain easing": return "cloud.drizzle"
        case "Heavy Rain": return "cloud.heavyrain"
        case "Fog": return "cloud.fog"
        case "Storm": return "cloud.bolt.rain"
        default: return nil
        }
    }
}
