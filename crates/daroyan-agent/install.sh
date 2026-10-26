#!/bin/sh
# Installs the Daroyan agent as a service. Run as root:
#   curl -fsSL https://daroyan.com/install.sh | sudo DAROYAN_TOKEN=dy_agent_... sh
set -eu

: "${DAROYAN_TOKEN:?Set DAROYAN_TOKEN to an install token from the dashboard}"
API_URL="${DAROYAN_API_URL:-https://daroyan.com}"
BINARY="${DAROYAN_BINARY:-}"
CONFIG_DIR=/etc/daroyan
CONFIG="$CONFIG_DIR/agent.yml"

case "$API_URL" in
  https://* | http://127.0.0.1 | http://127.0.0.1:* | http://localhost | http://localhost:*) ;;
  *)
    if [ "${DAROYAN_ALLOW_HTTP:-}" != 1 ]; then
      echo "DAROYAN_API_URL must start with https://. Set DAROYAN_ALLOW_HTTP=1 to send the token unencrypted." >&2
      exit 1
    fi
    ;;
esac

if [ "$(id -u)" -ne 0 ]; then
  echo "Run this script as root." >&2
  exit 1
fi

case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

# Write the new binary next to the old one, then rename it into place, so a
# running agent never reads a half-written file.
TMP_BINARY=/usr/local/bin/.daroyan.new
if [ -n "$BINARY" ]; then
  cp "$BINARY" "$TMP_BINARY"
else
  URL="https://github.com/arikchakma/daroyan/releases/download/v__DAROYAN_VERSION__/daroyan-$ARCH-linux-musl"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --proto-redir '=https' "$URL" -o "$TMP_BINARY"
  else
    wget -qO "$TMP_BINARY" "$URL"
  fi
fi
chmod 755 "$TMP_BINARY"
"$TMP_BINARY" --version >/dev/null
mv -f "$TMP_BINARY" /usr/local/bin/daroyan

if ! id daroyan >/dev/null 2>&1; then
  if command -v useradd >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin daroyan
  else
    addgroup -S daroyan 2>/dev/null || true
    adduser -S -D -H -G daroyan -s /sbin/nologin daroyan
  fi
fi

# Reading journald and /var/log files for log collection needs these groups.
for group in systemd-journal adm; do
  if getent group "$group" >/dev/null 2>&1; then
    usermod -aG "$group" daroyan 2>/dev/null || addgroup daroyan "$group" 2>/dev/null || true
  fi
done

mkdir -p "$CONFIG_DIR/conf.d" /var/lib/daroyan /var/log/daroyan
chown daroyan /var/lib/daroyan /var/log/daroyan
chmod 700 /var/lib/daroyan

if [ ! -f "$CONFIG" ]; then
  (umask 077 && cat > "$CONFIG") <<EOF
api:
  url: $API_URL
  token: $DAROYAN_TOKEN

server:
  name: $(hostname)
  labels: []

# Add what to watch. Each entry has one of systemd, docker or process.
# processes:
#   - systemd: nginx
# ports:
#   - port: 443

# Let admins start, stop and restart the processes above from the dashboard.
# agent:
#   commands: true
EOF
fi
chown root:daroyan "$CONFIG"
chmod 640 "$CONFIG"

if [ -d /run/systemd/system ]; then
  cat > /etc/systemd/system/daroyan.service <<'EOF'
[Unit]
Description=Daroyan monitoring agent
After=network-online.target
Wants=network-online.target

[Service]
User=daroyan
ExecStart=/usr/local/bin/daroyan start
Restart=on-failure
RestartSec=5
# Reading /proc/<pid>/io of other users' processes needs these two capabilities.
AmbientCapabilities=CAP_SYS_PTRACE CAP_DAC_READ_SEARCH

[Install]
WantedBy=multi-user.target
EOF
  # Lets the agent start, stop and restart units and containers, and find a
  # container's processes with docker ps. The agent refuses anything that is
  # not in processes in its config. Polkit covers systemctl when it is
  # installed; sudo covers the rest.
  SUDO_RULES=""
  if [ -d /etc/polkit-1/rules.d ]; then
    cat > /etc/polkit-1/rules.d/50-daroyan.rules <<'EOF'
polkit.addRule(function (action, subject) {
  if (action.id != "org.freedesktop.systemd1.manage-units" || subject.user != "daroyan") {
    return polkit.Result.NOT_HANDLED;
  }
  var unit = action.lookup("unit") || "";
  var verb = action.lookup("verb");
  if (/\.service$/.test(unit) && unit != "debug-shell.service" &&
      (verb == "start" || verb == "stop" || verb == "restart")) {
    return polkit.Result.YES;
  }
});
EOF
  else
    SYSTEMCTL="$(command -v systemctl)"
    SUDO_RULES="$SYSTEMCTL start *, $SYSTEMCTL stop *, $SYSTEMCTL restart *"
  fi
  if command -v docker >/dev/null 2>&1; then
    DOCKER="$(command -v docker)"
    SUDO_RULES="${SUDO_RULES:+$SUDO_RULES, }$DOCKER start *, $DOCKER stop *, $DOCKER restart *, $DOCKER ps *, $DOCKER logs *"
  fi
  if [ -n "$SUDO_RULES" ]; then
    if [ -d /etc/sudoers.d ] && command -v visudo >/dev/null 2>&1; then
      echo "daroyan ALL=(root) NOPASSWD: $SUDO_RULES" > /etc/sudoers.d/daroyan
      chmod 440 /etc/sudoers.d/daroyan
      if ! visudo -cf /etc/sudoers.d/daroyan >/dev/null; then
        rm -f /etc/sudoers.d/daroyan
        echo "Could not add the sudoers entry. Process restarts will not work." >&2
      fi
    else
      echo "No sudo found. Process restarts will not work." >&2
    fi
  fi
  systemctl daemon-reload
  systemctl enable --now daroyan
  systemctl restart daroyan
elif command -v rc-service >/dev/null 2>&1; then
  cat > /etc/init.d/daroyan <<'EOF'
#!/sbin/openrc-run
description="Daroyan monitoring agent"
command=/usr/local/bin/daroyan
command_args="start"
command_user=daroyan
command_background=true
pidfile=/run/daroyan.pid
depend() {
  need net
}
EOF
  chmod 755 /etc/init.d/daroyan
  chown root:daroyan /usr/local/bin/daroyan
  chmod 750 /usr/local/bin/daroyan
  if command -v setcap >/dev/null 2>&1; then
    setcap cap_sys_ptrace,cap_dac_read_search+ep /usr/local/bin/daroyan
  else
    echo "setcap was not found, so process disk I/O shows as 0. Install libcap to fix it." >&2
  fi
  rc-update add daroyan default >/dev/null
  rc-service daroyan restart
else
  echo "No systemd or OpenRC found. Start the agent with: /usr/local/bin/daroyan start" >&2
  exit 1
fi

echo "The Daroyan agent is running. It reports to $API_URL."
