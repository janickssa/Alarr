//! The module catalogue: known services with their icon, accent and category.
//! Values come from the design (batch 1, module catalogue).

use serde::Serialize;
use utoipa::ToSchema;

use super::{Category, ServiceKind};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CatalogueEntry {
    /// Preset id, stored on service instances created from this entry.
    pub preset: &'static str,
    pub name: &'static str,
    /// Module kind used for this preset. `None` = module not implemented yet.
    pub kind: Option<ServiceKind>,
    /// Integration level from the design catalogue (full, read-only, health).
    pub level: &'static str,
    pub category: Category,
    /// Phosphor icon name.
    pub icon: &'static str,
    /// OKLCH hue of the service accent.
    pub accent_hue: u16,
    /// OKLCH chroma of the accent tile (only differs for the neutral custom accent).
    pub accent_chroma: f32,
    /// Path appended to the base URL for health checks.
    pub health_path: Option<&'static str>,
    /// Container id in the owner's current setup (demo/seed data only).
    pub demo_vmid: Option<u32>,
}

#[allow(clippy::too_many_arguments)]
const fn entry(
    preset: &'static str,
    name: &'static str,
    kind: Option<ServiceKind>,
    level: &'static str,
    category: Category,
    icon: &'static str,
    accent_hue: u16,
    demo_vmid: Option<u32>,
) -> CatalogueEntry {
    CatalogueEntry {
        preset,
        name,
        kind,
        level,
        category,
        icon,
        accent_hue,
        accent_chroma: 0.045,
        health_path: None,
        demo_vmid,
    }
}

const HC: Option<ServiceKind> = Some(ServiceKind::HealthCheck);
use Category::{Apps, Infrastructure, Media, NetworkSecurity};

pub static CATALOGUE: &[CatalogueEntry] = &[
    entry(
        "proxmox",
        "Proxmox cluster",
        Some(ServiceKind::Proxmox),
        "full",
        Infrastructure,
        "hard-drives",
        45,
        None,
    ),
    entry(
        "jellyfin",
        "Jellyfin",
        Some(ServiceKind::Jellyfin),
        "full",
        Media,
        "film-strip",
        305,
        Some(108),
    ),
    entry(
        "jellyfin-public",
        "Jellyfin (public)",
        Some(ServiceKind::Jellyfin),
        "full",
        Media,
        "film-strip",
        305,
        Some(105),
    ),
    entry(
        "seerr",
        "Seerr",
        Some(ServiceKind::Seerr),
        "full",
        Media,
        "ticket",
        268,
        Some(126),
    ),
    entry(
        "sonarr",
        "Sonarr",
        Some(ServiceKind::Sonarr),
        "full",
        Media,
        "television-simple",
        222,
        Some(123),
    ),
    entry(
        "radarr",
        "Radarr",
        Some(ServiceKind::Radarr),
        "full",
        Media,
        "film-slate",
        75,
        Some(124),
    ),
    entry(
        "prowlarr",
        "Prowlarr",
        Some(ServiceKind::Prowlarr),
        "full",
        Media,
        "binoculars",
        335,
        Some(125),
    ),
    entry(
        "bazarr",
        "Bazarr",
        Some(ServiceKind::Bazarr),
        "full",
        Media,
        "subtitles",
        115,
        Some(127),
    ),
    entry(
        "qbittorrent",
        "qBittorrent",
        Some(ServiceKind::Qbittorrent),
        "full",
        Media,
        "arrows-down-up",
        190,
        Some(120),
    ),
    entry(
        "navidrome",
        "Navidrome",
        Some(ServiceKind::Navidrome),
        "full",
        Media,
        "music-notes",
        10,
        Some(119),
    ),
    entry(
        "podgrab",
        "Podgrab",
        HC,
        "health",
        Media,
        "microphone",
        45,
        Some(118),
    ),
    entry(
        "technitium",
        "Technitium DNS",
        None,
        "full",
        NetworkSecurity,
        "funnel",
        190,
        Some(112),
    ),
    entry(
        "nginx-proxy-manager",
        "Nginx Proxy Manager",
        None,
        "read-only",
        NetworkSecurity,
        "arrows-split",
        75,
        Some(115),
    ),
    entry(
        "authentik",
        "Authentik",
        None,
        "read-only",
        NetworkSecurity,
        "fingerprint",
        10,
        Some(121),
    ),
    entry(
        "wireguard",
        "WireGuard",
        None,
        "read-only",
        NetworkSecurity,
        "plugs-connected",
        222,
        Some(102),
    ),
    CatalogueEntry {
        health_path: Some("/alive"),
        ..entry(
            "vaultwarden",
            "Vaultwarden",
            HC,
            "health",
            NetworkSecurity,
            "vault",
            268,
            Some(114),
        )
    },
    entry(
        "nextcloud",
        "Nextcloud",
        None,
        "read-only",
        Apps,
        "cloud",
        222,
        Some(113),
    ),
    entry(
        "lxc-updater",
        "lxc-updater",
        None,
        "read-only",
        Infrastructure,
        "arrows-clockwise",
        45,
        Some(104),
    ),
    entry(
        "apt-cacher-ng",
        "apt-cacher-ng",
        HC,
        "health",
        Infrastructure,
        "package",
        75,
        Some(103),
    ),
    entry(
        "smb-share",
        "smb-share",
        HC,
        "health",
        Infrastructure,
        "folder-simple",
        115,
        Some(111),
    ),
    entry(
        "bookstack",
        "BookStack",
        HC,
        "health",
        Apps,
        "books",
        115,
        Some(117),
    ),
    entry(
        "drawio",
        "draw.io",
        HC,
        "health",
        Apps,
        "flow-arrow",
        75,
        Some(116),
    ),
    entry(
        "kiwix",
        "Kiwix",
        HC,
        "health",
        Apps,
        "book-open-text",
        190,
        Some(122),
    ),
    entry(
        "mkdocs",
        "MkDocs docs",
        HC,
        "health",
        Apps,
        "file-text",
        268,
        Some(128),
    ),
    CatalogueEntry {
        accent_chroma: 0.015,
        ..entry(
            "custom",
            "Custom service",
            HC,
            "health",
            Apps,
            "puzzle-piece",
            265,
            None,
        )
    },
];

pub fn find(preset: &str) -> Option<&'static CatalogueEntry> {
    CATALOGUE.iter().find(|e| e.preset == preset)
}

/// The catalogue entry to use for display when an instance has no (known) preset.
pub fn for_instance(preset: Option<&str>, kind: ServiceKind) -> &'static CatalogueEntry {
    preset
        .and_then(find)
        .or_else(|| match kind {
            ServiceKind::HealthCheck => find("custom"),
            other => find(other.as_str()),
        })
        .expect("catalogue has proxmox and custom entries")
}
