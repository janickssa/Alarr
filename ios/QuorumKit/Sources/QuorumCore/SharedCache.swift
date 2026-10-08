import Foundation
import QuorumAPI

/// Small JSON files in the App Group container, written by the app and read by the widgets
/// (and by the app itself for an instant first screen).
public enum SharedCache {
    public static func url(_ name: String) -> URL? {
        AppGroup.container?.appending(path: "\(name).json")
    }

    public static func save<T: Encodable>(_ value: T, as name: String) {
        guard let url = url(name),
              let data = try? FlexibleDateTranscoder.encoder().encode(value)
        else { return }
        try? data.write(to: url, options: .atomic)
    }

    public static func load<T: Decodable>(_ type: T.Type, from name: String) -> T? {
        guard let url = url(name), let data = try? Data(contentsOf: url) else { return nil }
        return try? FlexibleDateTranscoder.decoder().decode(type, from: data)
    }

    public static func remove(_ name: String) {
        guard let url = url(name) else { return }
        try? FileManager.default.removeItem(at: url)
    }

    /// File names.
    public static let widget = "widget"
}
