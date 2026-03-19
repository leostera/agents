// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "AgentsLLMApple",
    platforms: [
        .macOS(.v10_15),
    ],
    products: [
        .library(
            name: "AgentsLLMApple",
            type: .static,
            targets: ["AgentsLLMApple"]
        ),
    ],
    targets: [
        .target(
            name: "AgentsLLMApple"
        ),
    ]
)
