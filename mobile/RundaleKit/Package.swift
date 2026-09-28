// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "RundaleKit",
    platforms: [
        .iOS(.v17),
        .macOS(.v14)
    ],
    products: [
        .library(name: "RundaleKit", targets: ["RundaleKit"])
    ],
    targets: [
        .target(
            name: "RundaleKit",
            path: "Sources/RundaleKit"
        ),
        .testTarget(
            name: "RundaleKitTests",
            dependencies: ["RundaleKit"],
            path: "Tests/RundaleKitTests",
            resources: [.process("Fixtures")]
        )
    ]
)
