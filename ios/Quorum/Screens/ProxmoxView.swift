import Charts
import QuorumAPI
import QuorumCore
import SwiftUI

struct ProxmoxView: View {
    @EnvironmentObject var model: AppModel
    @StateObject private var data = Loader<(S.ClusterOverview, [S.Guest])>()
    @State private var filter = "all"
    @State private var search = ""

    var body: some View {
        List {
            if case let (c, guests)? = data.value {
                Section {
                    HStack(spacing: 10) {
                        StatTile(title: "Nodes", value: "\(c.cluster.nodesOnline)/\(c.cluster.nodesTotal)",
                                 sub: c.cluster.quorate ? "Quorate" : "Not quorate",
                                 tint: c.cluster.quorate ? Theme.text : Theme.down)
                        StatTile(title: "Guests", value: "\(c.guests.running)/\(c.guests.total)", sub: "running")
                    }
                    .listRowInsets(EdgeInsets()).listRowBackground(Color.clear)
                }
                Section("Nodes") {
                    ForEach(c.nodes, id: \.name) { n in
                        NavigationLink(value: Route.node(n.name)) { NodeRow(node: n) }
                    }
                }
                if !c.storage.isEmpty {
                    Section("Storage") {
                        ForEach(c.storage, id: \.id) { s in
                            ResourceBar(
                                label: "\(s.storage) · \(s.node)",
                                fraction: ratio(s.used, s.total),
                                detail: "\(Format.bytes(s.used)) / \(Format.bytes(s.total))"
                            )
                            .padding(.vertical, 4)
                        }
                    }
                }
                Section {
                    Picker("Filter", selection: $filter) {
                        Text("All").tag("all")
                        Text("Running").tag("running")
                        Text("Stopped").tag("stopped")
                    }
                    .pickerStyle(.segmented)
                    .listRowBackground(Color.clear)
                    ForEach(filtered(guests), id: \.vmid) { g in
                        NavigationLink(value: Route.guest(g.vmid)) { GuestRow(guest: g) }
                            .swipeActions(edge: .trailing) { GuestActions(guest: g, swipe: true) }
                            .contextMenu { GuestActions(guest: g, swipe: false) }
                    }
                } header: {
                    Text("Guests")
                }
            } else if let e = data.error {
                StateView(kind: .error(e, retry: { Task { await load() } })).listRowBackground(Color.clear)
            } else {
                StateView(kind: .loading).listRowBackground(Color.clear)
            }
        }
        .listStyle(.insetGrouped)
        .quorumBackground()
        .navigationTitle("Proxmox")
        .searchable(text: $search, prompt: "Guests")
        .refreshable { await model.refresh() }
        .task(id: model.tick) { await load() }
    }

    func filtered(_ list: [S.Guest]) -> [S.Guest] {
        list.filter { g in
            !g.template
                && (filter == "all" || g.status.rawValue == filter)
                && (search.isEmpty || g.name.localizedCaseInsensitiveContains(search) || "\(g.vmid)".contains(search))
        }
        .sorted { $0.vmid < $1.vmid }
    }

    func load() async {
        guard let client = model.client else { return }
        await data.load {
            async let c = client.cluster().ok.body.json
            async let g = client.guests().ok.body.json
            return try await (c, g)
        }
    }
}

struct NodeRow: View {
    let node: S.Node
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                HealthDot(health: node.online ? "healthy" : "down")
                Text(node.name).font(.callout.monospaced())
                Spacer()
                if let t = node.temperatureC { Text(Format.celsius(t)).font(.caption).foregroundStyle(Theme.muted) }
                if !node.online { Text("Offline").font(.caption).foregroundStyle(Theme.down) }
            }
            if node.online {
                ResourceBar(label: "CPU", fraction: node.cpuUsage)
                ResourceBar(label: "RAM", fraction: ratio(node.memUsed, node.memTotal),
                            detail: "\(Format.bytes(node.memUsed)) / \(Format.bytes(node.memTotal))")
            }
        }
        .padding(.vertical, 4)
    }
}

struct GuestRow: View {
    let guest: S.Guest
    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: guest.guestType.rawValue == "qemu" ? "desktopcomputer" : "shippingbox")
                .foregroundStyle(Theme.muted).frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(guest.name).font(.callout.weight(.medium))
                Text("\(guest.vmid) · \(guest.node)").font(.caption.monospaced()).foregroundStyle(Theme.muted)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text(guest.status.rawValue.capitalized)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(statusColor(guest.status.rawValue))
                if guest.status.rawValue == "running" {
                    Text("CPU \(Format.percent(guest.cpuUsage)) · RAM \(Format.percent(ratio(guest.memUsed, guest.memTotal)))")
                        .font(.caption2.monospacedDigit()).foregroundStyle(Theme.muted)
                }
            }
        }
        .accessibilityElement(children: .combine)
    }
}

