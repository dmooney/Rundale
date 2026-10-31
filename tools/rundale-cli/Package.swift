// swift-tools-version: 6.0

import Foundation
import PackageDescription

/// The Rust engine is linked from its native static library, which
/// `just rundale-cli` builds with Cargo and names in RUNDALE_FFI_LIB_DIR. Without
/// it, the checkout's default Cargo output is used.
let repositoryRoot = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
let ffiLibraryDirectory = ProcessInfo.processInfo.environment["RUNDALE_FFI_LIB_DIR"]
    ?? repositoryRoot.appendingPathComponent("limerick/target/debug").path

let package = Package(
    name: "RundaleCLI",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "rundale-cli", targets: ["rundale-cli"])
    ],
    dependencies: [
        .package(path: "../../mobile/RundaleSession"),
        .package(path: "../../mobile/RundaleKit"),
        .package(path: "../../mobile/RundaleBridge"),
        .package(path: "../../mobile/LimerickEndpointKit"),
        .package(url: "https://github.com/steipete/Swiftdansi.git", exact: "0.3.1")
    ],
    targets: [
        .target(
            name: "RundaleCLICore",
            dependencies: [
                .product(name: "RundaleSession", package: "RundaleSession"),
                .product(name: "RundaleKit", package: "RundaleKit"),
                .product(name: "RundaleBridge", package: "RundaleBridge"),
                .product(name: "LimerickEndpointKit", package: "LimerickEndpointKit")
            ],
            path: "Sources/RundaleCLICore",
            linkerSettings: [
                .unsafeFlags(["-L", ffiLibraryDirectory]),
                .linkedLibrary("limerick_mobile_ffi"),
                .linkedLibrary("m"),
                .linkedLibrary("dl", .when(platforms: [.linux])),
                .linkedLibrary("pthread")
            ]
        ),
        .executableTarget(
            name: "rundale-cli",
            dependencies: [
                "RundaleCLICore",
                .product(name: "Swiftdansi", package: "Swiftdansi")
            ],
            path: "Sources/rundale-cli"
        ),
        .testTarget(
            name: "RundaleCLITests",
            dependencies: ["RundaleCLICore"],
            path: "Tests/RundaleCLITests",
            // Read from the checkout, so a golden update writes them in place.
            exclude: ["Scenarios", "Goldens"]
        )
    ]
)
