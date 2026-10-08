import QuorumAPI
import QuorumCore
import SwiftUI
import UIKit
import UserNotifications
import WidgetKit

struct MoreView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        List {
            Section {
                NavigationLink(value: Route.health) { Label("Health", systemImage: "waveform.path.ecg") }
                NavigationLink(value: Route.alerts) { Label("Alerts", systemImage: "bell") }
                NavigationLink(value: Route.activity) { Label("Activity", systemImage: "list.bullet.rectangle") }
                NavigationLink(value: Route.settings) { Label("Settings", systemImage: "gearshape") }
            }
            Section("Account") {
                if let me = model.me {
                    KeyValueRow(key: "Signed in as", value: "\(me.user.username) (\(me.user.role.rawValue))")
                }
                if let c = model.credentials {
                    KeyValueRow(key: "Server", value: c.server.host() ?? c.server.absoluteString, mono: true)
                    KeyValueRow(key: "This iPhone", value: c.deviceName)
                }
                Button("Sign out and unpair", role: .destructive) {
                    model.confirm = ConfirmRequest(
                        title: "Unpair this iPhone?",
                        message: "The device token is revoked. Pair again with a new code from Settings → Users on the web.",
                        confirmLabel: "Unpair", destructive: true
                    ) { _ in Task { await model.signOut() } }
                }
            }
        }
        .quorumBackground()
        .navigationTitle("More")
    }
}

struct HealthView: View {
    @EnvironmentObject var model: AppModel
    @StateObject private var data = Loader<S.HealthReport>()
    @State private var problemsOnly = false

    var body: some View {
        List {
            Toggle("Only problems", isOn: $problemsOnly).listRowBackground(Color.clear)
            if let r = data.value {
                let entries = r.entries
                    .filter { !problemsOnly || ["down", "warning"].contains($0.health.rawValue) }
                    .sorted { ($0.health.rank, $0.name) < ($1.health.rank, $1.name) }
                ForEach(entries, id: \.id) { e in
                    Button {
                        if let s = e.serviceId { model.push(.service(s)) } else if let v = e.vmid { model.push(.guest(v)) }
                    } label: {
                        HealthRow(entry: e)
                    }
                    .buttonStyle(.plain)
                }
            } else if let e = data.error {
                StateView(kind: .error(e, retry: { Task { await load() } })).listRowBackground(Color.clear)
            } else {
                StateView(kind: .loading).listRowBackground(Color.clear)
            }
        }
        .quorumBackground()
        .navigationTitle("Health")
        .refreshable { await load() }
        .task(id: model.tick) { await load() }
    }

    func load() async {
        guard let client = model.client else { return }
        await data.load { try await client.health().ok.body.json }
    }
}

struct HealthRow: View {
    let entry: S.HealthEntry
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                HealthDot(health: entry.health.rawValue)
                Text(entry.name).font(.callout.weight(.medium))
                if let v = entry.vmid { Text("\(v)").font(.caption.monospaced()).foregroundStyle(Theme.muted) }
                Spacer()
                Text(Format.uptime(entry.uptime24h)).font(.caption.monospacedDigit()).foregroundStyle(Theme.muted)
            }
            // 24 h timeline, one bar per bucket.
            HStack(spacing: 1) {
                ForEach(Array(entry.timeline.enumerated()), id: \.offset) { _, b in
                    Rectangle().fill(Theme.health(b.health.rawValue).fg.opacity(b.health.rawValue == "unknown" ? 0.25 : 1))
                }
            }
            .frame(height: 10)
            .clipShape(RoundedRectangle(cornerRadius: 2))
            if let r = entry.healthReason { Text(r).font(.caption).foregroundStyle(Theme.muted) }
            if let b = entry.backup, !b.ok {
                Text("Last backup failed \(Format.dateTime(b.at))").font(.caption).foregroundStyle(Theme.down)
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }
}

struct ActivityView: View {
    @EnvironmentObject var model: AppModel
    @State private var entries: [S.ActivityEntry] = []
    @State private var error: String?
    @State private var end = false

