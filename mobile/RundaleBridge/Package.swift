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
        .package(path: "../ParishEndpointKit")
    ],
    targets: [
        .target(
            name: "ParishMobileFFI",
            path: "Sources/ParishMobileFFI",
            publicHeadersPath: "include"
        ),
        .target(
            name: "RundaleBridge",
            dependencies: ["RundaleKit", "ParishMobileFFI", "ParishEndpointKit"],
            path: "Sources/RundaleBridge"
        ),
        .target(
            name: "ParishMobileFFITestSupport",
            path: "Tests/ParishMobileFFITestSupport",
            publicHeadersPath: "include"
        ),
        .testTarget(
            name: "RundaleBridgeTests",
            dependencies: ["RundaleBridge", "ParishMobileFFITestSupport"],
            path: "Tests/RundaleBridgeTests"
        )
    ]
)
