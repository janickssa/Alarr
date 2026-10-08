import QuorumCore
import SwiftUI

@main
struct QuorumApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AppModel()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
                .tint(Theme.accent)
                .onOpenURL { model.handle(url: $0) }
                .task { await model.start() }
                .onChange(of: phase) { _, p in
                    switch p {
                    case .active:
                        if model.credentials != nil { Task { await model.start() } }
                    case .background:
                        model.disconnect()
                        Task { await model.refreshWidget() }
                    default:
                        break
                    }
                }
        }
    }
}

struct RootView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        ZStack(alignment: .bottom) {
            Theme.bg.ignoresSafeArea()
            if model.credentials == nil {
                PairingView(link: model.pendingPairing)
            } else if model.me == nil {
                if model.offline {
                    OfflineView()
                } else {
                    VStack(spacing: 16) {
                        LogoMark(size: 56)
                        Text("Connecting to \(model.credentials?.server.host() ?? "Quorum")…")
                            .font(.callout.monospaced()).foregroundStyle(Theme.muted)
                    }
                }
            } else {
                MainTabs()
            }
            ToastOverlay(toasts: model.toasts)
        }
        .sheet(item: $model.confirm) { ConfirmSheet(request: $0) }
    }
}

struct OfflineView: View {
    @EnvironmentObject var model: AppModel
    var body: some View {
        VStack(spacing: 18) {
            LogoMark(size: 56)
            Text("Can't reach Quorum").font(.title2.weight(.medium))
            Text("\(model.credentials?.server.host() ?? "The server") isn't answering. Are you on Tailscale (or your VPN)? Is the server running?")
                .font(.callout).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
                .padding(.horizontal, 32)
            Button {
                Task { await model.start() }
            } label: {
                Label("Try again", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.borderedProminent)
        }
    }
}

/// The Quorum mark from the design: three nodes and a centre point.
struct LogoMark: View {
    var size: CGFloat = 32
    var body: some View {
        Canvas { ctx, sz in
            let s = sz.width / 48
            let p = { (x: CGFloat, y: CGFloat) in CGPoint(x: x * s, y: y * s) }
            var tri = Path()
            tri.move(to: p(24, 10)); tri.addLine(to: p(11, 34)); tri.addLine(to: p(37, 34)); tri.closeSubpath()
            ctx.stroke(tri, with: .color(Color(hex: 0xB5ABFC).opacity(0.6)), style: StrokeStyle(lineWidth: 2.2 * s, lineJoin: .round))
            for c in [p(24, 10), p(11, 34), p(37, 34)] {
                let r = CGRect(x: c.x - 5 * s, y: c.y - 5 * s, width: 10 * s, height: 10 * s)
                ctx.fill(Path(ellipseIn: r), with: .color(Color(hex: 0x262A60)))
                ctx.stroke(Path(ellipseIn: r), with: .color(Color(hex: 0xE7E5FE)), lineWidth: 2.4 * s)
            }
            let c = p(24, 26)
            ctx.fill(Path(ellipseIn: CGRect(x: c.x - 3.4 * s, y: c.y - 3.4 * s, width: 6.8 * s, height: 6.8 * s)), with: .color(Color(hex: 0xB5ABFC)))
        }
        .frame(width: size, height: size)
        .shadow(color: Color(hex: 0xB5ABFC).opacity(0.45), radius: size / 6)
        .accessibilityLabel("Quorum")
    }
}

struct MainTabs: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        TabView(selection: $model.tab) {
            stack(.home) { HomeView() }
                .tabItem { Label("Home", systemImage: "house") }
                .tag(Tab.home)
            if model.hasProxmox {
                stack(.proxmox) { ProxmoxView() }
                    .tabItem { Label("Proxmox", systemImage: "server.rack") }
                    .tag(Tab.proxmox)
            }
            if model.hasMedia {
                stack(.media) { MediaView() }
                    .tabItem { Label("Media", systemImage: "play.rectangle") }
                    .tag(Tab.media)
            }
            stack(.services) { ServicesView() }
                .tabItem { Label("Services", systemImage: "square.grid.2x2") }
                .tag(Tab.services)
            stack(.more) { MoreView() }
                .tabItem { Label("More", systemImage: "ellipsis.circle") }
                .tag(Tab.more)
        }
    }

    func stack<V: View>(_ tab: Tab, @ViewBuilder _ root: () -> V) -> some View {
        NavigationStack(path: model.binding(for: tab)) {
            root().navigationDestination(for: Route.self) { RouteView(route: $0) }
        }
    }
}

struct RouteView: View {
    let route: Route
    var body: some View {
        switch route {
        case .service(let id): ServiceDetailView(id: id)
        case .guest(let vmid): GuestDetailView(vmid: vmid)
        case .node(let name): NodeDetailView(name: name)
        case .health: HealthView()
        case .activity: ActivityView()
        case .alerts: AlertsView()
        case .settings: SettingsView()
        case .notifications: NotificationSettingsView()
        case .devices: DevicesView()
        case .manageServices: ManageServicesView()
        case .addService: AddServiceView()
        }
    }
}
