import Foundation

/// Units as the README asks: binary multiples labelled GB/TB (as Proxmox shows them),
/// MB/s, a space before `%`, 24-hour time and dates like `7 Oct 2026`.
public enum Format {
    static let units = ["B", "KB", "MB", "GB", "TB", "PB"]

    public static func bytes(_ value: Int64?) -> String {
        guard let value else { return "–" }
        var v = Double(value)
        var i = 0
        while v >= 1024, i < units.count - 1 {
            v /= 1024
            i += 1
        }
        return "\(number(v, digits: i == 0 ? 0 : (v < 10 ? 1 : 0))) \(units[i])"
    }

    /// Bytes per second as MB/s (KB/s below 1 MB/s).
    public static func rate(_ bytesPerSecond: Int64?) -> String {
        guard let b = bytesPerSecond else { return "–" }
        let v = Double(b)
        if v < 1024 * 1024 { return "\(number(v / 1024, digits: 0)) KB/s" }
        let mb = v / 1024 / 1024
        return "\(number(mb, digits: mb < 100 ? 1 : 0)) MB/s"
    }

    /// Bits per second as Mbit/s.
    public static func bitrate(_ bitsPerSecond: Int64?) -> String {
        guard let b = bitsPerSecond else { return "–" }
        let m = Double(b) / 1_000_000
        return "\(number(m, digits: m < 100 ? 1 : 0)) Mbit/s"
    }

    /// A 0.0–1.0 fraction as `12 %`.
    public static func percent(_ fraction: Double?, digits: Int = 0) -> String {
        guard let f = fraction, f.isFinite else { return "–" }
        return "\(number(f * 100, digits: digits)) %"
    }

    /// An uptime percentage that is already 0–100 (`99.9 %`).
    public static func uptime(_ pct: Double?) -> String {
        guard let p = pct else { return "–" }
        return "\(number(p, digits: p >= 99.95 || p == 0 ? 0 : 1)) %"
    }

    public static func ms(_ value: Int64?) -> String {
        guard let v = value else { return "–" }
        return v >= 1000 ? "\(number(Double(v) / 1000, digits: 1)) s" : "\(v) ms"
    }

    /// `2 h 5 min`, `45 s`, `3 d 4 h`.
    public static func duration(_ seconds: Int64?) -> String {
        guard let s = seconds, s >= 0 else { return "–" }
        let d = s / 86_400, h = s % 86_400 / 3600, m = s % 3600 / 60
        if d > 0 { return h > 0 ? "\(d) d \(h) h" : "\(d) d" }
        if h > 0 { return m > 0 ? "\(h) h \(m) min" : "\(h) h" }
        if m > 0 { return "\(m) min" }
        return "\(s) s"
    }

    public static func duration(_ seconds: Double?) -> String {
        duration(seconds.map { Int64($0) })
    }

    nonisolated(unsafe) static let timeFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_GB")
        f.dateFormat = "HH:mm"
        return f
    }()

    nonisolated(unsafe) static let dateFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_GB")
        f.dateFormat = "d MMM yyyy"
        return f
    }()

    /// `18:47`
    public static func time(_ date: Date?) -> String {
        guard let date else { return "–" }
        return timeFormatter.string(from: date)
    }

    /// `7 Oct 2026`
    public static func date(_ date: Date?) -> String {
        guard let date else { return "–" }
        return dateFormatter.string(from: date)
    }

    /// `18:47` today, `7 Oct 2026, 18:47` otherwise.
    public static func dateTime(_ date: Date?, now: Date = Date()) -> String {
        guard let date else { return "–" }
        if Calendar.current.isDate(date, inSameDayAs: now) { return time(date) }
        return "\(self.date(date)), \(time(date))"
    }

    /// `just now`, `4 min ago`, `3 h ago`, then the date.
    public static func ago(_ date: Date?, now: Date = Date()) -> String {
        guard let date else { return "–" }
        let s = Int64(now.timeIntervalSince(date))
        if s < 45 { return "just now" }
        if s < 3600 { return "\(max(1, s / 60)) min ago" }
        if s < 86_400 { return "\(s / 3600) h ago" }
        return self.date(date)
    }

    public static func celsius(_ value: Double?) -> String {
        guard let v = value else { return "–" }
        return "\(number(v, digits: 0)) °C"
    }

    static func number(_ v: Double, digits: Int) -> String {
        String(format: "%.\(digits)f", v)
    }
}
