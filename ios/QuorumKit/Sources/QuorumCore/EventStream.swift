import Foundation
import QuorumAPI

/// One server-sent event from `GET /api/v1/events`.
public struct ServerEvent: Sendable, Equatable {
    /// `snapshot`, `health`, `action` or `alert`.
    public var name: String
    /// The JSON payload.
    public var data: Data

    public init(name: String, data: Data) {
        self.name = name
        self.data = data
    }

    private struct Ref: Decodable {
        var service_id: String?
    }

    /// The service the event is about, when it names one.
    public var serviceID: String? {
        (try? JSONDecoder().decode(Ref.self, from: data))?.service_id
    }
}

/// Parses `text/event-stream` lines. The server writes one `data:` line per event (JSON on
/// one line), so an event is complete at its `data:` line; this also copes with line
/// readers that drop the blank separator lines.
public struct EventParser: Sendable {
    private var name = "message"

    public init() {}

    public mutating func feed(_ line: String) -> ServerEvent? {
        if line.isEmpty || line.hasPrefix(":") { return nil }
        let (field, value) = Self.split(line)
        switch field {
        case "event":
            name = value
            return nil
        case "data":
            defer { name = "message" }
            return ServerEvent(name: name, data: Data(value.utf8))
        default:
            return nil
        }
    }

    static func split(_ line: String) -> (String, String) {
        guard let colon = line.firstIndex(of: ":") else { return (line, "") }
        var value = line[line.index(after: colon)...]
        if value.first == " " { value = value.dropFirst() }
        return (String(line[..<colon]), String(value))
    }
}

#if canImport(Darwin)
/// The live connection: reconnects with a growing delay (1 s → 30 s) until cancelled.
public final class EventStream: @unchecked Sendable {
    private let credentials: Credentials
    private let session: URLSession

    public init(credentials: Credentials) {
        self.credentials = credentials
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 120
        config.timeoutIntervalForResource = .infinity
        session = URLSession(configuration: config)
    }

    /// `watch`: service ids to keep in fast polling while they are on screen.
    public func events(watch: [String] = []) -> AsyncStream<ServerEvent> {
        let credentials = credentials
        let session = session
        return AsyncStream { continuation in
            let task = Task {
                var delay: UInt64 = 1
                while !Task.isCancelled {
                    do {
                        var url = QuorumServer.apiURL(credentials.server).appending(path: "events")
                        if !watch.isEmpty {
                            url.append(queryItems: [URLQueryItem(name: "watch", value: watch.joined(separator: ","))])
                        }
                        var req = URLRequest(url: url)
                        req.setValue("Bearer \(credentials.token)", forHTTPHeaderField: "Authorization")
                        req.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                        let (bytes, response) = try await session.bytes(for: req)
                        guard (response as? HTTPURLResponse)?.statusCode == 200 else {
                            throw URLError(.badServerResponse)
                        }
                        delay = 1
                        continuation.yield(ServerEvent(name: "open", data: Data()))
                        var parser = EventParser()
                        for try await line in bytes.lines {
                            if let e = parser.feed(line) { continuation.yield(e) }
                        }
                    } catch is CancellationError {
                        break
                    } catch {
                        continuation.yield(ServerEvent(name: "error", data: Data()))
                    }
                    try? await Task.sleep(nanoseconds: delay * 1_000_000_000)
                    delay = min(delay * 2, 30)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
#endif
