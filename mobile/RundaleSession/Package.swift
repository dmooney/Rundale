// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "RundaleSession",
    platforms: [
        .iOS(.v17),
        .macOS(.v14)
    ],
    products: [
        .library(name: "RundaleSession", targets: ["RundaleSession"])
    ],
    dependencies: [
        .package(path: "../RundaleKit"),
        .package(path: "../RundaleBridge"),
        .package(path: "../LimerickEndpointKit")
    ],
    targets: [
        .target(
            name: "RundaleSession",
            dependencies: [
                .product(name: "RundaleKit", package: "RundaleKit"),
                .product(name: "RundaleBridge", package: "RundaleBridge"),
                .product(name: "LimerickEndpointKit", package: "LimerickEndpointKit")
            ],
            path: "Sources/RundaleSession"
        ),
        .testTarget(
            name: "RundaleSessionTests",
            dependencies: [
                "RundaleSession",
                // The tests script the engine, so the Rust library is not
                // linked; these stand in for its symbols.
                .product(name: "LimerickMobileFFITestSupport", package: "RundaleBridge")
            ],
            path: "Tests/RundaleSessionTests"
        )
    ]
)
