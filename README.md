# Quorum – Homelab Control Center

One place to see the status of every self-hosted service and take the few actions that matter: a Proxmox cluster overview with VM/container power actions, the media stack (Jellyfin, Seerr, Sonarr, Radarr, Prowlarr, Bazarr, qBittorrent, Navidrome), network/security services and simple health checks for everything else.

"Quorum" is the name recommended by the designer. It can be changed later; keep the name in one constant/config value per codebase so a rename is cheap.

---

## Instructions for the coding LLM (read this first)

You are building this project from this README plus the design files in `design/`. Work like this:

1. **Work phase by phase, in order.** Do not start a phase until every checkbox in the previous phase's *Definition of done* is ticked. Within a phase, work top to bottom.
2. **Tick checkboxes in this README** (`- [ ]` → `- [x]`) in the same commit that completes the task, so progress is visible in the repo.
3. **Small commits** with clear messages (`backend: add proxmox guest list endpoint`). Open one PR per phase if working with PRs.
4. **Verify every external API against the official docs for the installed version** before implementing it. The endpoint hints in this README are starting points, not guarantees.
5. **Never hard-code secrets, IPs or hostnames.** Example values (like `10.0.20.123`) are placeholders.
6. **The design files are the visual source of truth.** Where the design and this README disagree on looks, follow the design. Where they disagree on behaviour, follow this README.
7. **When something is ambiguous, pick the simplest option that keeps the door open, write it under "Decisions log" at the bottom of this file, and continue.** Only stop and ask the owner when a decision is hard to reverse (data model, auth, security).
8. All units are metric: GB/TB, MB/s and Mbit/s, °C. 24-hour time, dates as `7 Oct 2026` or `07-10-2026`. A space before `%` (`12 %`).

---

## Repository layout (target)

```
/
├─ README.md                 ← this file
├─ design/                   ← the design export, unzipped as-is (do not edit)
│  ├─ 01 Identity & Design System.dc.html
│  ├─ support.js
│  └─ _ds/nocturne-…/styles.css, readme.md, _ds_manifest.json, …
├─ docs/
│  └─ design-brief.md        ← optional: the full design brief (exported)
├─ backend/                  ← Rust server (cargo crate: quorum-server)
├─ web/                      ← Web UI + PWA (one codebase)
├─ ios/                      ← Native SwiftUI app (phase 6)
├─ agent/                    ← Tiny Rust agent for WireGuard peers (phase 4)
├─ openapi/openapi.json      ← Generated from the backend, committed
└─ deploy/                   ← Dockerfile, LXC/systemd setup, example config
```

### About the design files

- `design/01 Identity & Design System.dc.html` is **batch 1 of 5**: name and logo, colours, health colours, service accents, module icons, type scale, units/formatting, spacing, status badges, stat tiles, resource bars, charts, service cards, media components, power menu, confirmations, toasts, filters, loading/empty/error states, app icons and splash. Open it in a browser (it loads fonts and icons from the internet).
- `design/_ds/nocturne-…/styles.css` holds the design tokens (colours, ramps, spacing, radii, shadows). It is the token source of truth.
- The design system's own `readme.md` mentions `foundations/`, `components/`, `templates/` and `theme.json`. **Those files are not in the export; ignore those references.**
- Batches 2–5 (desktop web pages, native iOS screens + widgets, PWA screens, opt-in variants and interaction notes) **are not delivered yet.** Until they arrive, build page layouts from the page specs in this README using the batch 1 components. When new batches are added to `design/`, they override layout guidance here.

### Key design facts (extracted from batch 1)

