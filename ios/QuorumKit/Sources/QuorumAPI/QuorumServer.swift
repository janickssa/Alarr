import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HTTPTypes
import OpenAPIRuntime
import OpenAPIURLSession

/// Builds API clients for a Quorum server.
public enum QuorumServer {
    /// A client for `<server>/api/v1`. Without a token (pairing) no `Authorization` header is sent.
    public static func client(server: URL, token: String?, session: URLSession = .shared) -> Client {
        Client(
            serverURL: apiURL(server),
            configuration: Configuration(dateTranscoder: FlexibleDateTranscoder()),
            transport: URLSessionTransport(configuration: .init(session: session)),
            middlewares: token.map { [BearerMiddleware(token: $0)] } ?? []
        )
    }

    public static func apiURL(_ server: URL) -> URL {
        server.appending(path: "api/v1")
    }
}

/// Sends the device token with every request.
public struct BearerMiddleware: ClientMiddleware {
    public let token: String

    public init(token: String) {
        self.token = token
    }

    public func intercept(
        _ request: HTTPRequest,
        body: HTTPBody?,
        baseURL: URL,
        operationID: String,
        next: @Sendable (HTTPRequest, HTTPBody?, URL) async throws -> (HTTPResponse, HTTPBody?)
    ) async throws -> (HTTPResponse, HTTPBody?) {
        var request = request
        request.headerFields[.authorization] = "Bearer \(token)"
        return try await next(request, body, baseURL)
    }
}

/// ISO 8601 dates with or without fractional seconds: the server writes `…:05Z` for whole
/// seconds and up to nine fraction digits otherwise, which one `ISO8601DateFormatter`
/// configuration can't read.
public struct FlexibleDateTranscoder: DateTranscoder, @unchecked Sendable {
    private let lock = NSLock()
    private let plain: ISO8601DateFormatter
    private let fractional: ISO8601DateFormatter

    public init() {
        plain = ISO8601DateFormatter()
        fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    }

    public func encode(_ date: Date) throws -> String {
        lock.lock()
        defer { lock.unlock() }
        return plain.string(from: date)
    }

    public func decode(_ string: String) throws -> Date {
        lock.lock()
        defer { lock.unlock() }
        if let d = plain.date(from: string) { return d }
        if let d = fractional.date(from: Self.millis(string)) { return d }
        throw DecodingError.dataCorrupted(
            .init(codingPath: [], debugDescription: "Expected an ISO 8601 date, got \(string)")
        )
    }

    /// Cuts a fraction to three digits (`.123456789Z` → `.123Z`).
    static func millis(_ s: String) -> String {
        guard let dot = s.lastIndex(of: "."), let t = s.firstIndex(of: "T"), dot > t else { return s }
        let afterDot = s.index(after: dot)
        let end = s[afterDot...].firstIndex(where: { !$0.isNumber }) ?? s.endIndex
        let digits = s[afterDot..<end]
        guard digits.count > 3 else { return s }
        return String(s[..<afterDot]) + digits.prefix(3) + s[end...]
    }

    /// The same rules for a plain `JSONDecoder` / `JSONEncoder` (events, widget cache).
    public static let shared = FlexibleDateTranscoder()

    public static func decoder() -> JSONDecoder {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .custom { decoder in
            let s = try decoder.singleValueContainer().decode(String.self)
            return try shared.decode(s)
        }
        return d
    }

    public static func encoder() -> JSONEncoder {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .custom { date, encoder in
            var c = encoder.singleValueContainer()
            try c.encode(try shared.encode(date))
        }
        return e
    }
}
