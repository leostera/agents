// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "BorgLLMApple",
    platforms: [
        .macOS(.v10_15),
    ],
    products: [
        .library(
            name: "BorgLLMApple",
            type: .static,
            targets: ["BorgLLMApple"]
        ),
    ],
    targets: [
        .target(
            name: "BorgLLMApple"
        ),
    ]
)