| Topic | Value |
| --- | --- |
| Theme | Nocturne, dark. Ground `#161826`, surface `#232532`, raised `#292b31`, border `#3f424d`, text `#e9e9ed`, accent `#9184d9` |
| Fonts | Web: Inter (400/500, headings max 500) + JetBrains Mono for IDs, IPs, hostnames, logs. iOS: SF Pro + SF Mono, Dynamic Type |
| Numbers | Tabular figures (`font-variant-numeric: tabular-nums`) for every live-updating number |
| Icons | Phosphor everywhere. Every service icon tile is also a logo slot |
| Buttons | Primary = 1px accent outline on transparent, never a solid fill |
| Health colours | Healthy `oklch(0.78 0.13 155)`, Warning `oklch(0.83 0.13 80)`, Down `oklch(0.72 0.16 25)`, In progress `oklch(0.77 0.11 240)`, Disabled/Unknown `#9397ab`. Tinted fills at L 0.30–0.32. Always paired with icon + label |
| Service accents | Low chroma (≤ 0.075), never confused with health. Tile `oklch(0.33 0.045 H)`, ink `oklch(0.84 0.07 H)` |
| Type scale | Large title 32/500 · Title 22/500 · Stat 26/500 · Headline 16/500 · Body 15/400 · Caption 12/400 · Overline 11/500 +0.08em · Mono 13/400 |
| Radii | sm 4 (chips) · md 8 (cards) · lg 14 (sheets) |
| Density | Mobile: 56 pt rows, 16 px margins, 12 px gaps, 44 pt hit targets. Desktop: 36 px rows, 24 px gutters, 16 px gaps, 3–4 columns at ≥ 1280 px |
| Resource bars | Neutral below 80 %, amber + warning icon 80–89 %, red + error icon ≥ 90 % |
| Toasts | Bottom-centre on mobile (above tab bar), bottom-right on desktop, 5 s timeout; in-progress toasts stay and morph into success/error |
| Loading | Skeletons with a 1.2 s shimmer, never spinners for page loads |
| PWA colours | `theme_color` and `background_color` `#161826` |

Service accent hues and icons (from the module catalogue):

| Service | Container | Level | Phosphor icon | Accent hue |
| --- | --- | --- | --- | --- |
| Proxmox cluster | host | Full | `hard-drives` | 45 |
| Jellyfin / Jellyfin (public) | CT 108 / CT 105 | Full | `film-strip` | 305 |
| Seerr | CT 126 | Full | `ticket` | 268 |
| Sonarr | CT 123 | Full | `television-simple` | 222 |
| Radarr | CT 124 | Full | `film-slate` | 75 |
| Prowlarr | CT 125 | Full | `binoculars` | 335 |
| Bazarr | CT 127 | Full | `subtitles` | 115 |
| qBittorrent | CT 120 | Full | `arrows-down-up` | 190 |
| Navidrome | CT 119 | Full | `music-notes` | 10 |
| Podgrab | CT 118 | Health | `microphone` | 45 |
| Technitium DNS | CT 112 | Full | `funnel` | 190 |
| Nginx Proxy Manager | CT 115 | Read-only | `arrows-split` | 75 |
| Authentik | CT 121 | Read-only | `fingerprint` | 10 |
| WireGuard (wg-home) | CT 102 | Read-only (agent) | `plugs-connected` | 222 |
| Vaultwarden | CT 114 | Health only | `vault` | 268 |
| Nextcloud | CT 113 | Read-only | `cloud` | 222 |
| lxc-updater | CT 104 | Read-only | `arrows-clockwise` | 45 |
| apt-cacher-ng | CT 103 | Health | `package` | 75 |
| smb-share | CT 111 | Health | `folder-simple` | 115 |
| BookStack | CT 117 | Health | `books` | 115 |
| draw.io | CT 116 | Health | `flow-arrow` | 75 |
| Kiwix | CT 122 | Health | `book-open-text` | 190 |
| MkDocs docs | CT 128 | Health | `file-text` | 268 |
| Custom service | any | Health | `puzzle-piece` | slate (h 265, c 0.015) |

Container IDs are the owner's current setup and only used as seed/demo data. Real mapping is configured per service.

---

## Architecture

```
 iPhone app (SwiftUI) ─┐
 PWA (iPhone/Android) ─┼──HTTPS over Tailscale──▶  quorum-server (Rust, one binary)
 Desktop browser ──────┘                           ├─ REST API + SSE live updates
                                                   ├─ serves the built web UI
                                                   ├─ pollers + in-memory state cache
                                                   ├─ SQLite (settings, services, users, log)
                                                   └─ talks to Proxmox + every service
```

- **One backend** holds all API keys and talks to every service. Clients never talk to services directly.
- **Pollers + cache:** each enabled service instance has its own polling task. Clients read from the cache, so responses are instant and services are not hammered by many clients. Live changes are pushed over Server-Sent Events.
- **Every service is a module** implementing a shared trait. Adding a service = adding one module file + registering it. Multiple instances of the same kind are supported (two Jellyfins).
- **The OpenAPI spec is generated from the Rust code** and committed. The web client (TypeScript) and the iOS client (Swift) are generated from it.
- **Access is over Tailscale/WireGuard only.** The server still requires authentication for every request.