func statusColor(_ s: String) -> Color {
    switch s {
    case "running": Theme.healthy
    case "paused": Theme.warning
    default: Theme.neutral
    }
}

/// Power actions for a guest (swipe actions and context menu).
struct GuestActions: View {
    @EnvironmentObject var model: AppModel
    let guest: S.Guest
    let swipe: Bool

    var body: some View {
        if guest.status.rawValue == "running" {
            Button { model.power(guest, "shutdown") } label: { Label("Shut down", systemImage: "power") }
                .tint(Theme.warning)
            Button { model.power(guest, "reboot") } label: { Label("Reboot", systemImage: "arrow.clockwise") }
                .tint(Theme.accent)
            if !swipe {
                Button(role: .destructive) { model.power(guest, "stop") } label: { Label("Stop now", systemImage: "stop.fill") }
            }
        } else {
            Button { model.power(guest, "start") } label: { Label("Start", systemImage: "play.fill") }
                .tint(Theme.healthy)
        }
    }
}

struct GuestDetailView: View {
    @EnvironmentObject var model: AppModel
    let vmid: Int32
    @StateObject private var data = Loader<(S.GuestDetail, [S.RrdPoint])>()
    @State private var range = "hour"

    var body: some View {
        List {
            if case let (d, rrd)? = data.value {
                let g = d.guest
                Section {
                    HStack {
                        GuestRow(guest: g)
                    }
                    if g.status.rawValue == "running" {
                        ResourceBar(label: "CPU", fraction: g.cpuUsage)
                        ResourceBar(label: "Memory", fraction: ratio(g.memUsed, g.memTotal),
                                    detail: "\(Format.bytes(g.memUsed)) / \(Format.bytes(g.memTotal))")
                        if g.diskTotal != nil {
                            ResourceBar(label: "Disk", fraction: ratio(g.diskUsed, g.diskTotal),
                                        detail: "\(Format.bytes(g.diskUsed)) / \(Format.bytes(g.diskTotal))")
                        }
                        KeyValueRow(key: "Uptime", value: Format.duration(g.uptimeS))
                    }
                }
                Section {
                    HStack(spacing: 10) {
                        if g.status.rawValue == "running" {
                            powerButton("Shut down", "power", "shutdown", g)
                            powerButton("Reboot", "arrow.clockwise", "reboot", g)
                            powerButton("Stop", "stop.fill", "stop", g)
                        } else {
                            powerButton("Start", "play.fill", "start", g)
                        }
                    }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                }
                Section {
                    Picker("Range", selection: $range) {
                        Text("Hour").tag("hour"); Text("Day").tag("day"); Text("Week").tag("week")
                    }
                    .pickerStyle(.segmented)
                    Chart(rrd, id: \.time) { p in
                        if let c = p.cpu {
                            LineMark(x: .value("Time", p.time), y: .value("CPU", c * 100))
                                .foregroundStyle(Theme.accent)
                        }
                    }
                    .chartYScale(domain: 0...100)
                    .frame(height: 150)
                    .accessibilityLabel("CPU usage over the last \(range)")
                } header: { Text("CPU (%)") }
                if !d.linkedServices.isEmpty {
                    Section("Services") {
                        ForEach(d.linkedServices, id: \.id) { s in
                            NavigationLink(value: Route.service(s.id)) {
                                HStack { HealthDot(health: s.health.rawValue); Text(s.name) }
                            }
                        }
                    }
                }
                Section("Backup") {
                    if let b = d.backup {
                        KeyValueRow(key: b.ok ? "Last backup" : "Last backup failed", value: Format.dateTime(b.at))
                        if let e = b.error { Text(e).font(.caption).foregroundStyle(Theme.down) }
                    } else {
                        Text("No backup in the last 35 days").foregroundStyle(Theme.muted)
                    }
                }
                if !d.recentTasks.isEmpty {
                    Section("Recent tasks") {
                        ForEach(d.recentTasks, id: \.upid) { t in
                            HStack {
                                Text(t.kind ?? "task").font(.callout.monospaced())
                                Spacer()
                                Text(t.state.rawValue == "running" ? "running" : (t.exitStatus ?? t.state.rawValue))
                                    .font(.caption)
                                    .foregroundStyle(t.state.rawValue == "error" ? Theme.down : Theme.muted)
                                Text(Format.dateTime(t.startedAt)).font(.caption.monospacedDigit()).foregroundStyle(Theme.muted)
                            }
                        }
                    }
                }
                if !g.tags.isEmpty {
                    Section("Tags") { Text(g.tags.joined(separator: ", ")).font(.callout.monospaced()) }
                }
            } else if let e = data.error {
                StateView(kind: .error(e, retry: { Task { await load() } })).listRowBackground(Color.clear)
            } else {
                StateView(kind: .loading).listRowBackground(Color.clear)
            }
        }
        .quorumBackground()
        .navigationTitle(data.value?.0.guest.name ?? "\(vmid)")
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await load() }
        .task(id: "\(model.tick)-\(range)") { await load() }
    }

    func powerButton(_ title: String, _ icon: String, _ action: String, _ g: S.Guest) -> some View {
        Button { model.power(g, action) } label: {
            VStack(spacing: 4) {
                Image(systemName: icon)
                Text(title).font(.caption)
            }
            .frame(maxWidth: .infinity, minHeight: 56)
        }
        .buttonStyle(.bordered)
        .tint(action == "stop" ? Theme.down : Theme.accent)
    }

    func load() async {
        guard let client = model.client else { return }
        let r = Operations.GuestRrd.Input.Query.RangePayload(rawValue: range)
        await data.load {
            async let d = client.guest(path: .init(vmid: vmid)).ok.body.json
            async let rrd = client.guestRrd(path: .init(vmid: vmid), query: .init(range: r)).ok.body.json
            return try await (d, rrd)
        }
    }
}

