// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "BorgMenuSpike",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "BorgMenuSpike", targets: ["BorgMenuSpike"]),
    ],
    targets: [
        .executableTarget(
            name: "BorgMenuSpike",
            path: "Sources",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("Speech"),
            ]
        ),
    ]
)