### Tech choices

| Area | Choice |
| --- | --- |
| Backend | Rust stable, `axum`, `tokio`, `reqwest` (rustls), `serde`, `sqlx` (SQLite, migrations), `utoipa` (+ `utoipa-axum`) for OpenAPI, `tracing`, `rust-embed` to embed the web build, `argon2` for passwords, `chacha20poly1305` for secrets at rest, `wiremock` for tests |
| Web | SvelteKit with `adapter-static` (SPA, fallback `index.html`), TypeScript, Vite, `openapi-typescript` + `openapi-fetch`, Phosphor icons (bundled, not CDN), uPlot for time-series charts, small hand-written SVG sparklines |
| Fonts | Inter and JetBrains Mono **self-hosted** in the web build (no Google Fonts / unpkg at runtime: the app must work without internet) |
| PWA | `vite-plugin-pwa` (manifest + service worker) |
| iOS | SwiftUI, iOS 17+, `swift-openapi-generator`, WidgetKit, Keychain, App Group for widget data |
| Notifications | Both ntfy (self-hosted) and Apple Push Notification service (APNs, sent directly from the backend with the `a2` crate and a token-based `.p8` key). The owner has a paid Apple Developer account |
| Deployment | Single static binary in a Debian/Alpine LXC on Proxmox, or Docker image. systemd unit provided |

### Security rules (non-negotiable)

- Proxmox access uses a dedicated API token with a custom role holding only `VM.Audit`, `Sys.Audit`, `Datastore.Audit` and `VM.PowerMgmt`. The app must never call config-changing Proxmox endpoints.
- Proxmox's self-signed certificate is handled by **pinning its SHA-256 fingerprint**, never by disabling TLS verification globally.
- Service secrets (API keys, passwords) are stored in SQLite **encrypted** with a master key from the environment variable `QUORUM_SECRET_KEY`. Secrets are never logged, never returned by the API (return `"••••"` + "set/not set").
- Vaultwarden is **health check only** (`/alive`). Never ask for or store its admin token.
- Every request requires a session (web) or a device token (PWA/iOS). Roles: `owner`, `member`, `guest`. Guests see only modules the owner has allowed and cannot run power actions unless explicitly allowed.
- Destructive actions (stop, hard stop, reboot, delete torrent) require a confirmation flag in the request body (`"confirm": true`) so a stray call can't trigger them.
- Every action is written to the activity log: who, what, target, when, result.
- Rate-limit login and pairing endpoints.

---

## Backend design

### Module trait (sketch)

```rust
#[async_trait]
pub trait ServiceModule: Send + Sync {
    fn kind(&self) -> ServiceKind;            // Sonarr, Jellyfin, HealthCheck, …
    fn level(&self) -> IntegrationLevel;      // Full | ReadOnly | HealthCheck
    async fn poll(&self, ctx: &PollCtx) -> Result<ServiceSnapshot>; // health + data
    fn actions(&self) -> &[ActionDescriptor]; // e.g. scan_library, approve_request
    async fn run_action(&self, ctx: &ActionCtx, action: &str, args: Value) -> Result<ActionResult>;
    async fn test_connection(&self, cfg: &InstanceConfig) -> Result<ConnectionTest>;
}
```

- `ServiceSnapshot` always contains a common header (`health`, `health_reason`, `response_ms`, `updated_at`, `version`) plus a typed, module-specific `data` payload.
- Each module has typed request/response structs and `utoipa` schemas so the OpenAPI spec is fully typed per module.
- Health is computed per module: `healthy`, `warning`, `down`, `in_progress`, `disabled`, `unknown`.

### Polling intervals (defaults, configurable)

| What | Interval | Fast mode (a client is viewing it) |
| --- | --- | --- |
| Proxmox cluster/nodes/guests | 10 s | 3 s |
| qBittorrent transfer + torrents | 10 s | 2 s |
| Jellyfin / Navidrome sessions | 15 s | 5 s |
| Sonarr/Radarr/Prowlarr/Bazarr/Seerr | 60 s | 15 s |
| Network services | 60 s | 15 s |
| Generic health checks | 30 s | 30 s |

Back off exponentially (max 5 min) when a service is down, but report it as down immediately.

### API surface (first version)