    var body: some View {
        List {
            ForEach(entries, id: \.id) { e in
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: e.result.rawValue == "ok" ? "checkmark.circle" : e.result.rawValue == "pending" ? "clock" : "xmark.octagon")
                        .foregroundStyle(e.result.rawValue == "ok" ? Theme.healthy : e.result.rawValue == "pending" ? Theme.progress : Theme.down)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(e.action).font(.callout.monospaced())
                        Text([e.username ?? "system", e.target, e.detail].compactMap { $0 }.joined(separator: " · "))
                            .font(.caption).foregroundStyle(Theme.muted).lineLimit(3)
                    }
                    Spacer()
                    Text(Format.dateTime(e.at)).font(.caption2.monospacedDigit()).foregroundStyle(Theme.muted)
                }
                .onAppear { if e.id == entries.last?.id { Task { await more() } } }
            }
            if let error { Text(error).foregroundStyle(Theme.down) }
        }
        .quorumBackground()
        .navigationTitle("Activity")
        .refreshable { await reload() }
        .task(id: model.tick) { await reload() }
    }

    func reload() async {
        guard let client = model.client else { return }
        do {
            entries = try await client.listActivity(query: .init(limit: 50)).ok.body.json
            end = entries.count < 50
            error = nil
        } catch {
            self.error = describe(error)
        }
    }

    func more() async {
        guard !end, let client = model.client, let last = entries.last else { return }
        if let page = try? await client.listActivity(query: .init(limit: 50, before: last.id)).ok.body.json {
            entries += page
            end = page.count < 50
        }
    }
}

struct AlertsView: View {
    @EnvironmentObject var model: AppModel
    @StateObject private var data = Loader<[S.Alert]>()

    var body: some View {
        List {
            if let list = data.value {
                let active = list.filter { $0.resolvedAt == nil }
                let past = list.filter { $0.resolvedAt != nil }
                Section("Active") {
                    if active.isEmpty { Text("No active alerts").foregroundStyle(Theme.muted) }
                    ForEach(active, id: \.id) { AlertRow(alert: $0) }
                }
                if !past.isEmpty {
                    Section("Resolved") { ForEach(past.prefix(50), id: \.id) { AlertRow(alert: $0) } }
                }
            } else if let e = data.error {
                StateView(kind: .error(e, retry: { Task { await load() } })).listRowBackground(Color.clear)
            } else {
                StateView(kind: .loading).listRowBackground(Color.clear)
            }
        }
        .quorumBackground()
        .navigationTitle("Alerts")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: Route.notifications) { Image(systemName: "bell.badge") }
                    .accessibilityLabel("Notification settings")
            }
        }
        .refreshable { await load() }
        .task(id: model.tick) { await load() }
    }

    func load() async {
        guard let client = model.client else { return }
        await data.load { try await client.listAlerts(query: .init(limit: 100)).ok.body.json }
    }
}

struct AlertRow: View {
    @EnvironmentObject var model: AppModel
    let alert: S.Alert
    var body: some View {
        Button {
            if let s = alert.serviceId { model.push(.service(s)) }
        } label: {
            HStack(alignment: .top, spacing: 10) {
                HealthDot(health: alert.resolvedAt != nil ? "healthy" : alert.severity == "critical" ? "down" : alert.severity == "warning" ? "warning" : "in_progress")
                    .padding(.top, 5)
                VStack(alignment: .leading, spacing: 2) {
                    Text(alert.title).font(.callout.weight(.medium))
                    Text(alert.message).font(.caption).foregroundStyle(Theme.muted)
                    Text(alert.delivery.map { "\($0.channel): \($0.result.replacingOccurrences(of: "_", with: " "))" }.joined(separator: " · "))
                        .font(.caption2).foregroundStyle(Theme.muted)
                }
                Spacer()
                Text(Format.dateTime(alert.createdAt)).font(.caption2.monospacedDigit()).foregroundStyle(Theme.muted)
            }
        }
        .buttonStyle(.plain)
    }
}

struct SettingsView: View {
    @EnvironmentObject var model: AppModel
    @State private var info: S.SettingsResponse?
    @AppStorage("widget.service", store: AppGroup.defaults) private var widgetService = ""

