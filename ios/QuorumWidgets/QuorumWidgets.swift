import QuorumAPI
import QuorumCore
import SwiftUI
import WidgetKit

typealias Summary = Components.Schemas.WidgetSummary

/// Widgets read the summary the app last cached in the App Group and refresh it themselves
/// (`GET /api/v1/widget` with the Keychain token) every 15 minutes.
struct SummaryEntry: TimelineEntry {
    let date: Date
    let summary: Summary?
    /// Shown when nothing is cached yet.
    let message: String?
}

struct SummaryProvider: TimelineProvider {
    func placeholder(in context: Context) -> SummaryEntry {
        SummaryEntry(date: Date(), summary: nil, message: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (SummaryEntry) -> Void) {
        completion(SummaryEntry(date: Date(), summary: SharedCache.load(Summary.self, from: SharedCache.widget), message: nil))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SummaryEntry>) -> Void) {
        Task {
            let entry = await Self.fetch()
            let next = Calendar.current.date(byAdding: .minute, value: 15, to: Date()) ?? Date()
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    static func fetch() async -> SummaryEntry {
        let cached = SharedCache.load(Summary.self, from: SharedCache.widget)
        guard let creds = Keychain.load() else {
            return SummaryEntry(date: Date(), summary: nil, message: "Open Quorum to pair this iPhone")
        }
        do {
            let client = QuorumServer.client(server: creds.server, token: creds.token)
            let summary = try await client.widget().ok.body.json
            SharedCache.save(summary, as: SharedCache.widget)
            return SummaryEntry(date: Date(), summary: summary, message: nil)
        } catch {
            return SummaryEntry(
                date: Date(), summary: cached,
                message: cached == nil ? "Can't reach Quorum" : nil
            )
        }
    }
}

// MARK: - Shared pieces

struct Stale: View {
    let summary: Summary
    var body: some View {
        Text(Format.time(summary.generatedAt))
            .font(.caption2.monospacedDigit())
            .foregroundStyle(Theme.muted)
    }
}

struct Header: View {
    let title: String
    let symbol: String
    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: symbol).font(.caption2.weight(.semibold))
            Text(title.uppercased()).font(.caption2.weight(.semibold))
        }
        .foregroundStyle(Theme.accent2)
    }
}

struct Message: View {
    let text: String
    var body: some View {
        VStack(spacing: 6) {
            Image(systemName: "triangle.circle").foregroundStyle(Theme.accent2)
            Text(text).font(.caption).multilineTextAlignment(.center).foregroundStyle(Theme.muted)
        }
    }
}

func healthColor(_ h: Components.Schemas.Health) -> Color {
    Theme.health(h.rawValue).fg
}

// MARK: - Small: problems, or one service's number

struct ProblemsSmall: View {
    let entry: SummaryEntry
    var body: some View {
        if let s = entry.summary {
            let n = s.problems.count
            VStack(alignment: .leading, spacing: 4) {
                Header(title: "Quorum", symbol: "triangle")
                Spacer(minLength: 0)
                Text("\(n)")
                    .font(.system(size: 40, weight: .medium).monospacedDigit())
                    .foregroundStyle(n == 0 ? Theme.healthy : (s.status.down > 0 ? Theme.down : Theme.warning))
                Text(n == 1 ? "problem" : "problems").font(.caption).foregroundStyle(Theme.text)
                Text(n == 0 ? s.status.message : (s.problems.first?.title ?? ""))
                    .font(.caption2).foregroundStyle(Theme.muted).lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .widgetURL(URL(string: "quorum://open/"))
        } else {
            Message(text: entry.message ?? "Loading…")
        }
    }
}

struct ServiceSmall: View {
    let entry: SummaryEntry
    let serviceID: String?
    var body: some View {
        if let s = entry.summary, let svc = s.services.first(where: { $0.id == serviceID }) ?? s.services.first {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Image(systemName: Symbols.sf(svc.icon)).foregroundStyle(Theme.accent2)
                    Spacer()
                    Circle().fill(healthColor(svc.health)).frame(width: 8, height: 8)
                }
                Spacer(minLength: 0)
                Text(svc.name).font(.caption.weight(.medium)).foregroundStyle(Theme.text)
                Text(svc.headline)
                    .font(.headline.monospacedDigit()).foregroundStyle(Theme.text)
                    .lineLimit(2).minimumScaleFactor(0.7)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .widgetURL(URL(string: "quorum://open/services/\(svc.id)"))
        } else {
            Message(text: entry.message ?? "Loading…")
        }
    }
}

// MARK: - Medium: downloads or streams

