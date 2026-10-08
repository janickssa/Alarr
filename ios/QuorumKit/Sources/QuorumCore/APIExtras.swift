import Foundation
import OpenAPIRuntime
import QuorumAPI

public typealias S = Components.Schemas

/// Module-specific snapshot data with readable case names (the generated `ModuleData` is an
/// untagged `oneOf`, so its cases are numbered in spec order).
public enum Module: Sendable {
    case none
    case proxmox(S.ProxmoxData)
    case healthCheck(S.HealthCheckData)
    case jellyfin(S.JellyfinData)
    case seerr(S.SeerrData)
    case arr(S.ArrData)
    case prowlarr(S.ProwlarrData)
    case bazarr(S.BazarrData)
    case qbittorrent(S.QbittorrentData)
    case navidrome(S.NavidromeData)
    case technitium(S.TechnitiumData)
    case npm(S.NpmData)
    case authentik(S.AuthentikData)
    case nextcloud(S.NextcloudData)
    case wireguard(S.WireguardData)
}

public extension S.ModuleData {
    var module: Module {
        switch self {
        case .case1: .none
        case .case2(let p): .proxmox(p.value)
        case .case3(let p): .healthCheck(p.value)
        case .case4(let p): .jellyfin(p.value)
        case .case5(let p): .seerr(p.value)
        case .case6(let p): .arr(p.value)
        case .case7(let p): .prowlarr(p.value)
        case .case8(let p): .bazarr(p.value)
        case .case9(let p): .qbittorrent(p.value)
        case .case10(let p): .navidrome(p.value)
        case .case11(let p): .technitium(p.value)
        case .case12(let p): .npm(p.value)
        case .case13(let p): .authentik(p.value)
        case .case14(let p): .nextcloud(p.value)
        case .case15(let p): .wireguard(p.value)
        }
    }
}

/// An API call that didn't answer 2xx, with the server's message when it sent one.
public struct APIFailure: LocalizedError, Sendable {
    public var message: String
    public var errorDescription: String? { message }

    public init(_ message: String) {
        self.message = message
    }

    /// Finds an `ErrorBody` anywhere in a generated `Output` value (each operation has its
    /// own error cases, all carrying `ErrorBody` JSON).
    public init(output: Any) {
        if let body = Self.find(in: output, depth: 0) {
            message = body.message
        } else if let code = Self.statusCode(in: output) {
            message = "The server answered HTTP \(code)"
        } else {
            message = "Unexpected answer from the server"
        }
    }

    static func find(in value: Any, depth: Int) -> S.ErrorBody? {
        if let b = value as? S.ErrorBody { return b }
        guard depth < 6 else { return nil }
        for child in Mirror(reflecting: value).children {
            if let b = find(in: child.value, depth: depth + 1) { return b }
        }
        return nil
    }

    static func statusCode(in value: Any) -> Int? {
        for child in Mirror(reflecting: value).children {
            if child.label == "statusCode", let c = child.value as? Int { return c }
            if let c = statusCode(in: child.value) { return c }
        }
        return nil
    }
}

/// Readable messages for transport errors.
public func describe(_ error: Error) -> String {
    if let f = error as? APIFailure { return f.message }
    if let c = error as? ClientError {
        if let u = c.underlyingError as? URLError { return describe(u) }
        if let f = c.underlyingError as? APIFailure { return f.message }
        return c.underlyingError.localizedDescription
    }
    if let u = error as? URLError {
        switch u.code {
        case .notConnectedToInternet, .cannotFindHost, .cannotConnectToHost, .timedOut, .networkConnectionLost:
            return "Can't reach Quorum. Are you on Tailscale (or your VPN)?"
        case .secureConnectionFailed, .serverCertificateUntrusted, .serverCertificateHasBadDate:
            return "The server's HTTPS certificate isn't trusted."
        case .appTransportSecurityRequiresSecureConnection:
            return "Use an https:// address (iOS blocks plain HTTP)."
        default:
            return u.localizedDescription
        }
    }
    return error.localizedDescription
}

public extension S.ActionRequest.ArgsPayload {
    /// `["id": 42]`, `["hashes": ["abc"]]` …
    static func from(_ values: [String: any Sendable]) -> Self {
        var props: [String: OpenAPIValueContainer] = [:]
        for (k, v) in values {
            if let c = try? OpenAPIValueContainer(unvalidatedValue: v) { props[k] = c }
        }
        return .init(additionalProperties: props)
    }
}

public extension S.ServiceView {
    func setting(_ key: String) -> String? {
        settings.additionalProperties[key]?.value as? String
    }
}

public extension S.Health {
    /// Problems first.
    var rank: Int {
        switch rawValue {
        case "down": 0
        case "warning": 1
        case "in_progress": 2
        case "unknown": 3
        case "healthy": 4
        default: 5
        }
    }
}