    var body: some View {
        List {
            Section("This iPhone") {
                NavigationLink(value: Route.notifications) { Label("Notifications", systemImage: "bell.badge") }
                Picker("Service widget shows", selection: $widgetService) {
                    Text("First on Home").tag("")
                    ForEach(model.services.filter(\.enabled), id: \.id) { Text($0.name).tag($0.id) }
                }
                .onChange(of: widgetService) { _, _ in WidgetCenter.shared.reloadAllTimelines() }
            }
            if model.isOwner {
                Section("Server") {
                    NavigationLink(value: Route.manageServices) { Label("Services", systemImage: "square.grid.2x2") }
                    NavigationLink(value: Route.devices) { Label("Paired devices", systemImage: "iphone") }
                }
            }
            if let b = info?.backend {
                Section("About") {
                    KeyValueRow(key: b.name, value: b.version, mono: true)
                    KeyValueRow(key: "Running since", value: Format.dateTime(b.startedAt))
                    KeyValueRow(key: "Pollers", value: "\(b.pollers)")
                    if b.demo { Text("Demo mode: fake data").foregroundStyle(Theme.warning) }
                    KeyValueRow(key: "App", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "–", mono: true)
                }
            }
        }
        .quorumBackground()
        .navigationTitle("Settings")
        .task {
            guard let client = model.client else { return }
            info = try? await client.getSettings().ok.body.json
        }
    }
}

/// Per alert type: ntfy / push / both and quiet hours (shared with the web, owner only),
/// plus mutes that apply only to this iPhone.
struct NotificationSettingsView: View {
    @EnvironmentObject var model: AppModel
    @State private var view: S.AlertSettingsView?
    @State private var draft: [S.TypeSettings] = []
    @State private var mutes: Set<String> = []
    @State private var permission: UNAuthorizationStatus = .notDetermined
    @State private var error: String?
    @State private var saving = false

    var body: some View {
        Form {
            Section {
                switch permission {
                case .authorized, .provisional, .ephemeral:
                    Label("Notifications are on", systemImage: "checkmark.circle").foregroundStyle(Theme.healthy)
                    Button("Turn off push for this iPhone") { Task { await model.disablePush() } }
                case .denied:
                    Text("Notifications are off for Quorum in iOS Settings.").foregroundStyle(Theme.warning)
                    Button("Open iOS Settings") {
                        if let u = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(u) }
                    }
                default:
                    Button("Turn on notifications") { Task { await model.requestPush(); await refreshPermission() } }
                }
                if let e = model.pushError { Text(e).font(.caption).foregroundStyle(Theme.down) }
            } header: { Text("This iPhone") }

            if let v = view {
                Section {
                    ForEach(v.types, id: \.value1.alertType) { t in
                        Toggle(isOn: Binding(
                            get: { !mutes.contains(t.value1.alertType.rawValue) },
                            set: { on in Task { await setMute(t.value1.alertType, muted: !on) } }
                        )) {
                            VStack(alignment: .leading) {
                                Text(t.value2.label)
                                Text(t.value2.description).font(.caption).foregroundStyle(Theme.muted)
                            }
                        }
                    }
                } header: { Text("Show on this iPhone") }
                  footer: { Text("Muting here only affects this iPhone (push and in-app). ntfy topics are shared.") }

                if model.isOwner {
                    ForEach($draft, id: \.alertType) { $t in
                        Section(v.types.first { $0.value1.alertType == t.alertType }?.value2.label ?? t.alertType.rawValue) {
                            Toggle("Enabled", isOn: $t.enabled)
                            ForEach(v.channels, id: \.id) { c in
                                Toggle(isOn: Binding(
                                    get: { t.channels.contains(c.id) },
                                    set: { on in t.channels = on ? (t.channels + [c.id]).sorted() : t.channels.filter { $0 != c.id } }
                                )) {
                                    VStack(alignment: .leading) {
                                        Text(c.label)
                                        if !c.configured { Text(c.detail).font(.caption).foregroundStyle(Theme.warning) }
                                    }
                                }
                            }
                            Toggle("Quiet hours", isOn: Binding(
                                get: { t.quietHours != nil },
                                set: { t.quietHours = $0 ? .init(end: "07:00", start: "22:00") : nil }
                            ))
                            if t.quietHours != nil {
                                TimeField(label: "From", value: Binding(get: { t.quietHours?.start ?? "22:00" }, set: { t.quietHours?.start = $0 }))
                                TimeField(label: "To", value: Binding(get: { t.quietHours?.end ?? "07:00" }, set: { t.quietHours?.end = $0 }))
                            }
                        }
                    }
                    Section {
                        Button(saving ? "Saving…" : "Save") { Task { await save() } }.disabled(saving)
                    } footer: {
                        Text("Quiet hours use the server's time zone (\(v.timezone)). Alerts during quiet hours are recorded but not sent.")
                    }
                }
            }
            if let error { Text(error).foregroundStyle(Theme.down) }
        }
        .quorumBackground()
        .navigationTitle("Notifications")
        .navigationBarTitleDisplayMode(.inline)
        .task { await load() }
    }

    func refreshPermission() async {
        permission = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
    }

    func load() async {
        await refreshPermission()
        guard let client = model.client else { return }
        do {
            let v = try await client.getAlertSettings().ok.body.json
            view = v
            draft = v.types.map(\.value1)
            mutes = Set(try await client.getAlertMutes().ok.body.json.types.map(\.rawValue))
        } catch {
            self.error = describe(error)
        }
    }

    func setMute(_ t: S.AlertType, muted: Bool) async {
        guard let client = model.client else { return }
        var next = mutes
        if muted { next.insert(t.rawValue) } else { next.remove(t.rawValue) }
        do {
            let types = next.compactMap(S.AlertType.init(rawValue:))
            let out = try await client.putAlertMutes(body: .json(.init(types: types)))
            mutes = Set(try out.ok.body.json.types.map(\.rawValue))
        } catch {
            self.error = describe(error)
        }
    }

    func save() async {
        guard let client = model.client else { return }
        saving = true
        defer { saving = false }
        do {
            let out = try await client.putAlertSettings(body: .json(.init(types: draft)))
            guard case .ok(let ok) = out else { throw APIFailure(output: out) }
            let v = try ok.body.json
            view = v
            draft = v.types.map(\.value1)
            model.toasts.show("Notification settings saved", tone: .success)
        } catch {
            self.error = describe(error)
        }
    }
}

