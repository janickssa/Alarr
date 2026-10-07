//! Alert rules: which conditions are true right now, given the cached snapshots.
//!
//! A pure function so it is easy to test. The engine turns new conditions into alerts and
//! resolves alerts whose condition is gone.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use super::AlertType;
use crate::modules::http::fmt_bytes;
use crate::modules::proxmox::{GuestStatus, GuestType};
use crate::modules::qbittorrent::TorrentState;
use crate::modules::{Health, ModuleData, ServiceSnapshot};

/// Disks at or above this fraction are "full".
pub const DISK_FULL: f64 = 0.9;
/// Down and offline must last this long before alerting (no alerts for blips).
pub const DOWN_DELAY_S: i64 = 60;
/// Only Seerr requests newer than this are announced (no flood on first start).
pub const NEW_REQUEST_WINDOW_H: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub alert_type: AlertType,
    /// Deduplication key: one active alert per key.
    pub key: String,
    pub service_id: Option<String>,
    pub severity: Severity,
    pub title: String,
    pub message: String,
    /// How long the condition must hold before alerting, seconds.
    pub delay_s: i64,
    /// Never alert twice for this key (new requests).
    pub once: bool,
}

/// Conditions true now, plus the services whose data is stale (their data-based alerts are
/// neither raised nor resolved until fresh data arrives).
pub fn evaluate(
    snapshots: &[ServiceSnapshot],
    names: &HashMap<String, String>,
    now: DateTime<Utc>,
) -> (Vec<Condition>, Vec<String>) {
    let mut out = Vec::new();
    let mut stale = Vec::new();
    for snap in snapshots {
        let id = snap.service_id.as_str();
        let name = names.get(id).map(String::as_str).unwrap_or(id);
        let cond =
            |t: AlertType, key: String, sev: Severity, title: String, message: String| Condition {
                alert_type: t,
                key,
                service_id: Some(id.to_owned()),
                severity: sev,
                title,
                message,
                delay_s: 0,
                once: false,
            };
        if snap.health == Health::Down {
            out.push(Condition {
                delay_s: DOWN_DELAY_S,
                ..cond(
                    AlertType::ServiceDown,
                    format!("service_down:{id}"),
                    Severity::Critical,
                    format!("{name} is down"),
                    snap.health_reason
                        .clone()
                        .unwrap_or_else(|| "not responding".into()),
                )
            });
        }
        if snap.stale {
            stale.push(id.to_owned());
            continue;
        }
        match &snap.data {
            ModuleData::Proxmox(d) => {
                for n in &d.nodes {
                    if !n.online {
                        out.push(Condition {
                            delay_s: DOWN_DELAY_S,
                            ..cond(
                                AlertType::NodeOffline,
                                format!("node_offline:{}", n.name),
                                Severity::Critical,
                                format!("Node {} is offline", n.name),
                                "The Proxmox cluster lost contact with it".into(),
                            )
                        });
                    }
                }
                let mut disk =
                    |key: String, label: String, used: Option<u64>, total: Option<u64>| {
                        if let (Some(u), Some(t)) = (used, total)
                            && t > 0
                            && u as f64 / t as f64 >= DISK_FULL
                        {
                            out.push(cond(
                                AlertType::DiskFull,
                                format!("disk_full:{key}"),
                                Severity::Warning,
                                format!("{label} is {:.0} % full", u as f64 / t as f64 * 100.0),
                                format!(
                                    "{} free of {}",
                                    fmt_bytes(t.saturating_sub(u)),
                                    fmt_bytes(t)
                                ),
                            ));
                        }
                    };
                for s in d.storage.iter().filter(|s| s.active) {
                    disk(s.id.clone(), format!("Storage {}", s.id), s.used, s.total);
                }
                for n in d.nodes.iter().filter(|n| n.online) {
                    disk(
                        format!("node:{}", n.name),
                        format!("{} root disk", n.name),
                        n.disk_used,
                        n.disk_total,
                    );
                }
                // Container root filesystems (VM disk usage isn't reported without the agent).
                for g in d.guests.iter().filter(|g| {
                    !g.template
                        && g.guest_type == GuestType::Lxc
                        && g.status == GuestStatus::Running
                }) {
                    disk(
                        format!("guest:{}", g.vmid),
                        format!("CT {} {} disk", g.vmid, g.name),
                        g.disk_used,
                        g.disk_total,
                    );
                }
            }
            ModuleData::Arr(d) => {
                for disk in &d.disks {
                    if disk.total > 0
                        && (disk.total - disk.free.min(disk.total)) as f64 / disk.total as f64
                            >= DISK_FULL
                    {
                        let used = disk.total - disk.free.min(disk.total);
                        out.push(cond(
                            AlertType::DiskFull,
                            format!("disk_full:{id}:{}", disk.path),
                            Severity::Warning,
                            format!(
                                "{} on {name} is {:.0} % full",
                                disk.path,
                                used as f64 / disk.total as f64 * 100.0
                            ),
                            format!("{} free", fmt_bytes(disk.free)),
                        ));
                    }
                }
                for q in &d.queue {
                    let failed = q.tracked_status.as_deref() == Some("error")
                        || q.status.eq_ignore_ascii_case("failed");
                    if failed {
                        let title = match &q.subtitle {
                            Some(s) => format!("{} · {s}", q.title),
                            None => q.title.clone(),
                        };
                        out.push(cond(
                            AlertType::DownloadFailed,
                            format!("download_failed:{id}:{}", q.id),
                            Severity::Warning,
                            format!("Download failed: {title}"),
                            q.message
                                .clone()
                                .unwrap_or_else(|| format!("{name} reports an error")),
                        ));
                    }
                }
            }
            ModuleData::Qbittorrent(d) => {
                for t in d
                    .torrents
                    .iter()
                    .filter(|t| t.state == TorrentState::Errored)
                {
                    out.push(cond(
                        AlertType::DownloadFailed,
                        format!("download_failed:{id}:{}", t.hash),
                        Severity::Warning,
                        format!("Torrent error: {}", t.name),
                        format!("qBittorrent state {}", t.raw_state),
                    ));
                }
            }
            ModuleData::Npm(d) => {
                for c in d.expiring() {
                    let days = c.days_left.unwrap_or(0);
                    out.push(cond(
                        AlertType::CertificateExpiring,
                        format!("certificate_expiring:{id}:{}", c.id),
                        if days < crate::modules::npm::CERT_CRITICAL_DAYS {
                            Severity::Critical
                        } else {
                            Severity::Warning
                        },
                        if days < 0 {
                            format!("Certificate for {} expired", c.name)
                        } else {
                            format!("Certificate for {} expires in {days} d", c.name)
                        },
                        format!(
                            "Used by {} proxy host{}. Check renewal in Nginx Proxy Manager.",
                            c.hosts,
                            if c.hosts == 1 { "" } else { "s" }
                        ),
                    ));
                }
            }
            ModuleData::Seerr(d) => {
                for r in d.pending.iter().filter(|r| {
                    r.requested_at
                        .is_some_and(|at| now - at < Duration::hours(NEW_REQUEST_WINDOW_H))
                }) {
                    let year = r.year.map(|y| format!(" ({y})")).unwrap_or_default();
                    out.push(Condition {
                        once: true,
                        ..cond(
                            AlertType::NewRequest,
                            format!("new_request:{id}:{}", r.id),
                            Severity::Info,
                            format!("New request: {}{year}", r.title),
                            format!(
                                "{} requested a {}. Approve or decline in {name}.",
                                r.requested_by,
                                if r.media_type == "tv" {
                                    "series"
                                } else {
                                    "movie"
                                }
                            ),
                        )
                    });
                }
            }
            _ => {}
        }
    }
    (out, stale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ServiceKind;
    use crate::modules::npm::{Certificate, NpmData};

    fn snap(id: &str, health: Health, data: ModuleData) -> ServiceSnapshot {
        ServiceSnapshot {
            service_id: id.into(),
            kind: ServiceKind::HealthCheck,
            health,
            health_reason: Some("connection refused".into()),
            response_ms: None,
            updated_at: Utc::now(),
            version: None,
            stale: false,
            data,
        }
    }

    #[test]
    fn down_services_and_expiring_certificates() {
        let names: HashMap<String, String> = [("drawio".into(), "draw.io".into())].into();
        let npm = NpmData {
            hosts: vec![],
            certificates: vec![
                Certificate {
                    id: 4,
                    name: "docs.example.net".into(),
                    domains: vec![],
                    provider: "letsencrypt".into(),
                    expires_at: None,
                    days_left: Some(2),
                    hosts: 1,
                },
                Certificate {
                    id: 5,
                    name: "unused".into(),
                    domains: vec![],
                    provider: "other".into(),
                    expires_at: None,
                    days_left: Some(1),
                    hosts: 0,
                },
            ],
        };
        let (c, stale) = evaluate(
            &[
                snap("drawio", Health::Down, ModuleData::None),
                snap("npm", Health::Warning, ModuleData::Npm(Box::new(npm))),
            ],
            &names,
            Utc::now(),
        );
        assert!(stale.is_empty());
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].key, "service_down:drawio");
        assert_eq!(c[0].title, "draw.io is down");
        assert_eq!(c[0].delay_s, DOWN_DELAY_S);
        assert_eq!(c[1].key, "certificate_expiring:npm:4");
        assert_eq!(c[1].severity, Severity::Critical);
    }

    #[test]
    fn stale_data_is_frozen() {
        let mut s = snap("npm", Health::Down, ModuleData::None);
        s.stale = true;
        let (c, stale) = evaluate(&[s], &HashMap::new(), Utc::now());
        assert_eq!(c.len(), 1);
        assert_eq!(stale, ["npm"]);
    }
}
