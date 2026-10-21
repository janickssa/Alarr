import Foundation
#if canImport(SwiftUI)
import SwiftUI

/// Colours from the design tokens (`design/_ds/…/styles.css`). Dark only, like the web
/// app, until the designer delivers a light theme.
public enum Theme {
    public static let bg = Color(hex: 0x161826)
    public static let surface = Color(hex: 0x232532)
    public static let raised = Color(hex: 0x292B31)
    public static let border = Color(hex: 0x3F424D)
    public static let text = Color(hex: 0xE9E9ED)
    public static let muted = Color(hex: 0xE9E9ED).opacity(0.55)
    public static let accent = Color(hex: 0x9184D9)
    public static let accent2 = Color(hex: 0xA7A1DB)
    public static let section = Color(hex: 0x262A60)
    public static let neutral500 = Color(hex: 0x9397AB)

    public static let healthy = Color(oklch: 0.78, 0.13, 155)
    public static let healthyFill = Color(oklch: 0.3, 0.05, 155)
    public static let warning = Color(oklch: 0.83, 0.13, 80)
    public static let warningFill = Color(oklch: 0.32, 0.055, 80)
    public static let down = Color(oklch: 0.72, 0.16, 25)
    public static let downFill = Color(oklch: 0.31, 0.07, 25)
    public static let progress = Color(oklch: 0.77, 0.11, 240)
    public static let progressFill = Color(oklch: 0.31, 0.05, 240)
    public static let neutral = Color(hex: 0x9397AB)
    public static let neutralFill = Color(hex: 0x2F313A)

    /// A service's accent tile colour from its OKLCH hue (lightness as in the design).
    public static func accent(hue: Double, chroma: Double = 0.045) -> Color {
        Color(oklch: 0.42, max(chroma, 0.045) * 2.2, hue)
    }

    /// Foreground and fill for a health value (`healthy`, `warning`, `down`, `in_progress`,
    /// `disabled`, `unknown`).
    public static func health(_ raw: String) -> (fg: Color, fill: Color) {
        switch raw {
        case "healthy": (healthy, healthyFill)
        case "warning": (warning, warningFill)
        case "down": (down, downFill)
        case "in_progress": (progress, progressFill)
        default: (neutral, neutralFill)
        }
    }

    /// Resource bars: normal below 75 %, warning from 75 %, down from 90 %.
    public static func usage(_ fraction: Double) -> Color {
        fraction >= 0.9 ? down : fraction >= 0.75 ? warning : accent
    }
}

public extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }

    /// OKLCH (as in the design's CSS) converted to sRGB.
    init(oklch l: Double, _ c: Double, _ h: Double) {
        let (r, g, b) = OKLCH.toSRGB(l: l, c: c, h: h)
        self.init(.sRGB, red: r, green: g, blue: b, opacity: 1)
    }
}
#endif

public enum OKLCH {
    /// Björn Ottosson's OKLab → linear sRGB, then gamma; out-of-gamut values are clipped.
    public static func toSRGB(l: Double, c: Double, h: Double) -> (Double, Double, Double) {
        let hr = h * .pi / 180
        let a = c * cos(hr), b = c * sin(hr)
        let l_ = l + 0.3963377774 * a + 0.2158037573 * b
        let m_ = l - 0.1055613458 * a - 0.0638541728 * b
        let s_ = l - 0.0894841775 * a - 1.2914855480 * b
        let l3 = l_ * l_ * l_, m3 = m_ * m_ * m_, s3 = s_ * s_ * s_
        let r = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3
        let g = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3
        let bl = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3
        func gamma(_ x: Double) -> Double {
            let v = x <= 0.0031308 ? 12.92 * x : 1.055 * pow(x, 1 / 2.4) - 0.055
            return min(max(v, 0), 1)
        }
        return (gamma(r), gamma(g), gamma(bl))
    }
}

/// Phosphor icon names from the API mapped to SF Symbols (the app uses SF Symbols rather
/// than bundling Phosphor; see the decisions log).
public enum Symbols {
    static let map: [String: String] = [
        "hard-drives": "server.rack",
        "film-strip": "film",
        "ticket": "ticket",
        "television-simple": "tv",
        "film-slate": "movieclapper",
        "binoculars": "binoculars",
        "subtitles": "captions.bubble",
        "arrows-down-up": "arrow.up.arrow.down",
        "music-notes": "music.note.list",
        "microphone": "mic",
        "funnel": "line.3.horizontal.decrease.circle",
        "arrows-split": "arrow.triangle.branch",
        "fingerprint": "touchid",
        "plugs-connected": "network",
        "vault": "lock.shield",
        "cloud": "cloud",
        "arrows-clockwise": "arrow.clockwise",
        "package": "shippingbox",
        "folder-simple": "folder",
        "books": "books.vertical",
        "flow-arrow": "point.3.connected.trianglepath.dotted",
        "book-open-text": "book",
        "file-text": "doc.text",
        "puzzle-piece": "puzzlepiece",
        "cube": "cube",
        "desktop": "desktopcomputer",
        "heartbeat": "waveform.path.ecg",
        "pulse": "waveform.path.ecg",
        "bell": "bell",
        "gear": "gearshape",
        "house": "house",
    ]

    /// SF Symbol for a Phosphor name (with or without the `ph-` prefix).
    public static func sf(_ phosphor: String) -> String {
        var name = phosphor
        if name.hasPrefix("ph-") { name.removeFirst(3) }
        return map[name] ?? "square.grid.2x2"
    }
}
