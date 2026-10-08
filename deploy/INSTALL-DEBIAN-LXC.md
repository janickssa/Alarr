# Install Quorum in a Debian 13 LXC: step by step

Build and run Quorum inside one Debian 13 (trixie) container on Proxmox. Run every command
as **root** in the container's console (Proxmox → the container → Console), starting in the
folder you cloned the repository into (`cd Alarr`).

Replace the values in `<…>` with your own.

## 0. Container size

Compiling needs more resources than running. While you build, give the container:

- **RAM:** 4 GB (Quorum itself uses about 50–100 MB afterwards)
- **CPU:** 2 cores or more (the build takes 5–15 minutes)
- **Disk:** 10 GB

In Proxmox, check **Options → Features → Nesting** is ticked. The service uses systemd
sandboxing, which needs it in an unprivileged container.

## 1. Install the build tools

```sh
apt update
apt install -y build-essential pkg-config curl ca-certificates git
```

Debian 13's own Rust (1.85) and Node.js (20) are too old for Quorum, so get current ones:

```sh
# Rust (stable), installed for root
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"

# Node.js 22 from NodeSource
curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
apt install -y nodejs
```

Check:

```sh
rustc --version   # 1.88 or newer
node --version    # v22.17 or newer
```

## 2. Build

The web UI is built first, because it is embedded in the server binary.

```sh
cd web
npm ci
npm run build
cd ../backend
cargo build --release --locked
cd ..
```

The result is one file: `backend/target/release/quorum-server`.

## 3. Install the service

```sh
sh deploy/install-lxc.sh backend/target/release/quorum-server
```

This creates the `quorum` system user, `/var/lib/quorum` (database),
`/etc/quorum/quorum.toml` (config), a random master key in `/etc/quorum/quorum.env`, and
starts the `quorum` systemd service.

**Back up `/etc/quorum/quorum.env` now** (password manager or another machine). Without
it, the API keys and passwords Quorum stores can't be decrypted.

## 4. Configure

```sh
nano /etc/quorum/quorum.toml
```

Set the `[server]` block (leave the rest as it is for now):

```toml
[server]
bind = "0.0.0.0:8080"
public_url = "https://<quorum.yourdomain>"
database = "/var/lib/quorum/quorum.db"
trusted_proxies = ["<IP of Nginx Proxy Manager>"]
```

Restart and check it is running:

```sh
systemctl restart quorum
systemctl status quorum --no-pager
```

## 5. Nginx Proxy Manager

Add a proxy host:

- **Domain:** `<quorum.yourdomain>`
- **Scheme / Forward host / Port:** `http` / `<IP of this container>` / `8080`
- **SSL tab:** your Let's Encrypt certificate, **Force SSL** and **HTTP/2** on
- **Advanced tab:**

  ```nginx
  proxy_buffering off;
  proxy_read_timeout 1h;
  ```

Make sure your home DNS points `<quorum.yourdomain>` at Nginx Proxy Manager.

## 6. Create your account

Get the one-time setup token:

```sh
journalctl -u quorum | grep 'setup token'
```

Open `https://<quorum.yourdomain>`, enter the token, and choose your owner username and
password. Then add your services under **Settings → Services**. For the Proxmox service,
create the restricted API token first: see section 1 of [README.md](README.md).

## 7. Optional: allow only Nginx Proxy Manager to reach port 8080

```sh
apt install -y nftables
cat > /etc/nftables.conf <<'EOF'
#!/usr/sbin/nft -f
flush ruleset
table inet filter {
  chain input {
    type filter hook input priority 0; policy accept;
    tcp dport 8080 ip saddr != <IP of Nginx Proxy Manager> drop
  }
}
EOF
systemctl enable --now nftables
```

## Updating later

```sh
git pull
cd web && npm ci && npm run build && cd ..
cd backend && cargo build --release --locked && cd ..
install -m 0755 backend/target/release/quorum-server /usr/local/bin/quorum-server
systemctl restart quorum
```

Your config, master key and database are not touched by updates.

## If something goes wrong

| Problem | Fix |
| --- | --- |
| `cargo build` stops with "killed" or `signal: 9` | Not enough RAM: give the container 4 GB, or run `cargo build --release --locked -j 1` |
| `npm ci` complains about the Node.js version | `node --version` must be 22.17 or newer; redo the NodeSource step |
| `systemctl status quorum` shows `226/NAMESPACE` | Turn on **Nesting** (step 0) and restart the container |
| Service starts and stops again | `journalctl -u quorum -n 50 --no-pager` shows why (often a typo in `quorum.toml`) |
| Page doesn't load through the domain | Test directly first: `curl http://<IP of this container>:8080/api/v1/info` |
| Screens don't update live | Check the Advanced lines in the Nginx Proxy Manager host (step 5) |
| No setup token in the log | An account already exists; sign in, or delete `/var/lib/quorum/quorum.db` to start over |
