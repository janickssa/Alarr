import OpenAPIRuntime
import QuorumCore
import SwiftUI

let categoryOrder = ["infrastructure", "media", "network_security", "apps"]
let categoryLabel = [
    "infrastructure": "Infrastructure", "media": "Media",
    "network_security": "Network and security", "apps": "Apps",
]

struct ServicesView: View {
    @EnvironmentObject var model: AppModel
    @State private var search = ""

    var body: some View {
        List {
            ForEach(categoryOrder, id: \.self) { cat in
                let list = model.services
                    .filter { $0.enabled && $0.category.rawValue == cat }
                    .filter { search.isEmpty || $0.name.localizedCaseInsensitiveContains(search) }
                    .sorted { ($0.health.rank, $0.sortOrder, $0.name) < ($1.health.rank, $1.sortOrder, $1.name) }
                // A category only appears when it has an enabled service.
                if !list.isEmpty {
                    Section(categoryLabel[cat] ?? cat) {
                        ForEach(list, id: \.id) { s in
                            NavigationLink(value: Route.service(s.id)) { ServiceRow(service: s) }
                        }
                    }
                }
            }
            if model.services.filter(\.enabled).isEmpty {
                StateView(kind: .empty("No services yet")).listRowBackground(Color.clear)
            }
        }
        .quorumBackground()
        .navigationTitle("Services")
        .searchable(text: $search)
        .refreshable { await model.refresh() }
        .toolbar {
            if model.isOwner {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink(value: Route.manageServices) { Image(systemName: "slider.horizontal.3") }
                        .accessibilityLabel("Manage services")
                }
            }
        }
    }
}

struct ServiceRow: View {
    let service: S.ServiceView
    var body: some View {
        HStack(spacing: 12) {
            ServiceIcon(icon: service.icon, hue: Double(service.accentHue), chroma: Double(service.accentChroma))
            VStack(alignment: .leading, spacing: 2) {
                Text(service.name).font(.callout.weight(.medium))
                Text(service.healthReason ?? service.version.map { "v\($0)" } ?? service.kind.rawValue.replacingOccurrences(of: "_", with: " "))
                    .font(.caption).foregroundStyle(Theme.muted).lineLimit(1)
            }
            Spacer()
            if let ms = service.responseMs {
                Text(Format.ms(ms)).font(.caption.monospacedDigit()).foregroundStyle(Theme.muted)
            }
            HealthDot(health: service.health.rawValue)
        }
        .accessibilityElement(children: .combine)
        .accessibilityValue(service.health.rawValue)
    }
}

/// Owner: enable/disable and delete services, add new ones.
struct ManageServicesView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        List {
            Section {
                NavigationLink(value: Route.addService) { Label("Add service", systemImage: "plus") }
            }
            ForEach(categoryOrder, id: \.self) { cat in
                let list = model.services.filter { $0.category.rawValue == cat }.sorted { $0.name < $1.name }
                if !list.isEmpty {
                    Section(categoryLabel[cat] ?? cat) {
                        ForEach(list, id: \.id) { s in
                            Toggle(isOn: Binding(get: { s.enabled }, set: { on in Task { await toggle(s, on) } })) {
                                HStack(spacing: 10) {
                                    ServiceIcon(icon: s.icon, hue: Double(s.accentHue), chroma: Double(s.accentChroma), size: 26)
                                    Text(s.name)
                                }
                            }
                            .swipeActions {
                                Button(role: .destructive) { delete(s) } label: { Label("Delete", systemImage: "trash") }
                            }
                        }
                    }
                }
            }
        }
        .quorumBackground()
        .navigationTitle("Manage services")
        .navigationBarTitleDisplayMode(.inline)
    }

    func toggle(_ s: S.ServiceView, _ on: Bool) async {
        guard let client = model.client else { return }
        do {
            if on {
                let out = try await client.enable(path: .init(id: s.id))
                guard case .ok = out else { throw APIFailure(output: out) }
            } else {
                let out = try await client.disable(path: .init(id: s.id))
                guard case .ok = out else { throw APIFailure(output: out) }
            }
            Haptics.tap()
            await model.loadServices()
        } catch {
            model.toasts.show(describe(error), tone: .error)
        }
    }

    func delete(_ s: S.ServiceView) {
        model.confirm = ConfirmRequest(
            title: "Delete \(s.name)?",
            message: "Its settings, secrets and history are removed. The service itself is not touched.",
            confirmLabel: "Delete", destructive: true
        ) { _ in
            Task {
                guard let client = model.client else { return }
                do {
                    let out = try await client.delete(path: .init(id: s.id))
                    guard case .noContent = out else { throw APIFailure(output: out) }
                    model.toasts.show("\(s.name) deleted", tone: .success)
                    await model.loadServices()
                } catch {
                    model.toasts.show(describe(error), tone: .error)
                }
            }
        }
    }
}

