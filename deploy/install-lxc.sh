#!/bin/sh
# Install Quorum in a Debian or Alpine LXC container.
# Usage: ./install-lxc.sh /path/to/quorum-server
set -eu

BIN="${1:?usage: $0 /path/to/quorum-server}"
HERE="$(cd "$(dirname "$0")" && pwd)"

if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root." >&2
    exit 1
fi

if ! id quorum >/dev/null 2>&1; then
    if command -v adduser >/dev/null 2>&1 && [ -f /etc/alpine-release ]; then
        adduser -D -H -s /sbin/nologin quorum
    else
        useradd --system --no-create-home --shell /usr/sbin/nologin quorum
    fi
fi

install -m 0755 "$BIN" /usr/local/bin/quorum-server
install -d -m 0750 -o quorum -g quorum /var/lib/quorum
install -d -m 0755 /etc/quorum

if [ ! -f /etc/quorum/quorum.toml ]; then
    install -m 0644 "$HERE/quorum.example.toml" /etc/quorum/quorum.toml
    echo "Wrote /etc/quorum/quorum.toml: set public_url."
fi

if [ ! -f /etc/quorum/quorum.env ]; then
    umask 077
    KEY="$(head -c 32 /dev/urandom | base64)"
    printf 'QUORUM_SECRET_KEY=%s\n' "$KEY" > /etc/quorum/quorum.env
    echo "Generated the master key in /etc/quorum/quorum.env. Back it up: without it, stored secrets cannot be read."
fi

if command -v systemctl >/dev/null 2>&1; then
    install -m 0644 "$HERE/quorum.service" /etc/systemd/system/quorum.service
    systemctl daemon-reload
    systemctl enable --now quorum
    echo "Started. Find the one-time setup token with: journalctl -u quorum | grep 'setup token'"
elif command -v rc-service >/dev/null 2>&1; then
    cat > /etc/init.d/quorum <<'RC'
#!/sbin/openrc-run
description="Quorum homelab control center"
command=/usr/local/bin/quorum-server
command_args="serve"
command_user="quorum:quorum"
command_background=true
pidfile=/run/quorum.pid
output_log=/var/log/quorum.log
error_log=/var/log/quorum.log
start_pre() {
    set -a; . /etc/quorum/quorum.env; set +a
    export QUORUM_CONFIG=/etc/quorum/quorum.toml
    touch /var/log/quorum.log && chown quorum:quorum /var/log/quorum.log
}
RC
    chmod 0755 /etc/init.d/quorum
    rc-update add quorum default
    rc-service quorum start
    echo "Started. Find the one-time setup token with: grep 'setup token' /var/log/quorum.log"
else
    echo "No systemd or OpenRC found; start /usr/local/bin/quorum-server yourself." >&2
fi