/// `HH:mm` (24-hour) picker bound to a string.
struct TimeField: View {
    let label: String
    @Binding var value: String

    var body: some View {
        DatePicker(label, selection: Binding(
            get: {
                let f = DateFormatter(); f.dateFormat = "HH:mm"
                return f.date(from: value) ?? Date()
            },
            set: {
                let f = DateFormatter(); f.dateFormat = "HH:mm"
                value = f.string(from: $0)
            }
        ), displayedComponents: .hourAndMinute)
        .environment(\.locale, Locale(identifier: "en_GB"))
    }
}

struct DevicesView: View {
    @EnvironmentObject var model: AppModel
    @State private var devices: [S.Device] = []

    var body: some View {
        List {
            ForEach(devices, id: \.id) { d in
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text(d.name).font(.callout.weight(.medium))
                        if d.id == model.credentials?.deviceID { Text("this iPhone").font(.caption).foregroundStyle(Theme.accent) }
                    }
                    Text("\(d.platform)\(d.push ? " · push (\(d.pushEnv ?? ""))" : "") · last seen \(Format.ago(d.lastSeenAt))")
                        .font(.caption).foregroundStyle(Theme.muted)
                }
                .swipeActions {
                    if d.id != model.credentials?.deviceID {
                        Button(role: .destructive) { Task { await revoke(d) } } label: { Label("Revoke", systemImage: "xmark") }
                    }
                }
            }
        }
        .quorumBackground()
        .navigationTitle("Paired devices")
        .task { await load() }
    }

    func load() async {
        guard let client = model.client else { return }
        devices = (try? await client.listDevices().ok.body.json) ?? []
    }

    func revoke(_ d: S.Device) async {
        guard let client = model.client else { return }
        _ = try? await client.deleteDevice(path: .init(id: d.id))
        model.toasts.show("\(d.name) revoked", tone: .success)
        await load()
    }
}
