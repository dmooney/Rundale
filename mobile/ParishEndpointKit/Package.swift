// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "ParishEndpointKit",
    platforms: [
        .iOS(.v17),
        .macOS(.v14)
    ],
    products: [
        .library(name: "ParishEndpointKit", targets: ["ParishEndpointKit"])
    ],
    targets: [
        .target(
            name: "ParishEndpointKit",
            path: "Sources/ParishEndpointKit"
        ),
        .testTarget(
            name: "ParishEndpointKitTests",
            dependencies: ["ParishEndpointKit"],
            path: "Tests/ParishEndpointKitTests",
            swiftSettings: [.define("PARISH_ENDPOINTKIT_TEST_LOOPBACK")]
        )
    ]
)
