import XCTest
@testable import QuorumAPI
@testable import QuorumCore

final class QuorumCoreTests: XCTestCase {
    func testPairingLink() {
        let link = PairingLink("quorum://pair?server=https://quorum.tail.ts.net&code=ABCD-EF23")
        XCTAssertEqual(link?.server.absoluteString, "https://quorum.tail.ts.net")
        XCTAssertEqual(link?.code, "ABCD-EF23")
        XCTAssertNil(PairingLink("https://example.com"))
        XCTAssertNil(PairingLink("quorum://pair?server=ftp://x&code=A"))
        XCTAssertEqual(PairingLink.server(from: "quorum.lan:8080/")?.absoluteString, "https://quorum.lan:8080")
        XCTAssertEqual(PairingLink.server(from: "http://10.0.20.5:8080")?.absoluteString, "http://10.0.20.5:8080")
        XCTAssertNil(PairingLink.server(from: "  "))
    }

    func testEventParser() {
        var p = EventParser()
        XCTAssertNil(p.feed(": keep-alive"))
        XCTAssertNil(p.feed("event: snapshot"))
        let e = p.feed("data: {\"service_id\":\"sonarr\",\"health\":\"healthy\"}")
        XCTAssertEqual(e?.name, "snapshot")
        XCTAssertEqual(e?.serviceID, "sonarr")
        XCTAssertNil(p.feed(""))
        XCTAssertEqual(p.feed("data:{}")?.name, "message")
    }

    func testDates() throws {
        let t = FlexibleDateTranscoder()
        let whole = try t.decode("2026-10-07T18:47:05Z")
        let nanos = try t.decode("2026-10-07T18:47:05.123456789Z")
        XCTAssertEqual(nanos.timeIntervalSince(whole), 0.123, accuracy: 0.001)
        XCTAssertEqual(FlexibleDateTranscoder.millis("2026-10-07T18:47:05.5Z"), "2026-10-07T18:47:05.5Z")
        XCTAssertThrowsError(try t.decode("yesterday"))
    }

    func testFormat() {
        XCTAssertEqual(Format.bytes(1_288_490_189), "1.2 GB")
        XCTAssertEqual(Format.bytes(512), "512 B")
        XCTAssertEqual(Format.rate(40_055_603), "38.2 MB/s")
        XCTAssertEqual(Format.percent(0.12), "12 %")
        XCTAssertEqual(Format.duration(Int64(7_500)), "2 h 5 min")
        XCTAssertEqual(Format.duration(Int64(45)), "45 s")
        XCTAssertEqual(Format.ms(1500), "1.5 s")
        var c = DateComponents()
        c.year = 2026; c.month = 10; c.day = 7; c.hour = 18; c.minute = 5
        let d = Calendar.current.date(from: c)!
        XCTAssertEqual(Format.date(d), "7 Oct 2026")
        XCTAssertEqual(Format.time(d), "18:05")
    }

    func testOklch() {
        // The background-ish neutral: oklch(0.3 0 0) is a mid-dark grey.
        let (r, g, b) = OKLCH.toSRGB(l: 0.3, c: 0, h: 0)
        XCTAssertEqual(r, g, accuracy: 0.001)
        XCTAssertEqual(g, b, accuracy: 0.001)
        XCTAssertEqual(r, 0.18, accuracy: 0.02)
        XCTAssertEqual(Symbols.sf("ph-film-strip"), "film")
        XCTAssertEqual(Symbols.sf("unknown"), "square.grid.2x2")
    }
}
