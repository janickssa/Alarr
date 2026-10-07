# quorum-agent

A tiny agent for the WireGuard container. It runs `wg show all dump` and serves the
peers as JSON, so Quorum can show WireGuard without access to the container's shell.

- `GET /peers` (needs `Authorization: Bearer <token>`): interfaces (name, public key,
  port) and peers (public key, optional name, endpoint, allowed IPs, last handshake,
  received/sent bytes, keepalive).
- `GET /version`: the agent version. No token needed.

**Never sent:** the interface private key and peer preshared keys. The dump has them; the
agent drops them while parsing.

Standard library only, about 400 KB. Plain HTTP: bind it to the container's LAN address and
set `QUORUM_AGENT_ALLOW` to the Quorum server's IP so only Quorum can connect.

## Install

On your machine:

```sh
cd agent
cargo build --release
scp target/release/quorum-agent deploy/* root@<wireguard-container>:/tmp/
```

In the WireGuard container:

```sh
/tmp/install.sh /tmp/quorum-agent <quorum-server-ip>
```

The script creates a `quorum-agent` user, a random token in `/etc/quorum-agent/token`, and
a systemd unit (`deploy/quorum-agent.service`) that gives the agent only `CAP_NET_ADMIN`
(needed by `wg show`). On Alpine/OpenRC it runs as root. It prints the base URL and token to
enter in Quorum under Settings → Services → WireGuard.

## Configuration

`/etc/quorum-agent/agent.env`:

| Variable | Default | |
| --- | --- | --- |
| `QUORUM_AGENT_BIND` | `0.0.0.0:9587` | Listen address |
| `QUORUM_AGENT_ALLOW` | (any) | Comma-separated client IPs allowed to connect |
| `QUORUM_AGENT_TOKEN_FILE` | `/etc/quorum-agent/token` | At least 32 characters |
| `QUORUM_AGENT_NAMES` | `/etc/quorum-agent/names` | `<public key> <name>` per line |
| `QUORUM_AGENT_WG` | `wg` | Path of the `wg` binary |

Peer names are optional: WireGuard itself has none. Add a line per peer to
`/etc/quorum-agent/names`, e.g. `xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg= Janick's phone`.

## Develop

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```