struct NodeDetailView: View {
    @EnvironmentObject var model: AppModel
    let name: String
    @StateObject private var data = Loader<(S.Node?, [S.RrdPoint])>()
    @State private var range = "hour"

    var body: some View {
        List {
            if case let (n, rrd)? = data.value {
                if let n {
                    Section {
                        NodeRow(node: n)
                        if n.diskTotal != nil {
                            ResourceBar(label: "Root disk", fraction: ratio(n.diskUsed, n.diskTotal),
                                        detail: "\(Format.bytes(n.diskUsed)) / \(Format.bytes(n.diskTotal))")
                        }
                        KeyValueRow(key: "Uptime", value: Format.duration(n.uptimeS))
                        if let l = n.loadAvg { KeyValueRow(key: "Load", value: l.map { String(format: "%.2f", $0) }.joined(separator: " "), mono: true) }
                        if let v = n.pveVersion { KeyValueRow(key: "Proxmox", value: v, mono: true) }
                        if let c = n.cpuModel { KeyValueRow(key: "CPU", value: "\(n.cpuCount.map { "\($0) × " } ?? "")\(c)") }
                        if let ip = n.ip { KeyValueRow(key: "IP", value: ip, mono: true) }
                    }
                }
                Section {
                    Picker("Range", selection: $range) {
                        Text("Hour").tag("hour"); Text("Day").tag("day"); Text("Week").tag("week")
                    }
                    .pickerStyle(.segmented)
                    Chart(rrd, id: \.time) { p in
                        if let c = p.cpu {
                            LineMark(x: .value("Time", p.time), y: .value("Usage", c * 100), series: .value("Series", "CPU"))
                                .foregroundStyle(by: .value("Series", "CPU"))
                        }
                        if let m = p.memUsed, let t = p.memTotal, t > 0 {
                            LineMark(x: .value("Time", p.time), y: .value("Usage", m / t * 100), series: .value("Series", "RAM"))
                                .foregroundStyle(by: .value("Series", "RAM"))
                        }
                    }
                    .chartForegroundStyleScale(["CPU": Theme.accent, "RAM": Theme.progress])
                    .chartYScale(domain: 0...100)
                    .frame(height: 170)
                } header: { Text("CPU and memory (%)") }
            } else if let e = data.error {
                StateView(kind: .error(e, retry: { Task { await load() } })).listRowBackground(Color.clear)
            } else {
                StateView(kind: .loading).listRowBackground(Color.clear)
            }
        }
        .quorumBackground()
        .navigationTitle(name)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: "\(model.tick)-\(range)") { await load() }
    }

    func load() async {
        guard let client = model.client else { return }
        let r = Operations.NodeRrd.Input.Query.RangePayload(rawValue: range)
        await data.load {
            async let nodes = client.nodes().ok.body.json
            async let rrd = client.nodeRrd(path: .init(node: name), query: .init(range: r)).ok.body.json
            return try await (nodes.first { $0.name == name }, rrd)
        }
    }
}
