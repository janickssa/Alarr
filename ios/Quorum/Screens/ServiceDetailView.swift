import Charts
import QuorumCore
import SwiftUI

/// One service: header, quick actions and the module's own data.
struct ServiceDetailView: View {
    @EnvironmentObject var model: AppModel
    let id: String
    @StateObject private var data = Loader<(S.ServiceSnapshot, S.ServiceHistory?)>()

    var service: S.ServiceView? { model.service(id) }

    var body: some View {
        List {
            if let s = service {
                Section {
                    HStack(spacing: 12) {
                        ServiceIcon(icon: s.icon, hue: Double(s.accentHue), chroma: Double(s.accentChroma), size: 44)
                        VStack(alignment: .leading, spacing: 4) {
                            HealthBadge(health: s.health.rawValue)
                            if let r = s.healthReason { Text(r).font(.caption).foregroundStyle(Theme.muted) }
                        }
                        Spacer()
                    }
                    if let v = s.version { KeyValueRow(key: "Version", value: v, mono: true) }
                    if let ms = s.responseMs { KeyValueRow(key: "Response", value: Format.ms(ms)) }
                    if let g = s.linkedGuest {
                        NavigationLink(value: Route.guest(g.vmid)) {
                            KeyValueRow(key: "Runs on", value: "\(g.name) (\(g.vmid)) · \(g.node)")
                        }
                    }
                    if let u = s.baseUrl, let url = URL(string: u) {
                        Link(destination: url) { Label("Open \(s.name)", systemImage: "arrow.up.right.square") }
                    }
                }
            }
            if case let (snap, history)? = data.value {
                if snap.stale {
                    Label("Showing data from \(Format.time(snap.updatedAt)); the last check failed.", systemImage: "clock.badge.exclamationmark")
                        .font(.caption).foregroundStyle(Theme.warning)
                }
                ModuleSections(serviceID: id, module: snap.data.module, history: history)
            } else if let e = data.error {
                StateView(kind: .error(e, retry: { Task { await load() } })).listRowBackground(Color.clear)
            } else {
                StateView(kind: .loading).listRowBackground(Color.clear)
            }
        }
        .quorumBackground()
        .navigationTitle(service?.name ?? id)
        .navigationBarTitleDisplayMode(.inline)
        .refreshable {
            await model.action(id, "refresh", label: "Checking \(service?.name ?? id)")
            await load()
        }
        .task(id: model.tick) { await load() }
    }

    func load() async {
        guard let client = model.client else { return }
        let isCheck = service?.kind.rawValue == "health_check"
        await data.load {
            let snap = try await client.snapshot(path: .init(id: id)).ok.body.json
            let hist = isCheck ? try? await client.history(path: .init(id: id)).ok.body.json : nil
            return (snap, hist)
        }
    }
}

/// The module-specific part of a service page.
struct ModuleSections: View {
    @EnvironmentObject var model: AppModel
    let serviceID: String
    let module: Module
    let history: S.ServiceHistory?

