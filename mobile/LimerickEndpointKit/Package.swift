// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "LimerickEndpointKit",
    platforms: [
        .iOS(.v17),
        .macOS(.v14)
    ],
    products: [
        .library(name: "LimerickEndpointKit", targets: ["LimerickEndpointKit"])
    ],
    targets: [
        .target(
            name: "LimerickEndpointKit",
            path: "Sources/LimerickEndpointKit"
        ),
        .testTarget(
            name: "LimerickEndpointKitTests",
            dependencies: ["LimerickEndpointKit"],
            path: "Tests/LimerickEndpointKitTests",
            swiftSettings: [.define("LIMERICK_ENDPOINTKIT_TEST_LOOPBACK")]
        )
    ]
)