All under `/api/v1`. JSON. Live updates on `GET /api/v1/events` (SSE, events: `snapshot`, `health`, `action`, `alert`).

- `POST /auth/login`, `POST /auth/logout`, `GET /auth/me`
- `POST /pairing/start` (owner, returns code + QR payload), `POST /pairing/complete` (device), `GET/DELETE /devices`
- `GET /overview` – everything Home needs in one call (overall health, problems, node strip, highlight per service)
- `GET /health` – all services + guests with status, response time, uptime 24 h / 7 d, 24 h timeline
- `GET /services`, `POST /services`, `GET/PATCH/DELETE /services/{id}`, `POST /services/{id}/test`, `POST /services/{id}/enable|disable`
- `GET /services/{id}/snapshot`, `POST /services/{id}/actions/{action}`
- `GET /proxmox/cluster`, `GET /proxmox/nodes`, `GET /proxmox/nodes/{node}/rrd?range=hour|day|week`, `GET /proxmox/storage`
- `GET /proxmox/guests`, `GET /proxmox/guests/{vmid}`, `GET /proxmox/guests/{vmid}/rrd`, `POST /proxmox/guests/{vmid}/power` (`start|shutdown|stop|reboot`)
- `GET /proxmox/tasks/{upid}`
- `GET /media/overview` (now playing across instances, recently added, calendar, request tracker)
- `GET /activity`, `GET /alerts`, `GET/PUT /alerts/settings`
- `GET/PUT /users`, `GET/PUT /settings`
- `GET /api/v1/openapi.json`

### Demo mode

`quorum-server --demo` runs with **fake data for every module** (matching the sample data in the design: Glass Meridian, Saltwater Kings, CT 123 sonarr on pve-02, 38.2 MB/s, etc.). Actions succeed after a short delay. This lets the web, PWA and iOS work be done without real services and is used in screenshots and tests.

### Config

Server settings come from `/etc/quorum/quorum.toml` (path overridable with `QUORUM_CONFIG`). Services are added and edited in the UI and stored in SQLite. An optional `services.seed.toml` imports services on first start.

```toml
[server]
bind = "0.0.0.0:8080"
public_url = "https://quorum.your-tailnet.ts.net"
database = "/var/lib/quorum/quorum.db"
# QUORUM_SECRET_KEY must be set in the environment (32 bytes, base64)

[ntfy]
url = "https://ntfy.example.lan"
topic = "quorum"

[apns]
enabled = true
team_id = "ABCDE12345"
key_id = "XYZ987WVUT"
key_path = "/etc/quorum/AuthKey_XYZ987WVUT.p8"   # file mode 600, never committed
bundle_id = "com.example.quorum"
environment = "production"                        # or "sandbox" for Xcode debug builds
```

### External API hints (verify against current docs)