/// Add a service from the catalogue: pick it, fill in the address and secrets, test, save.
struct AddServiceView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var catalogue: [S.CatalogueEntry] = []
    @State private var entry: S.CatalogueEntry?
    @State private var name = ""
    @State private var baseURL = ""
    @State private var apiKey = ""
    @State private var username = ""
    @State private var password = ""
    @State private var tokenID = ""
    @State private var fingerprint = ""
    @State private var testResult: S.ConnectionTest?
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        Form {
            Section("Service") {
                Picker("Type", selection: $entry) {
                    Text("Choose…").tag(S.CatalogueEntry?.none)
                    ForEach(catalogue, id: \.preset) { e in
                        Text(e.name).tag(Optional(e))
                    }
                }
                .onChange(of: entry) { _, e in
                    if let e { name = e.name }
                    testResult = nil
                }
                if entry != nil {
                    TextField("Name", text: $name)
                    TextField(kind == "wireguard" ? "Agent URL (http://host:9587)" : "Address (https://…)", text: $baseURL)
                        .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                }
            }
            if let kind = entry?.kind?.rawValue {
                Section {
                    switch kind {
                    case "proxmox":
                        TextField("Token id (user@realm!token)", text: $tokenID).textInputAutocapitalization(.never)
                        SecureField("Token secret", text: $apiKey)
                        TextField("Certificate SHA-256 fingerprint (optional)", text: $fingerprint)
                            .textInputAutocapitalization(.characters).font(.caption.monospaced())
                    case "qbittorrent", "navidrome", "nginx_proxy_manager":
                        TextField(kind == "nginx_proxy_manager" ? "Email" : "Username", text: $username)
                            .textInputAutocapitalization(.never)
                        SecureField("Password", text: $password)
                    case "nextcloud":
                        TextField("Admin user (empty for serverinfo token)", text: $username).textInputAutocapitalization(.never)
                        SecureField("App password or token", text: $apiKey)
                    case "health_check":
                        EmptyView()
                    default:
                        SecureField(kind == "wireguard" ? "Agent token" : "API key / token", text: $apiKey)
                    }
                } header: {
                    Text("Credentials")
                } footer: {
                    Text("Stored encrypted on the server and never shown again.")
                }
            }
            if entry != nil {
                Section {
                    Button { Task { await test() } } label: {
                        HStack { Text("Test connection"); Spacer(); if busy { ProgressView() } }
                    }
                    .disabled(busy || baseURL.isEmpty)
                    if let t = testResult {
                        Label(t.message, systemImage: t.ok ? "checkmark.circle.fill" : "xmark.octagon.fill")
                            .foregroundStyle(t.ok ? Theme.healthy : Theme.down)
                        if let fp = t.presentedFingerprint {
                            Text("Presented certificate: \(fp)").font(.caption2.monospaced()).foregroundStyle(Theme.muted)
                        }
                        ForEach(t.warnings, id: \.self) { Text($0).font(.caption).foregroundStyle(Theme.warning) }
                    }
                    Button("Save") { Task { await save() } }
                        .disabled(busy || name.isEmpty || baseURL.isEmpty)
                }
            }
            if let error {
                Text(error).foregroundStyle(Theme.down)
            }
        }
        .quorumBackground()
        .navigationTitle("Add service")
        .navigationBarTitleDisplayMode(.inline)
        .task {
            guard let client = model.client else { return }
            catalogue = (try? await client.getCatalogue().ok.body.json.filter { $0.kind != nil }) ?? []
        }
    }

    var kind: String { entry?.kind?.rawValue ?? "" }

    func request() -> S.CreateService? {
        guard let e = entry, let k = e.kind else { return nil }
        var secrets: [String: String] = [:]
        var settings: [String: OpenAPIValueContainer] = [:]
        switch k.rawValue {
        case "proxmox":
            secrets["token_secret"] = apiKey
            settings["token_id"] = try? .init(unvalidatedValue: tokenID)
            if !fingerprint.isEmpty { settings["fingerprint"] = try? .init(unvalidatedValue: fingerprint) }
        case "qbittorrent", "navidrome", "nginx_proxy_manager":
            settings["username"] = try? .init(unvalidatedValue: username)
            secrets["password"] = password
        case "nextcloud":
            if !username.isEmpty { settings["username"] = try? .init(unvalidatedValue: username) }
            secrets["password"] = apiKey
        case "health_check":
            break
        default:
            secrets["api_key"] = apiKey
        }
        return S.CreateService(
            baseUrl: baseURL.trimmingCharacters(in: .whitespaces),
            category: e.category,
            enabled: true,
            kind: k,
            name: name,
            preset: e.preset,
            secrets: secrets.isEmpty ? nil : .init(additionalProperties: secrets),
            settings: settings.isEmpty ? nil : .init(additionalProperties: settings)
        )
    }

    func test() async {
        guard let client = model.client, let r = request() else { return }
        busy = true
        defer { busy = false }
        do {
            let out = try await client.testUnsaved(body: .json(r))
            guard case .ok(let ok) = out else { throw APIFailure(output: out) }
            testResult = try ok.body.json
            if testResult?.ok == true { Haptics.success() } else { Haptics.warning() }
        } catch {
            self.error = describe(error)
        }
    }

    func save() async {
        guard let client = model.client, let r = request() else { return }
        busy = true
        defer { busy = false }
        do {
            let out = try await client.create(body: .json(r))
            guard case .created(let c) = out else { throw APIFailure(output: out) }
            let s = try c.body.json
            model.toasts.show("\(s.name) added", tone: .success)
            await model.loadServices()
            dismiss()
        } catch {
            self.error = describe(error)
        }
    }
}
