#!/bin/sh
# Installs the Daroyan agent as a service. Run as root:
#   curl -fsSL https://daroyan.com/install.sh | sudo DAROYAN_TOKEN=dy_agent_... sh
set -eu

: "${DAROYAN_TOKEN:?Set DAROYAN_TOKEN to an install token from the dashboard}"
API_URL="${DAROYAN_API_URL:-https://daroyan.com}"
BINARY="${DAROYAN_BINARY:-}"
COMMANDS="${DAROYAN_COMMANDS:-}"
PROCESS_IO="${DAROYAN_PROCESS_IO:-}"
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

for flag in "$COMMANDS" "$PROCESS_IO"; do
  case "$flag" in
    "" | 0 | 1) ;;
    *) echo "DAROYAN_COMMANDS and DAROYAN_PROCESS_IO take 1 or 0." >&2; exit 1 ;;
  esac
done

# A rerun without a flag keeps what the server already has. This runs before
# the binary is replaced, because the new file drops setcap capabilities.
if [ -z "$COMMANDS" ]; then
  COMMANDS=0
  if [ -f /etc/polkit-1/rules.d/50-daroyan.rules ] || grep -qs " restart \*" /etc/sudoers.d/daroyan; then
    COMMANDS=1
  fi
fi
if [ -z "$PROCESS_IO" ]; then
  PROCESS_IO=0
  if grep -qs "^AmbientCapabilities=.*CAP_SYS_PTRACE" /etc/systemd/system/daroyan.service; then
    PROCESS_IO=1
  elif command -v getcap >/dev/null 2>&1 && getcap /usr/local/bin/daroyan 2>/dev/null | grep -q cap_sys_ptrace; then
    PROCESS_IO=1
  fi
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
# Restarts need the agent installed with DAROYAN_COMMANDS=1.
# agent:
#   commands: true
EOF
fi
chown root:daroyan "$CONFIG"
chmod 640 "$CONFIG"

# Lets the agent find a container's processes and read its details, events
# and logs, and, with DAROYAN_COMMANDS=1, start, stop and restart containers.
# The agent refuses anything that is not in processes in its config.
add_docker_rules() {
  if ! command -v docker >/dev/null 2>&1; then
    return
  fi
  DOCKER="$(command -v docker)"
  RULES="$DOCKER ps *, $DOCKER inspect *, $DOCKER logs *, $DOCKER events *"
  if [ "$COMMANDS" = 1 ]; then
    RULES="$DOCKER start *, $DOCKER stop *, $DOCKER restart *, $RULES"
  fi
  SUDO_RULES="${SUDO_RULES:+$SUDO_RULES, }$RULES"
}

write_sudoers() {
  if [ -z "$SUDO_RULES" ]; then
    rm -f /etc/sudoers.d/daroyan
    return
  fi
  if [ -d /etc/sudoers.d ] && command -v visudo >/dev/null 2>&1; then
    echo "daroyan ALL=(root) NOPASSWD: $SUDO_RULES" > /etc/sudoers.d/daroyan
    chmod 440 /etc/sudoers.d/daroyan
    if ! visudo -cf /etc/sudoers.d/daroyan >/dev/null; then
      rm -f /etc/sudoers.d/daroyan
      echo "Could not add the sudoers entry. Docker checks and process restarts will not work." >&2
    fi
  else
    echo "No sudo found. Docker checks and process restarts will not work." >&2
  fi
}

SUDO_RULES=""
if [ -d /run/systemd/system ]; then
  # Polkit covers systemctl when it is installed; sudo covers the rest.
  if [ "$COMMANDS" = 0 ]; then
    rm -f /etc/polkit-1/rules.d/50-daroyan.rules
  elif [ -d /etc/polkit-1/rules.d ]; then
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
  add_docker_rules
  write_sudoers

  {
    cat <<'EOF'
[Unit]
Description=Daroyan monitoring agent
After=network-online.target
Wants=network-online.target

[Service]
User=daroyan
ExecStart=/usr/local/bin/daroyan start
Restart=on-failure
RestartSec=5
EOF
    if [ "$PROCESS_IO" = 1 ]; then
      echo "# Reading /proc/<pid>/io of other users' processes needs these two capabilities."
      echo "AmbientCapabilities=CAP_SYS_PTRACE CAP_DAC_READ_SEARCH"
    fi
    if [ -z "$SUDO_RULES" ]; then
      echo "NoNewPrivileges=yes"
    fi
    cat <<'EOF'

[Install]
WantedBy=multi-user.target
EOF
  } > /etc/systemd/system/daroyan.service
  systemctl daemon-reload
  systemctl enable --now daroyan
  systemctl restart daroyan
elif command -v rc-service >/dev/null 2>&1; then
  add_docker_rules
  write_sudoers

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
  if [ "$PROCESS_IO" = 1 ]; then
    if command -v setcap >/dev/null 2>&1; then
      setcap cap_sys_ptrace,cap_dac_read_search+ep /usr/local/bin/daroyan
    else
      echo "setcap was not found, so process disk I/O stays off. Install libcap to fix it." >&2
    fi
  fi
  rc-update add daroyan default >/dev/null
  rc-service daroyan restart
else
  echo "No systemd or OpenRC found. Start the agent with: /usr/local/bin/daroyan start" >&2
  exit 1
fi

echo "The Daroyan agent is running. It reports to $API_URL."
if [ "$COMMANDS" = 0 ]; then
  echo "Process restarts are off. Rerun with DAROYAN_COMMANDS=1 to allow them."
fi
if [ "$PROCESS_IO" = 0 ]; then
  echo "Per-process disk I/O is off. Rerun with DAROYAN_PROCESS_IO=1 to collect it."
fi
