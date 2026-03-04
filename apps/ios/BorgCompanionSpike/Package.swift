// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "BorgCompanionSpike",
    platforms: [
        .iOS(.v17),
        .macOS(.v13),
    ],
    products: [
        .library(name: "BorgCompanionCore", targets: ["BorgCompanionCore"]),
        .executable(name: "BorgCompanionPushFixture", targets: ["BorgCompanionPushFixture"]),
    ],
    targets: [
        .target(
            name: "BorgCompanionCore",
            path: "Sources/BorgCompanionCore"
        ),
        .executableTarget(
            name: "BorgCompanionPushFixture",
            dependencies: ["BorgCompanionCore"],
            path: "Sources/BorgCompanionPushFixture"
        ),
        .testTarget(
            name: "BorgCompanionCoreTests",
            dependencies: ["BorgCompanionCore"],
            path: "Tests/BorgCompanionCoreTests"
        ),
    ]
)