| Service | Auth | Useful endpoints |
| --- | --- | --- |
| Proxmox VE | Header `Authorization: PVEAPIToken=USER@REALM!TOKENID=SECRET` | `GET /api2/json/cluster/status`, `/cluster/resources`, `/nodes/{node}/status`, `/nodes/{node}/rrddata`, `/nodes/{node}/{qemu\|lxc}/{vmid}/status/current`, `POST …/status/{start\|shutdown\|stop\|reboot}`, `/nodes/{node}/tasks/{upid}/status`, `/nodes/{node}/storage` |
| Jellyfin | Header `Authorization: MediaBrowser Token="…"` | `/System/Info`, `/Sessions`, `/Library/VirtualFolders`, `/Items/Counts`, `/Items/Latest`, `POST /Library/Refresh`, `POST /Items/{id}/Refresh` |
| Seerr (Overseerr/Jellyseerr API) | `X-Api-Key` | `/api/v1/status`, `/api/v1/request?filter=pending`, `/api/v1/request/count`, `POST /api/v1/request/{id}/approve`, `POST /api/v1/request/{id}/decline` |
| Sonarr | `X-Api-Key` | `/api/v3/queue`, `/calendar`, `/wanted/missing`, `/health`, `/diskspace`, `/rootfolder`, `/system/status`, `POST /api/v3/command` (`RescanSeries`, `RefreshSeries`, `RssSync`, `MissingEpisodeSearch`) |
| Radarr | `X-Api-Key` | Same shape as Sonarr; commands `RescanMovie`, `RefreshMovie`, `RssSync`, `MissingMoviesSearch` |
| Prowlarr | `X-Api-Key` | `/api/v1/indexer`, `/indexerstatus`, `/indexerstats`, `/health`, `POST /api/v1/indexer/testall` |
| Bazarr | `X-API-KEY` | `/api/system/status`, `/api/badges`, `/api/episodes/wanted`, `/api/movies/wanted`, history endpoints |
| qBittorrent | Cookie from `POST /api/v2/auth/login` | `/api/v2/app/webapiVersion`, `/transfer/info`, `/torrents/info`, `/transfer/toggleSpeedLimitsMode`, `/torrents/delete`. **v5 renamed `pause`/`resume` to `stop`/`start`**: detect the API version and use the right one |
| Navidrome | Subsonic API: `u`, `t`=md5(password+salt), `s`, `v`, `c`, `f=json` | `/rest/ping`, `/rest/getNowPlaying`, `/rest/startScan`, `/rest/getScanStatus` |
| Technitium DNS | `token` query param | `/api/dashboard/stats/get?type=LastDay`, temporary blocking disable via settings API, re-enable blocking |
| Nginx Proxy Manager | JWT from `POST /api/tokens` | `/api/nginx/proxy-hosts`, `/api/nginx/certificates` |
| Authentik | Bearer token | `/api/v3/events/events/` (filter login / login_failed), `/api/v3/outposts/instances/`, admin system endpoint |
| Nextcloud | App password + header `OCS-APIRequest: true` (or serverinfo token) | `/ocs/v2.php/apps/serverinfo/api/v1/info?format=json` |
| Vaultwarden | none | `GET /alive` only |
| WireGuard | Quorum agent (see phase 4) | `GET /peers` on the agent |
| lxc-updater | unknown | Open question: find out what it outputs (see phase 4) |
| Generic health check | none | HTTP(S) GET with expected status code, or TCP connect to host:port |

---

## Pages (behaviour spec for all clients)

Top-level navigation on every surface: **Home · Proxmox · Media · Services · More/Settings**. Global search (services, guests, movies, series, torrents). Categories: Infrastructure, Media, Network and security, Apps. A category only appears when it has at least one enabled service. A disabled service never leaves an empty slot.

- **Home:** status banner ("All 18 services healthy" / "2 problems"), problems list with quick actions, Proxmox node strip, one highlight card per enabled service (reorderable/hideable in edit mode).
- **Health:** every service and guest with status, response time, uptime %, 24 h timeline, certificate and disk warnings.
- **Proxmox:** cluster overview (quorum, nodes with CPU/RAM/disk/temperature/uptime, storage pools, sparklines), guest list (filter/sort/search, linked service badges, power actions), guest detail (graphs 1 h/24 h, info, linked services, last backup, recent tasks), power flow with normal and destructive confirmations as designed.
- **Media:** now playing across Jellyfin instances and Navidrome, request tracker stepper (Requested → Approved → Grabbed → Downloading → Imported → Available, stuck step in amber with reason), combined calendar, recently added.
- **Service pages (shared skeleton):** header (icon, name, health, linked container with power menu, "Open web UI"), 3–4 stat tiles, actions row, then lists:
  - Jellyfin (per instance): streams with transcode reason and bitrate, libraries with Scan / Scan all, recently added, users.
  - Seerr: pending requests with Approve/Decline, history, new request search.
  - Sonarr/Radarr: queue, calendar, wanted/missing with Search, library grid, Rescan/Refresh/RSS sync, health banner.
  - Prowlarr: indexers with status, response time, failures 24 h, Test all.
  - Bazarr: missing subtitle counts and list, Search / Search all, history.
  - qBittorrent: live speeds + 10 min graph, free space, torrent list with filters, pause/resume (single/all), delete with/without files, alt speed toggle, auto-throttle toggle.
  - Navidrome: now playing, library counts, last scan, Scan library.
  - Technitium DNS: 24 h stats, allowed vs blocked chart, top blocked/clients, Pause blocking (5 min / 30 min / 1 h) with countdown banner and Resume.
  - Nginx Proxy Manager: proxy hosts, certificates with days left (< 14 d amber, < 3 d or expired red).
  - Authentik: logins and failed logins 24 h, recent events, active sessions, outpost health.
  - WireGuard: peers, last handshake, transfer, online (handshake < 3 min), "agent not installed" state.
  - Nextcloud: storage, active users, files, update badge, PHP/DB health.
  - lxc-updater: containers with pending updates; badge on Proxmox guest rows.
  - Health-check template (Vaultwarden, Podgrab, apt-cacher-ng, smb-share, BookStack, draw.io, Kiwix, MkDocs, custom): status, response ms, 24 h chart, uptime 24 h / 7 d, last incident, linked container, Open.
