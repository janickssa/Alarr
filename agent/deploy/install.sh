#!/bin/sh
# Install the Quorum agent in the WireGuard container (Debian or Alpine).
# Usage: ./install.sh /path/to/quorum-agent [quorum-server-ip]
# Prints the token to paste into Quorum (Settings → Services → WireGuard).
set -eu

BIN="${1:?usage: $0 /path/to/quorum-agent [quorum-server-ip]}"
ALLOW="${2:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"

if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root." >&2
    exit 1
fi
if ! command -v wg >/dev/null 2>&1; then
    echo "wg (wireguard-tools) is not installed in this container." >&2
    exit 1
fi

if ! id quorum-agent >/dev/null 2>&1; then
    if [ -f /etc/alpine-release ]; then
        adduser -D -H -s /sbin/nologin quorum-agent
    else
        useradd --system --no-create-home --shell /usr/sbin/nologin quorum-agent
    fi
fi

install -m 0755 "$BIN" /usr/local/bin/quorum-agent
install -d -m 0750 -o root -g quorum-agent /etc/quorum-agent

if [ ! -f /etc/quorum-agent/token ]; then
    umask 027
    head -c 32 /dev/urandom | base64 | tr -d '\n=' > /etc/quorum-agent/token
    chgrp quorum-agent /etc/quorum-agent/token
fi
if [ ! -f /etc/quorum-agent/names ]; then
    printf '# <peer public key> <name>, one per line. Optional.\n' > /etc/quorum-agent/names
    chgrp quorum-agent /etc/quorum-agent/names
    chmod 0640 /etc/quorum-agent/names
fi
if [ ! -f /etc/quorum-agent/agent.env ]; then
    printf 'QUORUM_AGENT_BIND=0.0.0.0:9587\nQUORUM_AGENT_ALLOW=%s\n' "$ALLOW" > /etc/quorum-agent/agent.env
    chmod 0640 /etc/quorum-agent/agent.env
    chgrp quorum-agent /etc/quorum-agent/agent.env
fi

if command -v systemctl >/dev/null 2>&1; then
    install -m 0644 "$HERE/quorum-agent.service" /etc/systemd/system/quorum-agent.service
    systemctl daemon-reload
    systemctl enable --now quorum-agent
    systemctl restart quorum-agent
elif command -v rc-service >/dev/null 2>&1; then
    # OpenRC has no ambient capabilities; run as root there.
    cat > /etc/init.d/quorum-agent <<'RC'
#!/sbin/openrc-run
description="Quorum agent (WireGuard peers for Quorum)"
command=/usr/local/bin/quorum-agent
command_background=true
pidfile=/run/quorum-agent.pid
output_log=/var/log/quorum-agent.log
error_log=/var/log/quorum-agent.log
depend() { need net; }
start_pre() { set -a; . /etc/quorum-agent/agent.env; set +a; }
RC
    chmod 0755 /etc/init.d/quorum-agent
    rc-update add quorum-agent default
    rc-service quorum-agent restart
else
    echo "No systemd or OpenRC found; start /usr/local/bin/quorum-agent yourself." >&2
fi

echo
echo "Quorum agent installed. In Quorum, add WireGuard with:"
echo "  Base URL:    http://<this container's IP>:9587"
echo "  Agent token: $(cat /etc/quorum-agent/token)"