struct DownloadsMedium: View {
    let entry: SummaryEntry
    var body: some View {
        if let s = entry.summary, let d = s.downloads {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Header(title: "Downloads", symbol: "arrow.down.circle")
                    Spacer()
                    Text("↓ \(Format.rate(d.dlSpeed))  ↑ \(Format.rate(d.upSpeed))")
                        .font(.caption2.monospacedDigit()).foregroundStyle(Theme.text)
                }
                if d.items.isEmpty {
                    Spacer()
                    Text(d.errored > 0 ? "\(d.errored) errored" : "Nothing downloading")
                        .font(.callout).foregroundStyle(Theme.muted)
                    Spacer()
                } else {
                    ForEach(Array(d.items.prefix(3).enumerated()), id: \.offset) { _, t in
                        VStack(alignment: .leading, spacing: 2) {
                            HStack {
                                Text(t.name).font(.caption).lineLimit(1).foregroundStyle(Theme.text)
                                Spacer()
                                Text(Format.percent(t.progress)).font(.caption2.monospacedDigit()).foregroundStyle(Theme.muted)
                            }
                            ProgressView(value: min(max(t.progress, 0), 1)).tint(Theme.accent)
                        }
                    }
                    Spacer(minLength: 0)
                }
            }
            .widgetURL(URL(string: "quorum://open/media"))
        } else {
            Message(text: entry.message ?? "No qBittorrent in Quorum")
        }
    }
}

struct StreamsMedium: View {
    let entry: SummaryEntry
    var body: some View {
        if let s = entry.summary, let st = s.streams {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Header(title: "Streams", symbol: "play.circle")
                    Spacer()
                    Text(st.transcodes > 0 ? "\(st.count) · \(st.transcodes) transcoding" : "\(st.count)")
                        .font(.caption2.monospacedDigit()).foregroundStyle(Theme.text)
                }
                if st.items.isEmpty {
                    Spacer()
                    Text("Nobody is watching").font(.callout).foregroundStyle(Theme.muted)
                    Spacer()
                } else {
                    ForEach(Array(st.items.prefix(3).enumerated()), id: \.offset) { _, i in
                        HStack(spacing: 8) {
                            Image(systemName: i.paused ? "pause.fill" : "play.fill")
                                .font(.caption2).foregroundStyle(i.transcoding ? Theme.warning : Theme.healthy)
                            VStack(alignment: .leading, spacing: 0) {
                                Text(i.title).font(.caption).lineLimit(1).foregroundStyle(Theme.text)
                                Text("\(i.user) · \(i.serviceName)").font(.caption2).lineLimit(1).foregroundStyle(Theme.muted)
                            }
                            Spacer()
                            if let p = i.progress {
                                Text(Format.percent(p)).font(.caption2.monospacedDigit()).foregroundStyle(Theme.muted)
                            }
                        }
                    }
                    Spacer(minLength: 0)
                }
            }
            .widgetURL(URL(string: "quorum://open/media"))
        } else {
            Message(text: entry.message ?? "No Jellyfin or Navidrome in Quorum")
        }
    }
}

// MARK: - Large: nodes and problems

struct OverviewLarge: View {
    let entry: SummaryEntry
    var body: some View {
        if let s = entry.summary {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Header(title: "Quorum", symbol: "triangle")
                    Spacer()
                    Stale(summary: s)
                }
                Text(s.status.message).font(.headline).foregroundStyle(Theme.text)
                if !s.nodes.isEmpty {
                    VStack(spacing: 6) {
                        ForEach(s.nodes, id: \.name) { n in
                            HStack(spacing: 8) {
                                Circle().fill(n.online ? Theme.healthy : Theme.down).frame(width: 7, height: 7)
                                Text(n.name).font(.caption.monospaced()).foregroundStyle(Theme.text)
                                    .frame(width: 64, alignment: .leading)
                                if n.online {
                                    Bar(label: "CPU", value: n.cpuUsage ?? 0)
                                    Bar(label: "RAM", value: ratio(n.memUsed, n.memTotal))
                                } else {
                                    Text("offline").font(.caption).foregroundStyle(Theme.down)
                                    Spacer()
                                }
                            }
                        }
                    }
                }
                Divider().overlay(Theme.border)
                if s.problems.isEmpty {
                    Label("No problems", systemImage: "checkmark.circle").font(.callout).foregroundStyle(Theme.healthy)
                } else {
                    ForEach(Array(s.problems.prefix(5).enumerated()), id: \.offset) { _, p in
                        HStack(alignment: .top, spacing: 8) {
                            Circle().fill(healthColor(p.health)).frame(width: 7, height: 7).padding(.top, 5)
                            VStack(alignment: .leading, spacing: 0) {
                                Text(p.title).font(.caption.weight(.medium)).foregroundStyle(Theme.text).lineLimit(1)
                                if let r = p.reason {
                                    Text(r).font(.caption2).foregroundStyle(Theme.muted).lineLimit(1)
                                }
                            }
                        }
                    }
                }
                Spacer(minLength: 0)
            }
            .widgetURL(URL(string: "quorum://open/"))
        } else {
            Message(text: entry.message ?? "Loading…")
        }
    }

    func ratio(_ a: Int64?, _ b: Int64?) -> Double {
        guard let a, let b, b > 0 else { return 0 }
        return Double(a) / Double(b)
    }

    struct Bar: View {
        let label: String
        let value: Double
        var body: some View {
            HStack(spacing: 4) {
                Text(label).font(.caption2).foregroundStyle(Theme.muted)
                ProgressView(value: min(max(value, 0), 1)).tint(Theme.usage(value))
                Text(Format.percent(value)).font(.caption2.monospacedDigit()).foregroundStyle(Theme.muted)
                    .frame(width: 34, alignment: .trailing)
            }
        }
    }
}

