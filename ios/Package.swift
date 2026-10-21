// swift-tools-version: 6.0
// The Linux build (xtool + omarchy-apple-dev, see README.md). It builds the same sources as
// project.yml, which stays for building with Xcode on a Mac.
import PackageDescription

let package = Package(
    name: "Quorum",
    platforms: [.iOS(.v17)],
    products: [
        // xtool builds the app and its extensions from these library products.
        .library(name: "Quorum", targets: ["Quorum"]),
        .library(name: "QuorumWidgets", targets: ["QuorumWidgets"]),
    ],
    dependencies: [
        .package(path: "QuorumKit"),
    ],
    targets: [
        .target(
            name: "Quorum",
            dependencies: [.product(name: "QuorumKit", package: "QuorumKit")],
            path: "Quorum",
            resources: [.process("Assets.xcassets")]
        ),
        .target(
            name: "QuorumWidgets",
            dependencies: [.product(name: "QuorumKit", package: "QuorumKit")],
            path: "QuorumWidgets"
        ),
    ],
    swiftLanguageModes: [.v5]
)
