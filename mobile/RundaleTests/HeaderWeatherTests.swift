import XCTest
import UIKit
@testable import Rundale

final class HeaderWeatherTests: XCTestCase {
    func testClearAndCloudySkySymbolsRespectGameTime() {
        for time in ["Dawn", "Morning", "Midday", "Afternoon", "Dusk"] {
            XCTAssertEqual(header("Clear", at: time).weatherSymbol, "sun.max")
            XCTAssertEqual(header("Partly Cloudy", at: time).weatherSymbol, "cloud.sun")
        }
        for time in ["Night", "Midnight"] {
            XCTAssertEqual(header("Clear", at: time).weatherSymbol, "moon.stars")
            XCTAssertEqual(header("Partly Cloudy", at: time).weatherSymbol, "cloud.moon")
        }
    }

    func testEveryEngineConditionHasAnAvailableDistinctSymbol() throws {
        let conditions = ["Clear", "Partly Cloudy", "Overcast", "Light Rain", "Heavy Rain", "Fog", "Storm"]
        let symbols = try conditions.map { condition in
            let symbol = try XCTUnwrap(header(condition).weatherSymbol, condition)
            XCTAssertNotNil(UIImage(systemName: symbol), "Missing system symbol for \(condition)")
            return symbol
        }
        XCTAssertEqual(Set(symbols).count, conditions.count)
        XCTAssertEqual(header("Rain easing").weatherSymbol, header("Light Rain").weatherSymbol)
    }

    func testUnknownWeatherDoesNotInventAConditionIcon() {
        XCTAssertNil(header("Unrecognised condition").weatherSymbol)
    }

    private func header(_ weather: String, at time: String = "Morning") -> PresentedHeader {
        PresentedHeader(location: "Letter Office", timeOfDay: time, weather: weather)
    }
}