- **Launcher view:** optional grid of all services as large tiles (homepage-style).
- **Activity log**, **Alerts** (list + per-type settings: on/off, channel, quiet hours), **Settings** (Manage services, add/configure flow with connection test, custom health check, users and devices, appearance, backend info), **Login** and **Pairing**.
- **States everywhere:** loading (skeleton), empty, error with retry, service unreachable, backend unreachable / not on VPN, partial data, guest without permission.

---

## Phases

### Phase 0 – Repository and tooling

- [ ] Create the folder layout above; unzip the design export into `design/` unchanged.
- [ ] `backend/`: cargo project `quorum-server`, `rustfmt.toml`, clippy set to deny warnings in CI.
- [ ] `web/`: SvelteKit + TypeScript + `adapter-static`, ESLint + Prettier.
- [ ] GitHub Actions: backend (`fmt --check`, `clippy`, `test`), web (`lint`, `check`, `build`), and a job that regenerates `openapi/openapi.json` and fails if it differs from the committed file.
- [ ] `.gitignore`, `.editorconfig`, `LICENSE` (owner to choose; default MIT), `CONTRIBUTING.md` pointing to this README.
- [ ] Add the "Decisions log" section entries for anything decided here.

**Definition of done:** CI is green on an empty-but-building backend and web app.

### Phase 1 – Backend core

- [ ] Config loading (TOML + env), structured logging with `tracing` (secrets redacted).
- [ ] SQLite with `sqlx` migrations: users, sessions, devices, service instances (encrypted secrets), activity log, alert settings, uptime samples (keep 7 d at 1 min resolution, then downsample).
- [ ] Auth: owner bootstrap on first start (one-time setup token printed to the log), argon2 passwords, cookie sessions for web, device tokens for PWA/iOS, roles and per-module visibility for guests.
- [ ] Pairing flow: owner starts pairing → code + QR payload → device completes → device token. List and revoke devices.
- [ ] `ServiceModule` trait, module registry, multi-instance support, per-instance poller tasks with fast mode and backoff, in-memory snapshot cache.
- [ ] SSE endpoint broadcasting snapshot/health/action/alert events, filtered by the user's permissions.
- [ ] Generic **health-check module** (HTTP status / TCP connect, response time, uptime history).
- [ ] **Proxmox module**: cluster status, nodes, storage, guests, RRD data, guest detail, tasks, power actions with confirm flag, fingerprint pinning, linked-service mapping (by config and by Proxmox tag `svc-<service-id>`).
- [ ] Endpoints: auth, pairing, devices, services CRUD + test + enable/disable, overview, health, proxmox, activity, settings.
- [ ] OpenAPI generation with `utoipa`, served at `/api/v1/openapi.json` and written to `openapi/openapi.json` by a `cargo run -- openapi` subcommand.
- [ ] `--demo` mode with fake Proxmox data and fake health checks.
- [ ] Serve the web build embedded with `rust-embed` (SPA fallback to `index.html`).
- [ ] Tests: unit tests per module with `wiremock` fixtures, API tests for auth/permissions and the confirm flag on destructive actions.
- [ ] `deploy/`: Dockerfile (multi-stage, static binary), systemd unit, LXC install script, `quorum.example.toml`, and a short doc showing how to create the Proxmox role and token:
      `pveum role add QuorumRole -privs "VM.Audit Sys.Audit Datastore.Audit VM.PowerMgmt"`.

**Definition of done:** in demo mode and against a real Proxmox, the API returns cluster, node, guest and health data; power actions work and are logged; SSE pushes changes; OpenAPI file is committed.

### Phase 2 – Web UI (desktop first)

