# Deploying Quorum

Quorum is one binary (`quorum-server`) with the web UI built in. Run it in a small Debian or Alpine LXC on Proxmox, or as a Docker container. Clients reach it over Tailscale/WireGuard only; it still requires sign-in for everything.

**Step-by-step guide for a Debian 13 LXC (build and run in the container): [INSTALL-DEBIAN-LXC.md](INSTALL-DEBIAN-LXC.md).**

| File | Purpose |
| --- | --- |
| `Dockerfile` | Multi-stage build: web UI, then a static musl binary, then a small Alpine image |
| `quorum.service` | systemd unit (hardened) |
| `install-lxc.sh` | Installs the binary, user, directories, config, master key and service in an LXC |
| `quorum.example.toml` | Server config |
| `services.seed.example.toml` | Optional services imported on first start |

## 1. Proxmox role and API token

Quorum only reads and powers guests on/off. Give it a dedicated user and token with exactly these privileges, on a Proxmox node shell:

```sh
pveum role add QuorumRole -privs "VM.Audit Sys.Audit Datastore.Audit VM.PowerMgmt"
pveum user add quorum@pve --comment "Quorum control center"
pveum user token add quorum@pve quorum --privsep 1
# With privilege separation a token gets only what BOTH the user and the token are
# granted, so give the role to both:
pveum acl modify / --roles QuorumRole --users quorum@pve
pveum acl modify / --roles QuorumRole --tokens 'quorum@pve!quorum'
# Check: should list VM.Audit, Sys.Audit, Datastore.Audit and VM.PowerMgmt on "/"
pveum user token permissions quorum@pve quorum
```

The `token add` command prints the secret once. In Quorum, add a **Proxmox cluster** service with:

- Base URL: `https://<node address>:8006`
- Token ID: `quorum@pve!quorum`
- Token secret: the printed value (stored encrypted)

The connection test warns when the token is missing a privilege or has more than it needs.

### Self-signed certificate: pin its fingerprint

Quorum never disables TLS verification. For Proxmox's self-signed certificate, pin its SHA-256 fingerprint. On the node:

```sh
openssl x509 -in /etc/pve/local/pve-ssl.pem -noout -fingerprint -sha256
# If you installed a custom certificate, use /etc/pve/local/pveproxy-ssl.pem instead.
```

Paste the `AB:CD:…` value into the service's **Fingerprint** field. When the field is empty and the certificate isn't publicly trusted, the connection test shows the fingerprint the server presented, so you can compare it with the one above before saving.

Linking guests to services: set the container on the service, or add the tag `svc-<service id>` to the guest in Proxmox (e.g. `svc-vaultwarden`).

## 2a. LXC install

Build on any machine with Rust and Node (or use the Docker build stage):

```sh
(cd web && npm ci && npm run build)
(cd backend && cargo build --release)
scp backend/target/release/quorum-server deploy/* root@quorum-lxc:/root/quorum/
ssh root@quorum-lxc /root/quorum/install-lxc.sh /root/quorum/quorum-server
```

The script creates the `quorum` user, `/var/lib/quorum`, `/etc/quorum/quorum.toml`, a random master key in `/etc/quorum/quorum.env` (mode 600) and starts the service.

**Back up `/etc/quorum/quorum.env`.** Without the master key, stored service secrets cannot be decrypted (re-enter them to recover).

## 2b. Docker

```sh
docker build -f deploy/Dockerfile -t quorum .
echo "QUORUM_SECRET_KEY=$(openssl rand -base64 32)" > quorum.env   # keep safe
docker run -d --name quorum --env-file quorum.env -p 8080:8080 -v quorum-data:/var/lib/quorum quorum
```

## 2c. Behind Nginx Proxy Manager (HTTPS on your own domain)

Phones need HTTPS to install the web app and for the iPhone app. If your VPN clients use
your home DNS and Nginx Proxy Manager (NPM) holds a publicly trusted certificate for your
domain (for example Let's Encrypt through the Cloudflare DNS challenge), put Quorum behind it:

1. **Proxy host** in NPM: domain `quorum.yourdomain`, scheme `http`, forward to the Quorum
   LXC's IP, port `8080`. SSL tab: your certificate, **Force SSL**, HTTP/2.
2. **Advanced** tab (live updates are one long-lived connection; Quorum also sends
   `X-Accel-Buffering: no`, this is belt and braces):

   ```nginx
   proxy_buffering off;
   proxy_read_timeout 1h;
   ```
3. **`quorum.toml`**:

   ```toml
   [server]
   public_url = "https://quorum.yourdomain"   # in pairing QR codes; enables Secure cookies
   trusted_proxies = ["<NPM's IP>"]           # rate limits per device, not per proxy
   ```

   Restart Quorum (`systemctl restart quorum`).
4. Optionally let only NPM reach port 8080 (firewall on the LXC or in Proxmox), so nobody
   bypasses the proxy.

A **Cloudflare Origin CA** certificate won't work: only Cloudflare's own proxy trusts it, so
phones reject it. Use a Let's Encrypt (or other public) certificate in NPM.

## 3. First start

With no users yet, the server logs a one-time **setup token**. Open the web UI, enter it, and create the owner account. Then add services in Settings.

Try everything without real services: `quorum-server --demo` (fake data, in-memory database, sign in as `demo`/`demo`).

## Environment

| Variable | Meaning |
| --- | --- |
| `QUORUM_SECRET_KEY` | Required. 32 bytes, base64. Encrypts service secrets at rest |
| `QUORUM_CONFIG` | Config path (default `/etc/quorum/quorum.toml`) |
| `QUORUM_BIND`, `QUORUM_DATABASE`, `QUORUM_PUBLIC_URL` | Override the config file |
| `QUORUM_TRUSTED_PROXIES` | Comma-separated proxy IPs/ranges, overrides `trusted_proxies` |
| `RUST_LOG` | Log filter, e.g. `info` or `quorum_server=debug` |
