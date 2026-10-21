import Foundation
#if canImport(Security)
import Security
#endif

/// What pairing leaves behind: the server and this device's token.
public struct Credentials: Codable, Equatable, Sendable {
    public var server: URL
    public var token: String
    public var deviceID: Int64
    public var deviceName: String

    public init(server: URL, token: String, deviceID: Int64, deviceName: String) {
        self.server = server
        self.token = token
        self.deviceID = deviceID
        self.deviceName = deviceName
    }
}

/// The App Group shared by the app and its widgets (Info.plist key `QuorumAppGroup`).
public enum AppGroup {
    public static var id: String? {
        Bundle.main.object(forInfoDictionaryKey: "QuorumAppGroup") as? String
    }

    public static var defaults: UserDefaults {
        id.flatMap(UserDefaults.init(suiteName:)) ?? .standard
    }

    public static var container: URL? {
        #if canImport(Darwin)
        id.flatMap { FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0) }
        #else
        nil
        #endif
    }
}

#if canImport(Security)
/// Credentials in the Keychain, readable by the widget extension through the App Group
/// (app groups are keychain access groups too) and available after the first unlock so
/// widgets can refresh while the phone is locked.
public enum Keychain {
    static let service = "quorum"
    static let account = "device"

    private static func query() -> [String: Any] {
        var q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        if let group = AppGroup.id {
            q[kSecAttrAccessGroup as String] = group
        }
        return q
    }

    public static func load() -> Credentials? {
        var q = query()
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess,
              let data = out as? Data
        else { return nil }
        return try? JSONDecoder().decode(Credentials.self, from: data)
    }

    @discardableResult
    public static func save(_ c: Credentials) -> Bool {
        guard let data = try? JSONEncoder().encode(c) else { return false }
        SecItemDelete(query() as CFDictionary)
        var q = query()
        q[kSecValueData as String] = data
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(q as CFDictionary, nil) == errSecSuccess
    }

    public static func delete() {
        SecItemDelete(query() as CFDictionary)
    }
}
#endif

/// `quorum://pair?server=<url>&code=<code>` from the desktop QR code.
public struct PairingLink: Equatable, Sendable {
    public var server: URL
    public var code: String

    public init?(_ text: String) {
        guard let c = URLComponents(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
              c.scheme == "quorum", c.host == "pair",
              let s = c.queryItems?.first(where: { $0.name == "server" })?.value,
              let server = URL(string: s), let scheme = server.scheme,
              ["http", "https"].contains(scheme.lowercased()), server.host != nil,
              let code = c.queryItems?.first(where: { $0.name == "code" })?.value,
              !code.isEmpty
        else { return nil }
        self.server = server
        self.code = code
    }

    /// A server typed by hand: adds `https://` when no scheme is given.
    public static func server(from typed: String) -> URL? {
        var t = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        while t.hasSuffix("/") { t.removeLast() }
        guard !t.isEmpty else { return nil }
        if !t.contains("://") { t = "https://" + t }
        guard let url = URL(string: t), let scheme = url.scheme?.lowercased(),
              ["http", "https"].contains(scheme), url.host != nil
        else { return nil }
        return url
    }
}