- [ ] Port the design tokens from `design/_ds/nocturne-…/styles.css` into `web/src/lib/styles/tokens.css`. Add the health colours, service accents (tile/ink per hue), JetBrains Mono and the type scale from the table above as tokens. Self-host Inter, JetBrains Mono and Phosphor icons.
- [ ] Generated API client from `openapi/openapi.json`; SSE store that merges live updates into page state; offline/backend-unreachable detection.
- [ ] Component library matching batch 1: status dot + badge, stat tile, resource bar (80/90 % thresholds), sparkline, time-series chart (1 h / 24 h / 7 d), uptime strip, service cards (compact, standard, large, health-check), power menu, confirm dialog (normal + destructive), toasts (in-progress → success/error), banners, segmented filters with counts, search field (`/` shortcut), skeletons, empty and error states, poster card, now-playing row, queue/torrent row, request tracker stepper.
- [ ] Simple component gallery page at `/dev/components` (only in dev builds) to compare against `design/01 Identity & Design System.dc.html`.
- [ ] App shell: left sidebar (5 destinations + enabled services by category), header with global search, right-side detail panel, keyboard shortcuts (`/`, `g h`, `g p`, `Esc`).
- [ ] Pages: Login, first-run onboarding (setup token → owner account → add first services), Home, Health, Proxmox (cluster, guests table with multi-select bulk start/shutdown, guest detail panel), Services list, health-check service page, Settings (Manage services with toggles, add/configure flow with connection test, custom health check, users and devices with pairing QR, appearance), Activity log.
- [ ] Opt-in behaviour: Home, sidebar and categories reflow with any combination of enabled services; verify with "everything on" and "only Proxmox + Jellyfin + qBittorrent".
- [ ] Accessibility: keyboard navigation, `:focus-visible` accent ring, WCAG AA contrast, status never colour-only.

**Definition of done:** the owner can log in, add Proxmox and health checks from the UI, see Home/Health/Proxmox live, and start/stop/reboot guests with the designed confirmations, all served from the single Rust binary.

### Phase 3 – Media stack

Backend module + web page for each, in this order. Each needs demo data and wiremock tests.

- [ ] qBittorrent (API version detection, alt speed toggle, delete with/without files).
- [ ] Sonarr and Radarr (shared code where the APIs match).
- [ ] Jellyfin (multi-instance: `jellyfin` and `jellyfin-pub`), library scan per library and all.
- [ ] Seerr (approve/decline).
- [ ] Prowlarr (test all indexers).
- [ ] Bazarr (search missing).
- [ ] Navidrome (scan, now playing).
- [ ] Media overview endpoint + Media page: now playing, request tracker (match Seerr request → Sonarr/Radarr item → qBittorrent torrent by hash → Jellyfin item), combined calendar, recently added.
- [ ] Home highlight cards for every media module.
- [ ] Global search across guests, services, movies, series and torrents.

**Definition of done:** every media page works against real services and in demo mode; library scans, approvals, searches and torrent actions work and are logged.

### Phase 4 – Network, infrastructure, alerts and automation

- [ ] Technitium DNS (stats, chart, top lists, pause blocking with countdown, resume).
- [ ] Nginx Proxy Manager (hosts, certificate expiry).
- [ ] Authentik (logins, failed logins, events, outposts).
- [ ] Nextcloud (serverinfo).
- [ ] lxc-updater: **first ask the owner what it is and what it outputs**; then integrate (pending updates per container + badges on guest rows), or skip with a note.
- [ ] `agent/`: tiny Rust binary for the WireGuard container that runs `wg show all dump` and exposes peers on `GET /peers` with a shared token. Install script + systemd unit. WireGuard module consumes it; "agent not installed" state.
- [ ] Alerts engine: service down, disk ≥ 90 %, download failed, certificate < 14 d, new request, node offline. Per-type settings, quiet hours, deduplication.
- [ ] Notification channel abstraction (`NotificationChannel` trait) so alerts can go to several channels; per alert type the owner picks ntfy, iOS push or both, and each user/device can mute types.
- [ ] ntfy delivery.
- [ ] Automation: auto-throttle qBittorrent (alt speed on) while Jellyfin has active streams (option: only remote streams), with hysteresis so it doesn't flap.
- [ ] Backup status: last vzdump job per guest on guest detail and Health.
- [ ] Launcher view.

**Definition of done:** all modules from the catalogue exist and can be toggled; alerts arrive in ntfy; auto-throttle works.

### Phase 5 – PWA for iPhone and Android

Same web codebase; this phase makes it a first-class mobile app.

