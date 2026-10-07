//! Service instances: secrets, API views and linking services to Proxmox guests.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::crypto::SecretBox;
use crate::db::ServiceRow;
use crate::modules::proxmox::{Guest, GuestStatus, GuestType, ProxmoxData};
use crate::modules::{
    Category, Health, InstanceConfig, IntegrationLevel, ModuleData, ServiceKind, ServiceSnapshot,
    catalogue,
};
use crate::state::AppState;

fn secret_context(service_id: &str, name: &str) -> String {
    format!("service/{service_id}/{name}")
}

pub fn seal(secrets: &SecretBox, service_id: &str, name: &str, value: &str) -> String {
    secrets.seal(&secret_context(service_id, name), value)
}

/// Decrypt a stored service into the config its module is built from.
pub fn instance_config(row: &ServiceRow, secrets: &SecretBox) -> anyhow::Result<InstanceConfig> {
    let kind = ServiceKind::parse(&row.kind)
        .ok_or_else(|| anyhow::anyhow!("unknown service kind {}", row.kind))?;
    let mut plain = BTreeMap::new();
    for (name, sealed) in &row.secrets {
        plain.insert(
            name.clone(),
            secrets.open(&secret_context(&row.id, name), sealed)?,
        );
    }
    Ok(InstanceConfig {
        id: row.id.clone(),
        kind,
        name: row.name.clone(),
        base_url: row.base_url.clone(),
        settings: row.settings.clone(),
        secrets: plain,
    })
}

/// Whether a secret is stored. The value itself is never returned by the API.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SecretStatus {
    pub set: bool,
    /// `"••••"` when set.
    pub display: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LinkedGuest {
    pub vmid: u32,
    pub guest_type: GuestType,
    pub name: String,
    pub node: String,
    pub status: GuestStatus,
}

/// A service as returned by the API: config (without secret values) plus live health.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ServiceView {
    pub id: String,
    pub kind: ServiceKind,
    pub preset: Option<String>,
    pub name: String,
    pub category: Category,
    /// Phosphor icon name.
    pub icon: String,
    /// OKLCH hue of the service accent.
    pub accent_hue: u16,
    pub accent_chroma: f32,
    pub level: IntegrationLevel,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub sort_order: i64,
    /// Module settings (see `ProxmoxSettings`, `HealthCheckSettings`).
    #[schema(value_type = Object)]
    pub settings: Value,
    pub secrets: BTreeMap<String, SecretStatus>,
    /// Container/VM explicitly linked in the service config.
    pub linked_vmid: Option<u32>,
    /// The linked guest (by config or `svc-<id>` tag), if Proxmox knows it.
    pub linked_guest: Option<LinkedGuest>,
    pub health: Health,
    pub health_reason: Option<String>,
    pub response_ms: Option<u64>,
    pub updated_at: Option<DateTime<Utc>>,
    pub version: Option<String>,
}

pub fn parse_category(value: &str) -> Option<Category> {
    serde_json::from_value(Value::String(value.to_owned())).ok()
}

/// Links between services and guests: explicit `linked_vmid` plus `svc-<service id>` tags.
pub struct Links {
    pub service_to_guest: HashMap<String, u32>,
    pub guest_to_services: HashMap<u32, Vec<String>>,
}

pub fn links(services: &[ServiceRow], guests: &[Guest]) -> Links {
    let mut service_to_guest = HashMap::new();
    for s in services {
        if let Some(vmid) = s.linked_vmid {
            service_to_guest.insert(s.id.clone(), vmid);
        }
    }
    for g in guests {
        for tag in &g.tags {
            if let Some(id) = tag.strip_prefix("svc-")
                && services.iter().any(|s| s.id == id)
            {
                service_to_guest.entry(id.to_owned()).or_insert(g.vmid);
            }
        }
    }
    let mut guest_to_services: HashMap<u32, Vec<String>> = HashMap::new();
    for (id, vmid) in &service_to_guest {
        guest_to_services.entry(*vmid).or_default().push(id.clone());
    }
    for v in guest_to_services.values_mut() {
        v.sort();
    }
    Links {
        service_to_guest,
        guest_to_services,
    }
}

