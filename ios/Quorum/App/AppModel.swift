import Foundation
import QuorumAPI
import QuorumCore
import SwiftUI
import UIKit
import UserNotifications
import WidgetKit

/// Why the app can't show your data, on the "Can't connect" screen.
struct Problem: Equatable {
    var title: String
    var reason: String
}

enum Tab: Hashable {
    case home, proxmox, media, services, more
}

/// Pushed pages inside a tab.
enum Route: Hashable {
    case service(String)
    case guest(Int32)
    case node(String)
    case health
    case activity
    case alerts
    case settings
    case notifications
    case devices
    case manageServices
    case addService
}

/// App state: pairing, the API client, the service list, live updates, navigation and
/// feedback (toasts, confirmations).
@MainActor
final class AppModel: ObservableObject {
    @Published var credentials: Credentials?
    @Published var me: S.Me?
    @Published var services: [S.ServiceView] = []
    /// Why the app can't load right now ("Can't connect", "Sign-in not accepted"). The
    /// pairing stays in the Keychain: only Log out removes it.
    @Published var problem: Problem?
    @Published var connected = false
    /// Bumped (at most every 2 s) when live data changed; screens reload on it.
    @Published var tick = 0
    @Published var tab: Tab = .home
    @Published var paths: [Tab: [Route]] = [:]
    @Published var confirm: ConfirmRequest?
    @Published var pushError: String?
    @Published var pendingPairing: PairingLink?

    let toasts = Toasts()
    private(set) var client: Client?
    private var events: Task<Void, Never>?
    private var retry: Task<Void, Never>?
    private var lastTick = Date.distantPast
    private var tickScheduled = false
    /// Activity entries we started (power actions): their result updates the toast.
    private var pending: [Int64: (toast: UUID, label: String, started: Date)] = [:]

    init() {
        if let c = Keychain.load() { use(c) }
        AppDelegate.model = self
    }

    var isOwner: Bool { me?.user.role.rawValue == "owner" }

    var hasProxmox: Bool { services.contains { $0.enabled && $0.kind.rawValue == "proxmox" } }

    var hasMedia: Bool {
        let media = ["jellyfin", "seerr", "sonarr", "radarr", "prowlarr", "bazarr", "qbittorrent", "navidrome"]
        return services.contains { $0.enabled && media.contains($0.kind.rawValue) }
    }

    func service(_ id: String) -> S.ServiceView? { services.first { $0.id == id } }

    // MARK: - Pairing and session

    private func use(_ c: Credentials) {
        credentials = c
        client = QuorumServer.client(server: c.server, token: c.token)
    }

    func pair(server: URL, code: String, deviceName: String) async throws {
        let pairing = QuorumServer.client(server: server, token: nil)
        let out = try await pairing.complete(body: .json(.init(code: code, deviceName: deviceName, platform: "ios")))
        guard case .ok(let ok) = out else { throw APIFailure(output: out) }
        let done = try ok.body.json
        guard let token = done.token else { throw APIFailure("The server didn't return a token") }
        let c = Credentials(server: server, token: token, deviceID: done.device.id, deviceName: done.device.name)
        Keychain.save(c)
        use(c)
        await start()
        await requestPush()
    }

    /// Load the account and services and open the live connection. Never unpairs by
    /// itself: when the server can't be reached or refuses the token, the app shows why and
    /// keeps retrying until it works or you tap Log out.
    func start() async {
        guard let client else { return }
        let host = credentials?.server.host() ?? "the server"
        do {
            let out = try await client.me()
            switch out {
            case .ok(let ok):
                me = try ok.body.json
                problem = nil
                retry?.cancel()
            case .unauthorized:
                show(Problem(
                    title: "Sign-in not accepted",
                    reason: "\(host) didn't accept this iPhone's pairing. It may have been removed on the web, or the server's database was reset. Log out and pair again to continue."
                ), after: 15)
                return
            default:
                let msg = APIFailure(output: out).message
                let proxy = ["502", "503", "504"].contains { msg.contains("HTTP \($0)") }
                show(Problem(
                    title: "Can't connect",
                    reason: proxy
                        ? "The proxy in front of \(host) can't reach Quorum. Is the Quorum container running?"
                        : "\(host): \(msg)."
                ), after: 5)
                return
            }
        } catch {
            show(Problem(
                title: "Can't connect",
                reason: "No answer from \(host). Usually the VPN (WireGuard) isn't connected yet, or the server is down."
            ), after: 5)
            return
        }
        await loadServices()
        await refreshWidget()
        connect()
        UNUserNotificationCenter.current().getNotificationSettings { settings in
            if settings.authorizationStatus == .authorized {
                Task { @MainActor in UIApplication.shared.registerForRemoteNotifications() }
            }
        }
    }

