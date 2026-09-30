# Mobile scripts

| Script | Purpose |
| --- | --- |
| `build-rust-mobile.sh` | Build `LimerickMobileFFI.xcframework` for device + simulator |
| `install-swift-tools.sh` | Install pinned SwiftLint / SwiftFormat from `tool-versions.toml` |
| `swift_quality.py` | Lint, format check, package tests, optional Xcode lane (#2103) |
| `pick-ios-simulator.py` | Choose an available iPhone simulator UDID for CI |
| `test_swift_quality.py` | Unit tests for the quality runner (no Swift required) |

Phase `./verify`, release, and UI recording remain [#2045](https://github.com/dmooney/Rundale/issues/2045).
Physical-device / TestFlight continuity remains [#2046](https://github.com/dmooney/Rundale/issues/2046).

```sh
just swift-quality
just swift-quality-xcode
python3 -m unittest discover -s mobile/scripts -p 'test_swift_quality.py'
```