/// The Proxmox instance used by the `/proxmox` endpoints: the first enabled one.
pub fn proxmox_instance(rows: &[ServiceRow]) -> Option<&ServiceRow> {
    rows.iter()
        .find(|r| r.enabled && r.kind == ServiceKind::Proxmox.as_str())
}

pub fn proxmox_data(snapshot: &ServiceSnapshot) -> Option<&ProxmoxData> {
    match &snapshot.data {
        ModuleData::Proxmox(d) => Some(d),
        _ => None,
    }
}

/// Every guest Proxmox currently reports, with `linked_services` filled in.
pub fn guests_with_links(state: &AppState, rows: &[ServiceRow]) -> Option<(String, Vec<Guest>)> {
    let pve = proxmox_instance(rows)?;
    let snapshot = state.cache.get(&pve.id)?;
    let data = proxmox_data(&snapshot)?;
    let l = links(rows, &data.guests);
    let guests = data
        .guests
        .iter()
        .cloned()
        .map(|mut g| {
            g.linked_services = l
                .guest_to_services
                .get(&g.vmid)
                .cloned()
                .unwrap_or_default();
            g
        })
        .collect();
    Some((pve.id.clone(), guests))
}

pub fn view(
    row: &ServiceRow,
    snapshot: Option<&ServiceSnapshot>,
    guests: &[Guest],
    links: &Links,
) -> ServiceView {
    let kind = ServiceKind::parse(&row.kind).unwrap_or(ServiceKind::HealthCheck);
    let entry = catalogue::for_instance(row.preset.as_deref(), kind);
    let mut secrets = BTreeMap::new();
    for name in kind.secret_names() {
        let set = row.secrets.contains_key(*name);
        secrets.insert(
            (*name).to_owned(),
            SecretStatus {
                set,
                display: set.then(|| "••••".to_owned()),
            },
        );
    }
    let linked_guest = links
        .service_to_guest
        .get(&row.id)
        .and_then(|vmid| guests.iter().find(|g| g.vmid == *vmid))
        .map(|g| LinkedGuest {
            vmid: g.vmid,
            guest_type: g.guest_type,
            name: g.name.clone(),
            node: g.node.clone(),
            status: g.status,
        });
    let (health, health_reason, response_ms, updated_at, version) = match (row.enabled, snapshot) {
        (false, _) => (Health::Disabled, None, None, None, None),
        (true, None) => (Health::Unknown, None, None, None, None),
        (true, Some(s)) => (
            s.health,
            s.health_reason.clone(),
            s.response_ms,
            Some(s.updated_at),
            s.version.clone(),
        ),
    };
    ServiceView {
        id: row.id.clone(),
        kind,
        preset: row.preset.clone(),
        name: row.name.clone(),
        category: row
            .category
            .as_deref()
            .and_then(parse_category)
            .unwrap_or(entry.category),
        icon: entry.icon.to_owned(),
        accent_hue: entry.accent_hue,
        accent_chroma: entry.accent_chroma,
        level: kind.level(),
        base_url: row.base_url.clone(),
        enabled: row.enabled,
        sort_order: row.sort_order,
        settings: row.settings.clone(),
        secrets,
        linked_vmid: row.linked_vmid,
        linked_guest,
        health,
        health_reason,
        response_ms,
        updated_at,
        version,
    }
}

/// All services the user may see, as views.
pub async fn visible_views(
    state: &AppState,
    user: &crate::auth::CurrentUser,
) -> anyhow::Result<Vec<ServiceView>> {
    let rows = state.db.list_services().await?;
    let guests = guests_with_links(state, &rows)
        .map(|(_, g)| g)
        .unwrap_or_default();
    let l = links(&rows, &guests);
    Ok(rows
        .iter()
        .filter(|r| user.can_see(&r.id))
        .map(|r| view(r, state.cache.get(&r.id).as_ref(), &guests, &l))
        .collect())
}