    func loadServices() async {
        guard let client else { return }
        if let list = try? await client.listServices().ok.body.json {
            services = list
        }
    }

    /// Show why loading failed and try again after a few seconds, while the app is open.
    private func show(_ p: Problem, after seconds: Double) {
        problem = p
        // Keep showing loaded data on a short blip; the problem screen is for a cold start
        // or a refused token.
        if p.title != "Can't connect" { me = nil }
        retry?.cancel()
        retry = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1e9))
            guard !Task.isCancelled, let self, self.problem != nil,
                  UIApplication.shared.applicationState == .active else { return }
            await self.start()
        }
    }

    /// The only way the app forgets its pairing: you tap Log out.
    func signOut() async {
        if let client { _ = try? await client.logout() }
        signOutLocally()
    }

    private func signOutLocally() {
        events?.cancel()
        retry?.cancel()
        problem = nil
        Keychain.delete()
        SharedCache.remove(SharedCache.widget)
        WidgetCenter.shared.reloadAllTimelines()
        credentials = nil
        client = nil
        me = nil
        services = []
        paths = [:]
        tab = .home
    }

    // MARK: - Live updates

    func connect() {
        guard let credentials else { return }
        events?.cancel()
        let stream = EventStream(credentials: credentials)
        events = Task { [weak self] in
            for await e in stream.events() {
                guard let self else { return }
                self.handle(e)
            }
        }
    }

    func disconnect() {
        events?.cancel()
        events = nil
        connected = false
    }

    private func handle(_ e: ServerEvent) {
        switch e.name {
        case "open":
            connected = true
            bump()
        case "error":
            connected = false
        case "action":
            if let ev = try? FlexibleDateTranscoder.decoder().decode(S.ActionEvent.self, from: e.data) {
                finished(ev.entry)
            }
            bump()
        case "health":
            Task { await loadServices() }
            bump()
        default:
            bump()
        }
    }

    /// Coalesce bursts of events into one reload every 2 s.
    private func bump() {
        let wait = 2 - Date().timeIntervalSince(lastTick)
        if wait <= 0 {
            lastTick = Date()
            tick += 1
        } else if !tickScheduled {
            tickScheduled = true
            Task {
                try? await Task.sleep(nanoseconds: UInt64(wait * 1e9))
                tickScheduled = false
                lastTick = Date()
                tick += 1
            }
        }
    }

    /// Pull to refresh.
    func refresh() async {
        await loadServices()
        tick += 1
        await refreshWidget()
    }

    func refreshWidget() async {
        guard let client, let w = try? await client.widget().ok.body.json else { return }
        SharedCache.save(w, as: SharedCache.widget)
        WidgetCenter.shared.reloadAllTimelines()
    }

    // MARK: - Actions

    /// Proxmox power. Start runs at once; shutdown and reboot ask; stop asks with the
    /// destructive sheet and offers "Shutdown instead".
    func power(_ guest: S.Guest, _ action: String) {
        let name = "\(guest.name) (\(guest.vmid))"
        let run: (String) -> Void = { a in Task { await self.runPower(vmid: guest.vmid, name: guest.name, action: a) } }
        switch action {
        case "start":
            run("start")
        case "shutdown":
            confirm = ConfirmRequest(
                title: "Shut down \(name)?",
                message: "The guest gets an ACPI shutdown and stops when its OS is done.",
                confirmLabel: "Shut down", destructive: false
            ) { _ in run("shutdown") }
        case "reboot":
            confirm = ConfirmRequest(
                title: "Reboot \(name)?",
                message: "Services on it are unavailable until it is back.",
                confirmLabel: "Reboot", destructive: false
            ) { _ in run("reboot") }
        case "stop":
            confirm = ConfirmRequest(
                title: "Stop \(name) now?",
                message: "Like pulling the plug: unsaved data in the guest can be lost.",
                confirmLabel: "Stop now", destructive: true,
                alternativeLabel: "Shutdown instead", alternative: { run("shutdown") }
            ) { _ in run("stop") }
        default:
            break
        }
    }

    private func runPower(vmid: Int32, name: String, action: String) async {
        guard let client, let a = S.PowerAction(rawValue: action) else { return }
        let verb = ["start": "Starting", "shutdown": "Shutting down", "stop": "Stopping", "reboot": "Rebooting"][action] ?? action
        let toast = toasts.show("\(verb) \(name)…", tone: .progress)
        do {
            let out = try await client.power(path: .init(vmid: vmid), body: .json(.init(action: a, confirm: action != "start")))
            guard case .ok(let ok) = out else { throw APIFailure(output: out) }
            let started = try ok.body.json
            pending[started.activityId] = (toast, "\(name) \(Self.done[action] ?? action)", Date())
        } catch {
            toasts.update(toast, text: "\(verb) \(name) failed: \(describe(error))", tone: .error)
        }
    }

    static let done = ["start": "started", "shutdown": "shut down", "stop": "stopped", "reboot": "rebooted"]

    private func finished(_ entry: S.ActivityEntry) {
        guard let p = pending[entry.id], entry.result.rawValue != "pending" else { return }
        pending[entry.id] = nil
        let secs = Int(Date().timeIntervalSince(p.started))
        if entry.result.rawValue == "ok" {
            toasts.update(p.toast, text: "\(p.label) in \(secs) s", tone: .success)
        } else {
            toasts.update(p.toast, text: entry.detail ?? "Failed", tone: .error)
        }
    }

    /// A service action (`scan_all`, `approve` …). Returns true on success.
    @discardableResult
    func action(
        _ service: String, _ action: String, args: [String: any Sendable] = [:],
        confirm: Bool = false, label: String
    ) async -> Bool {
        guard let client else { return false }
        let toast = toasts.show("\(label)…", tone: .progress)
        do {
            let body = S.ActionRequest(args: args.isEmpty ? nil : .from(args), confirm: confirm ? true : nil)
            let out = try await client.runAction(path: .init(id: service, action: action), body: .json(body))
            guard case .ok(let ok) = out else { throw APIFailure(output: out) }
            let result = try ok.body.json
            toasts.update(toast, text: result.ok ? result.message : result.message, tone: result.ok ? .success : .error)
            bump()
            return result.ok
        } catch {
            toasts.update(toast, text: "\(label) failed: \(describe(error))", tone: .error)
            return false
        }
    }

    /// Approve / Decline from a notification.
    func answerRequest(service: String, request: Int64, approve: Bool) async {
        await action(service, approve ? "approve" : "decline", args: ["id": request], label: approve ? "Approving" : "Declining")
    }

    // MARK: - Push

    func requestPush() async {
        let granted = (try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge, .timeSensitive])) ?? false
        if granted { UIApplication.shared.registerForRemoteNotifications() }
    }

    func registerPushToken(_ hex: String) async {
        guard let client, let c = credentials else { return }
        let body = S.PushToken(environment: AppDelegate.apnsEnvironment, token: hex)
        do {
            let out = try await client.setPushToken(path: .init(id: c.deviceID), body: .json(body))
            guard case .noContent = out else { throw APIFailure(output: out) }
            pushError = nil
        } catch {
            pushError = describe(error)
        }
    }

    func disablePush() async {
        guard let client, let c = credentials else { return }
        _ = try? await client.deletePushToken(path: .init(id: c.deviceID))
        UIApplication.shared.unregisterForRemoteNotifications()
    }

    // MARK: - Navigation

    func push(_ route: Route, on tab: Tab? = nil) {
        let t = tab ?? self.tab
        self.tab = t
        paths[t, default: []].append(route)
    }

    /// Paths from notifications, widgets and links (`/services/sonarr`, `/proxmox`, …).
    func open(path: String) {
        let parts = path.split(separator: "/").map(String.init)
        switch parts.first {
        case nil: tab = .home; paths[.home] = []
        case "proxmox":
            tab = .proxmox
            if parts.count >= 3, parts[1] == "guests", let id = Int32(parts[2]) { paths[.proxmox] = [.guest(id)] } else { paths[.proxmox] = [] }
        case "media": tab = .media; paths[.media] = []
        case "services":
            tab = .services
            paths[.services] = parts.count > 1 ? [.service(parts[1])] : []
        case "alerts": tab = .more; paths[.more] = [.alerts]
        case "health": tab = .more; paths[.more] = [.health]
        case "activity": tab = .more; paths[.more] = [.activity]
        case "settings": tab = .more; paths[.more] = [.settings]
        default: tab = .home
        }
    }

    /// `quorum://pair?…` (QR scanned with the Camera app) and `quorum://open/<path>`.
    func handle(url: URL) {
        if let link = PairingLink(url.absoluteString) {
            pendingPairing = link
            return
        }
        guard url.scheme == "quorum", url.host == "open" else { return }
        open(path: url.path)
    }

    func binding(for tab: Tab) -> Binding<[Route]> {
        Binding(get: { self.paths[tab] ?? [] }, set: { self.paths[tab] = $0 })
    }
}
