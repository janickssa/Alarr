import QuorumCore
import SwiftUI

/// Loads data for a screen and reloads on live updates (`model.tick`) and pull to refresh.
@MainActor
final class Loader<T>: ObservableObject {
    @Published var value: T?
    @Published var error: String?

    func load(_ fetch: () async throws -> T) async {
        do {
            value = try await fetch()
            error = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            self.error = describe(error)
        }
    }
}

struct HomeView: View {
    @EnvironmentObject var model: AppModel
    @StateObject private var data = Loader<S.Overview>()

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if !model.connected {
                    Label("Live updates paused — reconnecting", systemImage: "wifi.slash")
                        .font(.caption).foregroundStyle(Theme.warning)
                }
                if let ov = data.value {
                    status(ov.status)
                    if !ov.problems.isEmpty { problems(ov.problems) }
                    if !ov.nodes.isEmpty { nodes(ov.nodes) }
                    highlights(ov.highlights)
                } else if let e = data.error {
                    StateView(kind: .error(e, retry: { Task { await load() } }))
                } else {
                    StateView(kind: .loading)
                }
            }
            .padding(16)
        }
        .quorumBackground()
        .navigationTitle("Home")
        .refreshable { await model.refresh() }
        .task(id: model.tick) { await load() }
    }

    func load() async {
        guard let client = model.client else { return }
        await data.load { try await client.overview().ok.body.json }
    }

    func status(_ s: S.StatusSummary) -> some View {
        let tone = s.down > 0 ? "down" : s.warning > 0 ? "warning" : "healthy"
        return HStack(spacing: 12) {
            Image(systemName: tone == "healthy" ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                .font(.title2).foregroundStyle(Theme.health(tone).fg)
            VStack(alignment: .leading, spacing: 2) {
                Text(s.message).font(.headline)
                Text("\(s.healthy) healthy · \(s.warning) warning · \(s.down) down")
                    .font(.caption).foregroundStyle(Theme.muted)
            }
            Spacer()
        }
        .padding(14)
        .background(Theme.health(tone).fill, in: RoundedRectangle(cornerRadius: 14))
        .accessibilityElement(children: .combine)
    }

    func problems(_ list: [S.Problem]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionTitle(text: "Problems")
            ForEach(Array(list.enumerated()), id: \.offset) { _, p in
                HStack(spacing: 10) {
                    HealthDot(health: p.health.rawValue)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(p.title).font(.callout.weight(.medium))
                        if let r = p.reason { Text(r).font(.caption).foregroundStyle(Theme.muted) }
                    }
                    Spacer()
                    if let qa = p.quickAction {
                        Button(qa.label) { Task { await quick(p, qa) } }
                            .buttonStyle(.bordered).controlSize(.small)
                    }
                }
                .padding(12)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12))
                .contentShape(Rectangle())
                .onTapGesture {
                    if let id = p.serviceId { model.push(.service(id)) } else if let v = p.vmid { model.push(.guest(v), on: .proxmox) }
                }
            }
        }
    }

    /// Quick actions from the overview: retry (refresh) a service or start a stopped guest.
    func quick(_ p: S.Problem, _ qa: S.QuickAction) async {
        switch qa.kind.rawValue {
        case "start_guest":
            if let v = p.vmid, let client = model.client,
               let g = try? await client.guest(path: .init(vmid: v)).ok.body.json.guest {
                model.power(g, "start")
            }
        default:
            if let id = p.serviceId { await model.action(id, "refresh", label: "Checking \(p.title)") }
        }
    }

    func nodes(_ list: [S.NodeSummary]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionTitle(text: "Nodes")
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 10) {
                    ForEach(list, id: \.name) { n in
                        Button { model.push(.node(n.name), on: .proxmox) } label: {
                            VStack(alignment: .leading, spacing: 8) {
                                HStack {
                                    HealthDot(health: n.online ? "healthy" : "down")
                                    Text(n.name).font(.callout.monospaced())
                                }
                                if n.online {
                                    ResourceBar(label: "CPU", fraction: n.cpuUsage)
                                    ResourceBar(label: "RAM", fraction: ratio(n.memUsed, n.memTotal))
                                    Text("\(n.guestsRunning) running").font(.caption).foregroundStyle(Theme.muted)
                                } else {
                                    Text("Offline").font(.caption).foregroundStyle(Theme.down)
                                }
                            }
                            .frame(width: 150, alignment: .leading)
                            .padding(12)
                            .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14))
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }

    func highlights(_ list: [S.Highlight]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionTitle(text: "Services")
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                ForEach(list, id: \.service.id) { h in
                    Button { model.push(.service(h.service.id), on: .services) } label: {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                ServiceIcon(icon: h.service.icon, hue: Double(h.service.accentHue), chroma: Double(h.service.accentChroma), size: 28)
                                Spacer()
                                HealthDot(health: h.service.health.rawValue)
                            }
                            Text(h.service.name).font(.callout.weight(.medium)).lineLimit(1)
                            Text(h.headline).font(.caption.monospacedDigit()).lineLimit(2)
                            if let d = h.detail { Text(d).font(.caption2).foregroundStyle(Theme.muted).lineLimit(1) }
                        }
                        .frame(maxWidth: .infinity, minHeight: 110, alignment: .topLeading)
                        .padding(12)
                        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}

func ratio(_ a: Int64?, _ b: Int64?) -> Double? {
    guard let a, let b, b > 0 else { return nil }
    return Double(a) / Double(b)
}
