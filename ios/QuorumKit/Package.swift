// swift-tools-version: 6.0
// Shared code for the Quorum app and its widgets:
// - QuorumAPI: the API client, generated at build time from openapi/openapi.json
//   (swift-openapi-generator build plugin; the spec is a symlink in Sources/QuorumAPI).
// - QuorumCore: Keychain, server-sent events, the widget cache and formatting.
import PackageDescription

let package = Package(
    name: "QuorumKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "QuorumKit", targets: ["QuorumAPI", "QuorumCore"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-openapi-generator", from: "1.10.0"),
        .package(url: "https://github.com/apple/swift-openapi-runtime", from: "1.8.0"),
        .package(url: "https://github.com/apple/swift-openapi-urlsession", from: "1.1.0"),
        .package(url: "https://github.com/apple/swift-http-types", from: "1.0.0"),
    ],
    targets: [
        .target(
            name: "QuorumAPI",
            dependencies: [
                .product(name: "OpenAPIRuntime", package: "swift-openapi-runtime"),
                .product(name: "OpenAPIURLSession", package: "swift-openapi-urlsession"),
                .product(name: "HTTPTypes", package: "swift-http-types"),
            ],
            plugins: [.plugin(name: "OpenAPIGenerator", package: "swift-openapi-generator")]
        ),
        .target(
            name: "QuorumCore",
            dependencies: ["QuorumAPI", .product(name: "OpenAPIRuntime", package: "swift-openapi-runtime")]
        ),
        .testTarget(
            name: "QuorumCoreTests",
            dependencies: ["QuorumCore", "QuorumAPI"],
            resources: [.copy("Fixtures")]
        ),
    ],
    swiftLanguageModes: [.v5]
)