// MARK: - Lock screen

struct SpeedGauge: View {
    let entry: SummaryEntry
    var body: some View {
        let mbps = Double(entry.summary?.downloads?.dlSpeed ?? 0) / 1024 / 1024
        Gauge(value: min(mbps, 100), in: 0...100) {
            Image(systemName: "arrow.down")
        } currentValueLabel: {
            Text(mbps < 10 ? String(format: "%.1f", mbps) : String(format: "%.0f", mbps))
        }
        .gaugeStyle(.accessoryCircular)
        .widgetURL(URL(string: "quorum://open/media"))
        .accessibilityLabel("Download speed \(Format.rate(entry.summary?.downloads?.dlSpeed ?? 0))")
    }
}

struct ProblemsAccessory: View {
    @Environment(\.widgetFamily) var family
    let entry: SummaryEntry
    var body: some View {
        let n = entry.summary?.problems.count
        switch family {
        case .accessoryRectangular:
            VStack(alignment: .leading) {
                Text("Quorum").font(.headline)
                Text(n.map { $0 == 0 ? "All healthy" : "\($0) problem\($0 == 1 ? "" : "s")" } ?? "–")
                if let p = entry.summary?.problems.first { Text(p.title).font(.caption).lineLimit(1) }
            }
            .widgetURL(URL(string: "quorum://open/"))
        case .accessoryInline:
            Text(n.map { $0 == 0 ? "Quorum: all healthy" : "Quorum: \($0) problems" } ?? "Quorum")
        default:
            ZStack {
                AccessoryWidgetBackground()
                VStack(spacing: 0) {
                    Image(systemName: n == 0 ? "checkmark" : "exclamationmark.triangle").font(.caption)
                    Text(n.map(String.init) ?? "–").font(.title3.weight(.semibold).monospacedDigit())
                }
            }
            .widgetURL(URL(string: "quorum://open/"))
        }
    }
}

// MARK: - Widgets

struct ProblemsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "problems", provider: SummaryProvider()) { entry in
            ProblemsSmall(entry: entry).containerBackground(Theme.bg, for: .widget)
        }
        .configurationDisplayName("Problems")
        .description("How many services need attention.")
        .supportedFamilies([.systemSmall])
    }
}

struct ServiceWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "service", provider: SummaryProvider()) { entry in
            ServiceSmall(entry: entry, serviceID: AppGroup.defaults.string(forKey: "widget.service"))
                .containerBackground(Theme.bg, for: .widget)
        }
        .configurationDisplayName("Service")
        .description("One service's main number. Pick the service in the app's Settings.")
        .supportedFamilies([.systemSmall])
    }
}

struct DownloadsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "downloads", provider: SummaryProvider()) { entry in
            DownloadsMedium(entry: entry).containerBackground(Theme.bg, for: .widget)
        }
        .configurationDisplayName("Downloads")
        .description("qBittorrent speed and the busiest downloads.")
        .supportedFamilies([.systemMedium])
    }
}

struct StreamsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "streams", provider: SummaryProvider()) { entry in
            StreamsMedium(entry: entry).containerBackground(Theme.bg, for: .widget)
        }
        .configurationDisplayName("Streams")
        .description("Who is watching or listening.")
        .supportedFamilies([.systemMedium])
    }
}

struct OverviewWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "overview", provider: SummaryProvider()) { entry in
            OverviewLarge(entry: entry).containerBackground(Theme.bg, for: .widget)
        }
        .configurationDisplayName("Overview")
        .description("Proxmox nodes and current problems.")
        .supportedFamilies([.systemLarge])
    }
}

struct SpeedWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "speed", provider: SummaryProvider()) { entry in
            SpeedGauge(entry: entry).containerBackground(.clear, for: .widget)
        }
        .configurationDisplayName("Download speed")
        .description("qBittorrent download speed in MB/s.")
        .supportedFamilies([.accessoryCircular])
    }
}

struct ProblemsLockWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "problems-lock", provider: SummaryProvider()) { entry in
            ProblemsAccessory(entry: entry).containerBackground(.clear, for: .widget)
        }
        .configurationDisplayName("Problems")
        .description("Problem count on the lock screen.")
        .supportedFamilies([.accessoryCircular, .accessoryRectangular, .accessoryInline])
    }
}

@main
struct QuorumWidgetBundle: WidgetBundle {
    var body: some Widget {
        ProblemsWidget()
        ServiceWidget()
        DownloadsWidget()
        StreamsWidget()
        OverviewWidget()
        SpeedWidget()
        ProblemsLockWidget()
    }
}
