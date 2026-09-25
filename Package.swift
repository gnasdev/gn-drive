// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "gn-drive",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "GNDriveCore", targets: ["GNDriveCore"]),
        .executable(name: "gn-drive", targets: ["gn-drive"]),
        .executable(name: "GNDriveApp", targets: ["GNDriveApp"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-argument-parser.git", .upToNextMajor(from: "1.5.0")),
    ],
    targets: [
        .target(
            name: "CArgon2",
            publicHeadersPath: "include"
        ),
        .target(
            name: "GNDriveCore",
            dependencies: ["CArgon2"],
            linkerSettings: [.linkedLibrary("sqlite3")]
        ),
        .executableTarget(
            name: "gn-drive",
            dependencies: [
                "GNDriveCore",
                .product(name: "ArgumentParser", package: "swift-argument-parser"),
            ]
        ),
        .executableTarget(
            name: "GNDriveApp",
            dependencies: ["GNDriveCore"]
        ),
        .testTarget(
            name: "GNDriveCoreTests",
            dependencies: ["GNDriveCore"]
        ),
    ]
)