- [ ] Mobile layouts for every page (breakpoints below 768 px): bottom navigation with the 5 destinations, pushed detail pages with back button and edge swipe, comfortable density (56 pt rows, 44 pt targets, 16 px margins).
- [ ] Mobile interactions: swipe actions on guest rows, torrents and Seerr requests; bottom sheets for power menus and confirmations; pull to refresh; toasts above the bottom nav.
- [ ] Safe areas and notch (`viewport-fit=cover`, `env(safe-area-inset-*)`), standalone display mode, no browser chrome.
- [ ] Web app manifest, icons from the design (`icon-192/512`, `maskable-192/512`, `apple-touch-icon` 180, favicon 16/32 + SVG), splash screens, `theme_color`/`background_color` `#161826`.
- [ ] Service worker: cache the app shell for instant start; never cache API responses with secrets; clear "backend unreachable / not on VPN" screen.
- [ ] Device pairing from the PWA by scanning the QR from desktop or typing the code; token stored securely (HttpOnly cookie where possible).
- [ ] Test on iPhone Safari (installed to home screen) and Android Chrome (installed). Tablet layout 768–1024 px.
- [ ] Light mode: **not designed yet.** Structure tokens so a light theme can be added; ask the owner whether to derive one now or wait for the designer.

**Definition of done:** the PWA installs on iPhone and Android, feels like an app, and every page is usable one-handed.

### Phase 6 – Native iPhone app

- [ ] Xcode project in `ios/` (SwiftUI, iOS 17+), targets: app, widget extension, shared package for API + models.
- [ ] API client generated from `openapi/openapi.json` with `swift-openapi-generator`; SSE client using `URLSession` byte streams; device token in Keychain.
- [ ] Pairing by scanning the desktop QR code with the camera, or entering the code.
- [ ] Tab bar with the 5 destinations, large titles, SF Symbols where Phosphor has no equivalent asset (or bundle Phosphor as SVG assets to match the web), SF Pro / SF Mono, Dynamic Type.
- [ ] Screens: Home, Proxmox overview, guest list (swipe actions, context menus), guest detail, Media, every service page, Health, Manage services, add service flow, Activity, Settings.
- [ ] Power confirmations as sheets (normal + destructive), toasts/banners, haptics on action results.
- [ ] Widgets via App Group cache: small (problems count or one service number), medium (download queue or active streams), large (nodes + problems), lock screen (download speed gauge, problems count).
- [ ] Optional: Live Activity for an active download or library scan.
- [ ] App icon (1024, single size) from the design.
- [ ] APNs in the backend: `a2` crate, token-based auth with the `.p8` key from `[apns]` config, device push tokens registered per paired device (`POST /devices/{id}/push-token`), invalid tokens (HTTP 410) removed automatically, sandbox vs production per device, delivery result in the activity log.
- [ ] APNs in the app: request permission after pairing, register the token with the backend, notification categories with actions where safe (e.g. "Approve" / "Decline" on a new Seerr request, "Open" on a down service; never power actions from a notification), deep links from a notification to the right page, Time Sensitive interruption level for "service down" and "node offline".
- [ ] Notification settings screen in the app: per alert type ntfy / push / both, quiet hours, mirrored from the web Settings.
- [ ] Distribution notes in `ios/README.md`: the owner has a paid Apple Developer account, so install via TestFlight or ad hoc builds (no 7-day expiry). Document creating the App ID with the Push Notifications capability, the APNs `.p8` key, and the App Group for widgets.

**Definition of done:** the app pairs with the backend, shows all pages live, runs power actions and scans, the widgets update, and alerts arrive as iOS push notifications (and in ntfy when chosen).

---

## Open questions for the owner

- Confirm the name **Quorum** (alternatives in the design: Hearth, Lantern).
- Light mode: derive now or wait for the designer? (Brief asks for both; batch 1 is dark only.)
- What does `lxc-updater` (CT 104) output?
- Are there more containers/VMs beyond CT 128 to add to the catalogue?

## Decisions log

| Date | Decision | Why |
| --- | --- | --- |
| 2026-10-07 | Rust backend, SvelteKit web/PWA, SwiftUI iOS | Owner's choice; one binary serves API + web |
| 2026-10-07 | Access over Tailscale/WireGuard only, auth still required | Defence in depth |
| 2026-10-07 | Fonts and icons self-hosted | Must work without internet |
| 2026-10-07 | Notifications via both ntfy and APNs | Owner has a paid Apple Developer account; ntfy also covers Android |
