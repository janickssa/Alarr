import XCTest
@testable import QuorumAPI
@testable import QuorumCore

/// The generated types decode real responses (captured from `quorum-server --demo`).
final class DecodingTests: XCTestCase {
    func fixture<T: Decodable>(_ name: String, as type: T.Type) throws -> T {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try FlexibleDateTranscoder.decoder().decode(type, from: Data(contentsOf: url))
    }

    func testOverviewWidgetMediaHealth() throws {
        let ov = try fixture("overview", as: S.Overview.self)
        XCTAssertFalse(ov.highlights.isEmpty)
        XCTAssertFalse(ov.nodes.isEmpty)
        XCTAssertTrue(ov.problems.contains { $0.quickAction != nil }, "quick actions decode")
        let w = try fixture("widget", as: S.WidgetSummary.self)
        XCTAssertNotNil(w.downloads)
        XCTAssertNotNil(w.streams)
        let media = try fixture("media", as: S.MediaOverview.self)
        XCTAssertFalse(media.requests.isEmpty)
        let health = try fixture("health", as: S.HealthReport.self)
        XCTAssertFalse(health.entries.isEmpty)
        let cluster = try fixture("cluster", as: S.ClusterOverview.self)
        XCTAssertGreaterThan(cluster.nodes.count, 0)
        let services = try fixture("services", as: [S.ServiceView].self)
        XCTAssertTrue(services.contains { $0.linkedGuest != nil }, "linked guests decode")
        _ = try fixture("alert-settings", as: S.AlertSettingsView.self)
    }

    func testEveryModuleMapsToItsCase() throws {
        let expected: [String: String] = [
            "proxmox": "proxmox", "apt-cacher-ng": "healthCheck", "jellyfin": "jellyfin",
            "seerr": "seerr", "sonarr": "arr", "radarr": "arr", "prowlarr": "prowlarr",
            "bazarr": "bazarr", "qbittorrent": "qbittorrent", "navidrome": "navidrome",
            "technitium": "technitium", "nginx-proxy-manager": "npm", "authentik": "authentik",
            "nextcloud": "nextcloud", "wireguard": "wireguard",
        ]
        for (id, name) in expected {
            let snap = try fixture("snap-\(id)", as: S.ServiceSnapshot.self)
            let label = String(describing: snap.data.module).prefix { $0 != "(" }
            XCTAssertEqual(String(label), name, id)
        }
    }

    func testErrorMessageFromOutput() {
        struct Wrapper { let inner: (Int, S.ErrorBody) }
        let f = APIFailure(output: Wrapper(inner: (400, .init(error: "bad_request", message: "confirm is required"))))
        XCTAssertEqual(f.message, "confirm is required")
    }
}
