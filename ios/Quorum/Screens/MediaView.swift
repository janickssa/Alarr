import QuorumCore
import SwiftUI

struct MediaView: View {
    @EnvironmentObject var model: AppModel
    @StateObject private var data = Loader<S.MediaOverview>()

    var body: some View {
        List {
            if let m = data.value {
                Section {
                    HStack(spacing: 10) {
                        StatTile(title: "Streams", value: "\(m.counts.streams)",
                                 sub: m.counts.transcodes > 0 ? "\(m.counts.transcodes) transcoding" : nil)
                        StatTile(title: "Downloading", value: "\(m.counts.downloading)")
                        StatTile(title: "Requests", value: "\(m.counts.pendingRequests)", sub: "pending")
                    }
                    .listRowInsets(EdgeInsets()).listRowBackground(Color.clear)
                }
                Section("Now playing") {
                    if m.nowPlaying.isEmpty {
                        Text("Nobody is watching or listening").foregroundStyle(Theme.muted)
                    }
                    ForEach(Array(m.nowPlaying.enumerated()), id: \.offset) { _, p in
                        NowPlayingRow(entry: p)
                    }
                }
                if !m.requests.isEmpty {
                    Section("Requests") {
                        ForEach(m.requests, id: \.seerrId) { r in
                            RequestRow(tracked: r)
                        }
                    }
                }
                if !m.calendar.isEmpty {
                    Section("Coming up") {
                        ForEach(Array(m.calendar.prefix(12).enumerated()), id: \.offset) { _, c in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(c.item.title).font(.callout)
                                    if let s = c.item.subtitle { Text(s).font(.caption).foregroundStyle(Theme.muted) }
                                }
                                Spacer()
                                VStack(alignment: .trailing, spacing: 2) {
                                    Text(Format.date(c.item.at)).font(.caption.monospacedDigit())
                                    if c.item.hasFile {
                                        Text("Downloaded").font(.caption2).foregroundStyle(Theme.healthy)
                                    }
                                }
                            }
                        }
                    }
                }
                if !m.recentlyAdded.isEmpty {
                    Section("Recently added") {
                        ForEach(Array(m.recentlyAdded.prefix(12).enumerated()), id: \.offset) { _, r in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(r.item.seriesName ?? r.item.name).font(.callout)
                                    if r.item.seriesName != nil { Text(r.item.name).font(.caption).foregroundStyle(Theme.muted) }
                                }
                                Spacer()
                                Text(Format.ago(r.item.addedAt)).font(.caption).foregroundStyle(Theme.muted)
                            }
                        }
                    }
                }
            } else if let e = data.error {
                StateView(kind: .error(e, retry: { Task { await load() } })).listRowBackground(Color.clear)
            } else {
                StateView(kind: .loading).listRowBackground(Color.clear)
            }
        }
        .quorumBackground()
        .navigationTitle("Media")
        .refreshable { await model.refresh() }
        .task(id: model.tick) { await load() }
    }

    func load() async {
        guard let client = model.client else { return }
        await data.load { try await client.mediaOverview().ok.body.json }
    }
}

struct NowPlayingRow: View {
    let entry: S.NowPlayingEntry
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image(systemName: entry.kind == "music" ? "music.note" : (entry.paused ? "pause.fill" : "play.fill"))
                    .foregroundStyle(entry.playMethod == "Transcode" ? Theme.warning : Theme.healthy)
                VStack(alignment: .leading, spacing: 1) {
                    Text(entry.title).font(.callout.weight(.medium)).lineLimit(1)
                    if let s = entry.subtitle { Text(s).font(.caption).foregroundStyle(Theme.muted).lineLimit(1) }
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 1) {
                    Text(entry.user).font(.caption)
                    Text(entry.remote ? "remote" : (entry.device ?? entry.serviceName)).font(.caption2).foregroundStyle(Theme.muted)
                }
            }
            if let p = entry.progress { ProgressView(value: p).tint(Theme.accent) }
            if let d = entry.detail {
                Text(d).font(.caption2.monospaced()).foregroundStyle(Theme.muted).lineLimit(1)
            }
            if let r = entry.transcodeReason {
                Text("Transcoding: \(r)").font(.caption2).foregroundStyle(Theme.warning)
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }
}

/// Seerr → Sonarr/Radarr → qBittorrent → Jellyfin, as on the web.
struct RequestRow: View {
    let tracked: S.TrackedRequest

    var current: S.TrackerStep? {
        tracked.steps.first { $0.state.rawValue == "stuck" } ?? tracked.steps.first { $0.state.rawValue == "current" }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(tracked.request.title + (tracked.request.year.map { " (\($0))" } ?? ""))
                    .font(.callout.weight(.medium)).lineLimit(1)
                Spacer()
                Text(tracked.request.requestedBy).font(.caption).foregroundStyle(Theme.muted)
            }
            HStack(spacing: 4) {
                ForEach(Array(tracked.steps.enumerated()), id: \.offset) { _, s in
                    Capsule().fill(color(s.state.rawValue)).frame(height: 5)
                }
            }
            if let c = current {
                Text("\(c.label)\(c.detail.map { " · \($0)" } ?? "")")
                    .font(.caption)
                    .foregroundStyle(c.state.rawValue == "stuck" ? Theme.down : Theme.muted)
            } else if tracked.steps.allSatisfy({ $0.state.rawValue == "done" }) {
                Text("Available").font(.caption).foregroundStyle(Theme.healthy)
            }
        }
        .accessibilityElement(children: .combine)
    }

    func color(_ s: String) -> Color {
        switch s {
        case "done": Theme.healthy
        case "current": Theme.progress
        case "stuck": Theme.down
        default: Theme.raised
        }
    }
}
