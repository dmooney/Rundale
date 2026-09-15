// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "RundaleBridge",
    platforms: [
        .iOS(.v17),
        .macOS(.v14)
    ],
    products: [
        .library(name: "RundaleBridge", targets: ["RundaleBridge"])
    ],
    dependencies: [
        .package(path: "../RundaleKit"),
        .package(path: "../LimerickEndpointKit")
    ],
    targets: [
        .target(
            name: "LimerickMobileFFI",
            path: "Sources/LimerickMobileFFI",
            publicHeadersPath: "include"
        ),
        .target(
            name: "RundaleBridge",
            dependencies: ["RundaleKit", "LimerickMobileFFI", "LimerickEndpointKit"],
            path: "Sources/RundaleBridge"
        ),
        .target(
            name: "LimerickMobileFFITestSupport",
            path: "Tests/LimerickMobileFFITestSupport",
            publicHeadersPath: "include"
        ),
        .testTarget(
            name: "RundaleBridgeTests",
            dependencies: ["RundaleBridge", "LimerickMobileFFITestSupport"],
            path: "Tests/RundaleBridgeTests"
        )
    ]
)
