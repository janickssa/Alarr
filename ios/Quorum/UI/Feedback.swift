import QuorumCore
import SwiftUI
import UIKit

/// Haptics for action results.
enum Haptics {
    static func success() { UINotificationFeedbackGenerator().notificationOccurred(.success) }
    static func error() { UINotificationFeedbackGenerator().notificationOccurred(.error) }
    static func warning() { UINotificationFeedbackGenerator().notificationOccurred(.warning) }
    static func tap() { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
}

/// A toast: "Rebooting sonarr…", then "sonarr rebooted in 18 s".
struct Toast: Identifiable, Equatable {
    enum Tone { case info, progress, success, error }
    let id = UUID()
    var tone: Tone
    var text: String
}

@MainActor
final class Toasts: ObservableObject {
    @Published private(set) var items: [Toast] = []

    @discardableResult
    func show(_ text: String, tone: Toast.Tone = .info) -> UUID {
        let t = Toast(tone: tone, text: text)
        items.append(t)
        if items.count > 3 { items.removeFirst(items.count - 3) }
        switch tone {
        case .success: Haptics.success()
        case .error: Haptics.error()
        default: break
        }
        if tone != .progress { dismiss(t.id, after: tone == .error ? 6 : 3.5) }
        return t.id
    }

    /// Turn a progress toast into its result.
    func update(_ id: UUID, text: String, tone: Toast.Tone) {
        guard let i = items.firstIndex(where: { $0.id == id }) else {
            show(text, tone: tone)
            return
        }
        items[i].text = text
        items[i].tone = tone
        if tone == .success { Haptics.success() } else if tone == .error { Haptics.error() }
        dismiss(id, after: tone == .error ? 6 : 3.5)
    }

    func dismiss(_ id: UUID, after seconds: Double = 0) {
        Task { @MainActor in
            if seconds > 0 { try? await Task.sleep(nanoseconds: UInt64(seconds * 1e9)) }
            withAnimation { items.removeAll { $0.id == id } }
        }
    }
}

struct ToastOverlay: View {
    @ObservedObject var toasts: Toasts

    var body: some View {
        VStack(spacing: 8) {
            ForEach(toasts.items) { t in
                HStack(spacing: 10) {
                    switch t.tone {
                    case .progress: ProgressView().controlSize(.small)
                    case .success: Image(systemName: "checkmark.circle.fill").foregroundStyle(Theme.healthy)
                    case .error: Image(systemName: "xmark.octagon.fill").foregroundStyle(Theme.down)
                    case .info: Image(systemName: "info.circle.fill").foregroundStyle(Theme.accent)
                    }
                    Text(t.text).font(.callout).foregroundStyle(Theme.text)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 11)
                .background(Theme.raised, in: RoundedRectangle(cornerRadius: 12))
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(Theme.border, lineWidth: 0.5))
                .shadow(color: .black.opacity(0.35), radius: 12, y: 4)
                .transition(.move(edge: .bottom).combined(with: .opacity))
                .onTapGesture { toasts.dismiss(t.id) }
                .accessibilityAddTraits(.updatesFrequently)
            }
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 58)
        .animation(.spring(duration: 0.3), value: toasts.items)
    }
}

/// Confirmation sheet for power actions and other risky actions: a normal one (Shutdown,
/// Reboot) and a destructive one (Stop, Delete) with a safer alternative.
struct ConfirmRequest: Identifiable {
    let id = UUID()
    var title: String
    var message: String
    var confirmLabel: String
    var destructive: Bool
    /// e.g. "Shutdown instead" on a hard stop.
    var alternativeLabel: String? = nil
    var alternative: (() -> Void)? = nil
    var toggleLabel: String? = nil
    var confirm: (_ toggled: Bool) -> Void
}

struct ConfirmSheet: View {
    let request: ConfirmRequest
    @Environment(\.dismiss) private var dismiss
    @State private var toggled = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                Image(systemName: request.destructive ? "exclamationmark.triangle.fill" : "power")
                    .foregroundStyle(request.destructive ? Theme.down : Theme.accent)
                    .font(.title3)
                Text(request.title).font(.headline)
            }
            Text(request.message).font(.callout).foregroundStyle(Theme.muted)
            if let label = request.toggleLabel {
                Toggle(label, isOn: $toggled).tint(Theme.down)
            }
            VStack(spacing: 10) {
                Button(role: request.destructive ? .destructive : nil) {
                    dismiss()
                    request.confirm(toggled)
                } label: {
                    Text(request.confirmLabel).frame(maxWidth: .infinity, minHeight: 32)
                }
                .buttonStyle(.borderedProminent)
                .tint(request.destructive ? Theme.down : Theme.accent)
                if let alt = request.alternativeLabel, let run = request.alternative {
                    Button {
                        dismiss()
                        run()
                    } label: {
                        Text(alt).frame(maxWidth: .infinity, minHeight: 32)
                    }
                    .buttonStyle(.bordered)
                }
                Button("Cancel", role: .cancel) { dismiss() }
                    .frame(maxWidth: .infinity, minHeight: 32)
            }
        }
        .padding(20)
        .presentationDetents([.height(request.toggleLabel == nil ? 280 : 330)])
        .presentationDragIndicator(.visible)
        .presentationBackground(Theme.surface)
    }
}
