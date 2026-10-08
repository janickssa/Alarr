import QuorumCore
import SwiftUI

/// Coloured dot for a health value.
struct HealthDot: View {
    let health: String
    var size: CGFloat = 8

    var body: some View {
        Circle()
            .fill(Theme.health(health).fg)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

/// Status badge ("Healthy", "Down" …) as in the design's status badges.
struct HealthBadge: View {
    let health: String

    var label: String {
        switch health {
        case "healthy": "Healthy"
        case "warning": "Warning"
        case "down": "Down"
        case "in_progress": "In progress"
        case "disabled": "Disabled"
        default: "Unknown"
        }
    }

    var body: some View {
        let c = Theme.health(health)
        HStack(spacing: 5) {
            Circle().fill(c.fg).frame(width: 6, height: 6)
            Text(label)
        }
        .font(.caption.weight(.medium))
        .foregroundStyle(c.fg)
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(c.fill, in: Capsule())
        .accessibilityElement(children: .combine)
    }
}

/// Usage bar with the design's thresholds (75 % warning, 90 % down).
struct ResourceBar: View {
    let label: String
    let fraction: Double?
    var detail: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(label).font(.caption).foregroundStyle(Theme.muted)
                Spacer()
                Text(detail ?? Format.percent(fraction))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(Theme.text)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Theme.raised)
                    Capsule()
                        .fill(Theme.usage(fraction ?? 0))
                        .frame(width: geo.size.width * min(max(fraction ?? 0, 0), 1))
                }
            }
            .frame(height: 6)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityValue(detail ?? Format.percent(fraction))
    }
}

/// Big number with a caption.
struct StatTile: View {
    let title: String
    let value: String
    var sub: String? = nil
    var tint: Color = Theme.text

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased())
                .font(.caption2.weight(.medium))
                .foregroundStyle(Theme.muted)
            Text(value)
                .font(.title2.weight(.medium).monospacedDigit())
                .foregroundStyle(tint)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            if let sub {
                Text(sub).font(.caption).foregroundStyle(Theme.muted).lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14))
        .accessibilityElement(children: .combine)
    }
}

/// Service icon on its accent tile.
struct ServiceIcon: View {
    let icon: String
    let hue: Double
    var chroma: Double = 0.045
    var size: CGFloat = 32

    var body: some View {
        RoundedRectangle(cornerRadius: size * 0.28)
            .fill(Theme.accent(hue: hue, chroma: chroma))
            .frame(width: size, height: size)
            .overlay(
                Image(systemName: Symbols.sf(icon))
                    .font(.system(size: size * 0.5, weight: .medium))
                    .foregroundStyle(.white.opacity(0.92))
            )
            .accessibilityHidden(true)
    }
}

/// Section header in the design's overline style.
struct SectionTitle: View {
    let text: String
    var body: some View {
        Text(text.uppercased())
            .font(.caption.weight(.semibold))
            .foregroundStyle(Theme.muted)
    }
}

/// Card container.
struct Card<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        VStack(alignment: .leading, spacing: 10) { content }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14))
    }
}

/// Empty, loading and error states.
struct StateView: View {
    enum Kind { case loading, empty(String), error(String, retry: () -> Void) }
    let kind: Kind

    var body: some View {
        switch kind {
        case .loading:
            ProgressView().frame(maxWidth: .infinity, minHeight: 120)
        case .empty(let text):
            Text(text)
                .font(.callout)
                .foregroundStyle(Theme.muted)
                .frame(maxWidth: .infinity, minHeight: 80)
        case .error(let text, let retry):
            VStack(spacing: 10) {
                Image(systemName: "exclamationmark.triangle").foregroundStyle(Theme.warning)
                Text(text).font(.callout).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
                Button("Try again", action: retry).buttonStyle(.bordered)
            }
            .frame(maxWidth: .infinity, minHeight: 120)
        }
    }
}

/// A label + value row.
struct KeyValueRow: View {
    let key: String
    let value: String
    var mono = false

    var body: some View {
        HStack {
            Text(key).foregroundStyle(Theme.muted)
            Spacer()
            Text(value)
                .font(mono ? .callout.monospaced() : .callout)
                .multilineTextAlignment(.trailing)
        }
        .accessibilityElement(children: .combine)
    }
}

extension View {
    /// The app's dark background on scroll views and lists.
    func quorumBackground() -> some View {
        scrollContentBackground(.hidden).background(Theme.bg)
    }
}