    var body: some View {
        switch module {
        case .none:
            EmptyView()
        case .proxmox(let d):
            Section {
                KeyValueRow(key: "Nodes", value: "\(d.cluster.nodesOnline)/\(d.cluster.nodesTotal)")
                KeyValueRow(key: "Guests", value: "\(d.guests.filter { $0.status.rawValue == "running" }.count) running")
                Button("Open Proxmox") { model.open(path: "/proxmox") }
            }
        case .healthCheck(let d):
            Section("Check") {
                KeyValueRow(key: d.check.rawValue.uppercased(), value: d.target, mono: true)
                if let c = d.statusCode { KeyValueRow(key: "Status", value: "HTTP \(c)") }
                if let h = history {
                    KeyValueRow(key: "Uptime 24 h", value: Format.uptime(h.uptime24h))
                    KeyValueRow(key: "Uptime 7 d", value: Format.uptime(h.uptime7d))
                    if let i = h.lastIncident {
                        KeyValueRow(key: "Last incident", value: "\(Format.dateTime(i.start)) · \(i.minutes) min")
                    }
                }
            }
            if let h = history, !h.points.isEmpty {
                Section("Response time, 24 h (ms)") {
                    Chart(h.points.filter { $0.responseMs != nil }, id: \.at) { p in
                        LineMark(x: .value("Time", p.at), y: .value("ms", p.responseMs ?? 0))
                            .foregroundStyle(Theme.accent)
                    }
                    .frame(height: 140)
                }
            }
        case .jellyfin(let d):
            jellyfin(d)
        case .seerr(let d):
            seerr(d)
        case .arr(let d):
            arr(d)
        case .prowlarr(let d):
            Section {
                actionButton("Test all indexers", "test_all")
            }
            issues(d.health)
            Section("Indexers") {
                ForEach(d.indexers, id: \.id) { i in
                    HStack {
                        HealthDot(health: !i.enabled ? "disabled" : i.status == "ok" ? "healthy" : "warning")
                        Text(i.name)
                        Spacer()
                        Text("\(i.grabs24h) grabs · \(i.failures24h) failed").font(.caption.monospacedDigit()).foregroundStyle(Theme.muted)
                    }
                }
            }
        case .bazarr(let d):
            Section {
                KeyValueRow(key: "Missing (episodes)", value: "\(d.missingEpisodes)")
                KeyValueRow(key: "Missing (movies)", value: "\(d.missingMovies)")
                actionButton("Search all missing", "search_all")
            }
            if !d.wanted.isEmpty {
                Section("Wanted") {
                    ForEach(Array(d.wanted.prefix(20).enumerated()), id: \.offset) { _, w in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(w.title).font(.callout)
                                if let s = w.subtitle { Text(s).font(.caption).foregroundStyle(Theme.muted) }
                            }
                            Spacer()
                            Text(w.languages.joined(separator: ", ")).font(.caption.monospaced())
                        }
                        .swipeActions {
                            Button("Search") {
                                Task { await model.action(serviceID, "search", args: ["kind": w.kind, "id": w.itemId], label: "Searching \(w.title)") }
                            }
                            .tint(Theme.accent)
                        }
                    }
                }
            }
        case .qbittorrent(let d):
            qbittorrent(d)
        case .navidrome(let d):
            Section {
                if let s = d.songs { KeyValueRow(key: "Songs", value: "\(s)") }
                KeyValueRow(key: "Scanning", value: d.scanning ? "Yes" : "No")
                if let l = d.lastScan { KeyValueRow(key: "Last scan", value: l) }
                actionButton("Scan library", "scan")
                actionButton("Full rescan", "full_scan")
            }
            if !d.nowPlaying.isEmpty {
                Section("Now playing") {
                    ForEach(Array(d.nowPlaying.enumerated()), id: \.offset) { _, n in
                        VStack(alignment: .leading) {
                            Text(n.title).font(.callout)
                            Text([n.artist, n.user].compactMap { $0 }.joined(separator: " · ")).font(.caption).foregroundStyle(Theme.muted)
                        }
                    }
                }
            }
        case .technitium(let d):
            Section {
                KeyValueRow(key: "Queries (24 h)", value: "\(d.totalQueries)")
                KeyValueRow(key: "Blocked", value: "\(d.blocked) · \(Format.percent(d.totalQueries > 0 ? Double(d.blocked) / Double(d.totalQueries) : nil))")
                KeyValueRow(key: "Clients", value: "\(d.clients)")
                if d.blockingEnabled && d.pausedUntil == nil {
                    Menu {
                        ForEach([5, 30, 60], id: \.self) { m in
                            Button("\(m) min") {
                                Task { await model.action(serviceID, "pause_blocking", args: ["minutes": m], label: "Pausing blocking") }
                            }
                        }
                    } label: { Label("Pause blocking", systemImage: "pause.circle") }
                } else {
                    if let u = d.pausedUntil { KeyValueRow(key: "Paused until", value: Format.time(u)) }
                    actionButton("Resume blocking", "resume_blocking")
                }
            }
            top("Top blocked", d.topBlocked)
            top("Top clients", d.topClients)
        case .npm(let d):
            Section("Proxy hosts") {
                ForEach(d.hosts, id: \.id) { h in
                    HStack {
                        HealthDot(health: !h.enabled ? "disabled" : h.online == false ? "down" : "healthy")
                        VStack(alignment: .leading) {
                            Text(h.domains.joined(separator: ", ")).font(.callout).lineLimit(1)
                            Text("→ \(h.forward)").font(.caption.monospaced()).foregroundStyle(Theme.muted)
                        }
                    }
                }
            }
            Section("Certificates") {
                ForEach(d.certificates, id: \.id) { c in
                    HStack {
                        Text(c.name).lineLimit(1)
                        Spacer()
                        Text(c.daysLeft.map { "\($0) d" } ?? "–")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle((c.daysLeft ?? 99) < 14 ? Theme.warning : Theme.muted)
                    }
                }
            }
        case .authentik(let d):
            Section {
                KeyValueRow(key: "Logins (24 h)", value: "\(d.logins24h)\(d.loginsCapped ? "+" : "")")
                KeyValueRow(key: "Failed logins (24 h)", value: "\(d.failed24h)")
                if let s = d.sessions { KeyValueRow(key: "Sessions", value: "\(s)") }
                if let u = d.updateAvailable { KeyValueRow(key: "Update available", value: u) }
            }
            Section("Outposts") {
                ForEach(d.outposts, id: \.id) { o in
                    HStack {
                        HealthDot(health: o.healthy ? (o.outdated ? "warning" : "healthy") : "down")
                        Text(o.name)
                        Spacer()
                        Text(o.version ?? "").font(.caption.monospaced()).foregroundStyle(Theme.muted)
                    }
                }
            }
            Section("Recent events") {
                ForEach(Array(d.events.prefix(15).enumerated()), id: \.offset) { _, e in
                    HStack {
                        Image(systemName: e.action == "login_failed" ? "xmark.circle" : "person.crop.circle.badge.checkmark")
                            .foregroundStyle(e.action == "login_failed" ? Theme.down : Theme.healthy)
                        VStack(alignment: .leading) {
                            Text(e.user ?? "unknown").font(.callout)
                            Text([e.app, e.clientIp].compactMap { $0 }.joined(separator: " · ")).font(.caption).foregroundStyle(Theme.muted)
                        }
                        Spacer()
                        Text(Format.ago(e.at)).font(.caption).foregroundStyle(Theme.muted)
                    }
                }
            }
        case .nextcloud(let d):
            Section {
                KeyValueRow(key: "Users", value: "\(d.users)")
                KeyValueRow(key: "Active (5 min / 1 h / 24 h)", value: "\(d.active5m) / \(d.active1h) / \(d.active24h)")
                KeyValueRow(key: "Files", value: "\(d.files)")
                KeyValueRow(key: "Shares", value: "\(d.shares)")
                if let f = d.freeSpace { KeyValueRow(key: "Free space", value: Format.bytes(f)) }
                if d.appUpdates > 0 { KeyValueRow(key: "App updates", value: "\(d.appUpdates)") }
                if let u = d.updateAvailable { KeyValueRow(key: "Update available", value: u) }
                if let p = d.phpVersion { KeyValueRow(key: "PHP", value: p, mono: true) }
            }
        case .wireguard(let d):
            if !d.agentInstalled {
                Text("The Quorum agent isn't answering on this host. See deploy/ for how to install it.")
                    .font(.callout).foregroundStyle(Theme.warning)
            }
            Section("Peers") {
                ForEach(d.peers, id: \.publicKey) { p in
                    HStack {
                        HealthDot(health: p.online ? "healthy" : "disabled")
                        VStack(alignment: .leading) {
                            Text(p.name ?? String(p.publicKey.prefix(10)) + "…").font(.callout)
                            Text(p.lastHandshake.map { "handshake \(Format.ago($0))" } ?? "never connected")
                                .font(.caption).foregroundStyle(Theme.muted)
                        }
                        Spacer()
                        Text("↓ \(Format.bytes(p.rxBytes)) ↑ \(Format.bytes(p.txBytes))").font(.caption2.monospacedDigit()).foregroundStyle(Theme.muted)
                    }
                }
            }
        }
    }

    // MARK: - Pieces

    func actionButton(_ label: String, _ action: String) -> some View {
        Button(label) { Task { await model.action(serviceID, action, label: label) } }
    }

    func issues(_ list: [S.HealthIssue]) -> some View {
        Group {
            if !list.isEmpty {
                Section("Health") {
                    ForEach(Array(list.enumerated()), id: \.offset) { _, i in
                        Label(i.message, systemImage: "exclamationmark.triangle")
                            .font(.callout)
                            .foregroundStyle(i.level == "error" ? Theme.down : Theme.warning)
                    }
                }
            }
        }
    }

    func top(_ title: String, _ list: [S.TopEntry]) -> some View {
        Group {
            if !list.isEmpty {
                Section(title) {
                    ForEach(Array(list.prefix(8).enumerated()), id: \.offset) { _, t in
                        HStack {
                            Text(t.name).font(.callout.monospaced()).lineLimit(1)
                            Spacer()
                            Text("\(t.hits)").font(.caption.monospacedDigit()).foregroundStyle(Theme.muted)
                        }
                    }
                }
            }
        }
    }

    func jellyfin(_ d: S.JellyfinData) -> some View {
        Group {
            Section {
                KeyValueRow(key: "Movies", value: "\(d.counts.movies)")
                KeyValueRow(key: "Series", value: "\(d.counts.series) · \(d.counts.episodes) episodes")
                if d.counts.songs > 0 { KeyValueRow(key: "Songs", value: "\(d.counts.songs)") }
                actionButton("Scan all libraries", "scan_all")
            }
            Section("Streams") {
                if d.streams.isEmpty { Text("Nobody is watching").foregroundStyle(Theme.muted) }
                ForEach(d.streams, id: \.sessionId) { s in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(s.title).font(.callout.weight(.medium)).lineLimit(1)
                            Spacer()
                            Text(s.user).font(.caption)
                        }
                        Text("\(s.playMethod) · \(s.device)\(s.bitrate.map { " · \(Format.bitrate($0))" } ?? "")")
                            .font(.caption).foregroundStyle(s.playMethod == "Transcode" ? Theme.warning : Theme.muted)
                        if let p = s.positionS, let r = s.runtimeS, r > 0 { ProgressView(value: p / r).tint(Theme.accent) }
                    }
                }
            }
            Section("Libraries") {
                ForEach(d.libraries, id: \.id) { l in
                    HStack {
                        Text(l.name)
                        Spacer()
                        if let n = l.itemCount { Text("\(n)").font(.caption.monospacedDigit()).foregroundStyle(Theme.muted) }
                    }
                    .swipeActions {
                        Button("Scan") {
                            Task { await model.action(serviceID, "scan_library", args: ["id": l.id], label: "Scanning \(l.name)") }
                        }
                        .tint(Theme.accent)
                    }
                }
            }
        }
    }

    func seerr(_ d: S.SeerrData) -> some View {
        Group {
            Section {
                KeyValueRow(key: "Pending", value: "\(d.counts.pending)")
                KeyValueRow(key: "Processing", value: "\(d.counts.processing)")
                KeyValueRow(key: "Available", value: "\(d.counts.available)")
            }
            Section("Pending requests") {
                if d.pending.isEmpty { Text("Nothing to approve").foregroundStyle(Theme.muted) }
                ForEach(d.pending, id: \.id) { r in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(r.title + (r.year.map { " (\($0))" } ?? "")).font(.callout.weight(.medium))
                        Text("\(r.mediaType == "tv" ? "Series" : "Movie") · \(r.requestedBy)\(r.is4k ? " · 4K" : "")")
                            .font(.caption).foregroundStyle(Theme.muted)
                    }
                    .swipeActions(edge: .leading) {
                        Button { Task { await model.action(serviceID, "approve", args: ["id": r.id], label: "Approving \(r.title)") } } label: {
                            Label("Approve", systemImage: "checkmark")
                        }
                        .tint(Theme.healthy)
                    }
                    .swipeActions(edge: .trailing) {
                        Button(role: .destructive) {
                            Task { await model.action(serviceID, "decline", args: ["id": r.id], label: "Declining \(r.title)") }
                        } label: { Label("Decline", systemImage: "xmark") }
                    }
                    .contextMenu {
                        Button { Task { await model.action(serviceID, "approve", args: ["id": r.id], label: "Approving \(r.title)") } } label: {
                            Label("Approve", systemImage: "checkmark")
                        }
                        Button(role: .destructive) {
                            Task { await model.action(serviceID, "decline", args: ["id": r.id], label: "Declining \(r.title)") }
                        } label: { Label("Decline", systemImage: "xmark") }
                    }
                }
            }
        }
    }

    func arr(_ d: S.ArrData) -> some View {
        Group {
            Section {
                KeyValueRow(key: d.flavor.rawValue == "sonarr" ? "Series" : "Movies", value: "\(d.libraryCount) · \(Format.bytes(d.librarySize))")
                KeyValueRow(key: "Missing", value: "\(d.missingTotal)")
                ForEach(d.disks, id: \.path) { disk in
                    ResourceBar(label: disk.path, fraction: disk.total > 0 ? 1 - Double(disk.free) / Double(disk.total) : nil,
                                detail: "\(Format.bytes(disk.free)) free")
                }
            }
            Section("Actions") {
                actionButton("Rescan disk", "rescan")
                actionButton("Refresh metadata", "refresh")
                actionButton("RSS sync", "rss_sync")
                actionButton("Search all missing", "search_missing")
            }
            issues(d.health)
            Section("Queue") {
                if d.queue.isEmpty { Text("Queue is empty").foregroundStyle(Theme.muted) }
                ForEach(d.queue, id: \.id) { q in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(q.title).font(.callout).lineLimit(1)
                        if let s = q.subtitle { Text(s).font(.caption).foregroundStyle(Theme.muted) }
                        ProgressView(value: q.size > 0 ? 1 - Double(q.sizeLeft) / Double(q.size) : 0).tint(Theme.accent)
                        if let m = q.message { Text(m).font(.caption).foregroundStyle(Theme.warning) }
                    }
                }
            }
            if !d.calendar.isEmpty {
                Section("Coming up") {
                    ForEach(Array(d.calendar.prefix(10).enumerated()), id: \.offset) { _, c in
                        HStack {
                            Text(c.title).lineLimit(1)
                            Spacer()
                            Text(Format.date(c.at)).font(.caption.monospacedDigit()).foregroundStyle(Theme.muted)
                        }
                    }
                }
            }
            if !d.missing.isEmpty {
                Section("Missing") {
                    ForEach(Array(d.missing.prefix(20).enumerated()), id: \.offset) { _, m in
                        VStack(alignment: .leading) {
                            Text(m.title).font(.callout)
                            if let s = m.subtitle { Text(s).font(.caption).foregroundStyle(Theme.muted) }
                        }
                        .swipeActions {
                            Button("Search") {
                                Task { await model.action(serviceID, "search_item", args: ["id": m.id], label: "Searching \(m.title)") }
                            }
                            .tint(Theme.accent)
                        }
                    }
                }
            }
        }
    }

    func qbittorrent(_ d: S.QbittorrentData) -> some View {
        Group {
            Section {
                HStack(spacing: 10) {
                    StatTile(title: "Down", value: Format.rate(d.dlSpeed))
                    StatTile(title: "Up", value: Format.rate(d.upSpeed))
                }
                .listRowInsets(EdgeInsets()).listRowBackground(Color.clear)
            }
            Section {
                if !d.history.isEmpty {
                    Chart(d.history, id: \.at) { p in
                        AreaMark(x: .value("Time", p.at), y: .value("MB/s", Double(p.dl) / 1_048_576))
                            .foregroundStyle(Theme.accent.opacity(0.35))
                        LineMark(x: .value("Time", p.at), y: .value("MB/s", Double(p.dl) / 1_048_576))
                            .foregroundStyle(Theme.accent)
                    }
                    .frame(height: 110)
                    .accessibilityLabel("Download speed, last 10 minutes")
                }
                Toggle("Alternative speed limits", isOn: Binding(get: { d.altSpeed }, set: { _ in
                    Task { await model.action(serviceID, "toggle_alt_speed", label: "Switching speed limits") }
                }))
                if let f = d.freeSpace { KeyValueRow(key: "Free space", value: Format.bytes(f)) }
            } header: { Text("Download speed (MB/s)") }
            Section("Torrents") {
                ForEach(d.torrents.sorted { (rank($0), $0.name) < (rank($1), $1.name) }, id: \.hash) { t in
                    TorrentRow(torrent: t)
                        .swipeActions(edge: .leading) {
                            let stopped = t.state.rawValue == "stopped"
                            Button {
                                Task { await model.action(serviceID, stopped ? "start" : "stop", args: ["hashes": [t.hash]], label: stopped ? "Starting" : "Stopping") }
                            } label: { Label(stopped ? "Start" : "Stop", systemImage: stopped ? "play.fill" : "pause.fill") }
                            .tint(stopped ? Theme.healthy : Theme.warning)
                        }
                        .swipeActions(edge: .trailing) {
                            Button(role: .destructive) { deleteTorrent(t) } label: { Label("Delete", systemImage: "trash") }
                        }
                }
            }
        }
    }

    /// Downloading first, then problems, then the rest.
    func rank(_ t: S.Torrent) -> Int {
        switch t.state.rawValue {
        case "downloading": 0
        case "errored", "stalled": 1
        default: 2
        }
    }

    func deleteTorrent(_ t: S.Torrent) {
        model.confirm = ConfirmRequest(
            title: "Delete torrent?",
            message: t.name,
            confirmLabel: "Delete", destructive: true,
            toggleLabel: "Also delete downloaded files"
        ) { files in
            Task {
                await model.action(serviceID, "delete", args: ["hashes": [t.hash], "delete_files": files], confirm: true, label: "Deleting")
            }
        }
    }
}

struct TorrentRow: View {
    let torrent: S.Torrent
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(torrent.name).font(.callout).lineLimit(1)
            ProgressView(value: min(max(torrent.progress, 0), 1))
                .tint(torrent.state.rawValue == "errored" ? Theme.down : torrent.state.rawValue == "stalled" ? Theme.warning : Theme.accent)
            HStack {
                Text(torrent.state.rawValue).font(.caption).foregroundStyle(torrent.state.rawValue == "errored" ? Theme.down : Theme.muted)
                Spacer()
                Text("\(Format.percent(torrent.progress)) of \(Format.bytes(torrent.size))")
                if torrent.dlSpeed > 0 { Text("· \(Format.rate(torrent.dlSpeed))") }
                if let e = torrent.etaS, torrent.state.rawValue == "downloading" { Text("· \(Format.duration(e))") }
            }
            .font(.caption.monospacedDigit()).foregroundStyle(Theme.muted)
        }
        .accessibilityElement(children: .combine)
    }
}
