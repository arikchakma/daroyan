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

# Runs before the binary is replaced, because the new file drops setcap capabilities.
if [ -z "$COMMANDS" ]; then
  COMMANDS=0
  if grep -qs -- "grants .*--commands" /etc/systemd/system/daroyan.service /etc/init.d/daroyan ||
    [ -f /etc/polkit-1/rules.d/50-daroyan.rules ] || grep -qs " restart " /etc/sudoers.d/daroyan; then
    COMMANDS=1
  fi
fi
if [ -z "$PROCESS_IO" ]; then
  PROCESS_IO=0
  if [ -f /etc/systemd/system/daroyan-io.service ] || [ -f /etc/init.d/daroyan-io ]; then
    PROCESS_IO=1
  elif grep -qs "^AmbientCapabilities=.*CAP_SYS_PTRACE" /etc/systemd/system/daroyan.service; then
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

add_system_user() {
  if id "$1" >/dev/null 2>&1; then
    return
  fi
  if command -v useradd >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin "$1"
  else
    addgroup -S "$1" 2>/dev/null || true
    adduser -S -D -H -G "$1" -s /sbin/nologin "$1"
  fi
}

add_system_user daroyan
if [ "$PROCESS_IO" = 1 ]; then
  add_system_user daroyan-io
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

GRANTS="/usr/local/bin/daroyan grants --config $CONFIG"
if [ "$COMMANDS" = 1 ]; then
  GRANTS="$GRANTS --commands"
fi

NEEDS_SUDO=0
if command -v docker >/dev/null 2>&1; then
  NEEDS_SUDO=1
elif [ "$COMMANDS" = 1 ] && [ ! -d /etc/polkit-1/rules.d ]; then
  NEEDS_SUDO=1
fi
if [ "$NEEDS_SUDO" = 1 ] && ! command -v visudo >/dev/null 2>&1; then
  echo "No sudo found. Docker checks and process restarts will not work." >&2
fi

if [ -d /run/systemd/system ]; then
  {
    cat <<EOF
[Unit]
Description=Daroyan monitoring agent
After=network-online.target
Wants=network-online.target

[Service]
User=daroyan
ExecStartPre=+$GRANTS
ExecStart=/usr/local/bin/daroyan start
Restart=on-failure
RestartSec=5
EOF
    if [ "$NEEDS_SUDO" = 0 ]; then
      echo "NoNewPrivileges=yes"
    fi
    cat <<'EOF'

[Install]
WantedBy=multi-user.target
EOF
  } > /etc/systemd/system/daroyan.service

  if [ "$PROCESS_IO" = 1 ]; then
    cat > /etc/systemd/system/daroyan-io.socket <<'EOF'
[Unit]
Description=Daroyan disk I/O helper socket

[Socket]
ListenStream=/run/daroyan-io/io.sock
SocketGroup=daroyan
SocketMode=0660

[Install]
WantedBy=sockets.target
EOF
    cat > /etc/systemd/system/daroyan-io.service <<'EOF'
[Unit]
Description=Daroyan disk I/O helper
Requires=daroyan-io.socket

[Service]
ExecStart=/usr/local/bin/daroyan io-helper
StandardInput=socket
StandardOutput=journal
User=daroyan-io
Group=daroyan-io
# Reading /proc/<pid>/io of other users' processes needs these two capabilities.
AmbientCapabilities=CAP_SYS_PTRACE CAP_DAC_READ_SEARCH
CapabilityBoundingSet=CAP_SYS_PTRACE CAP_DAC_READ_SEARCH
NoNewPrivileges=yes
TemporaryFileSystem=/
BindReadOnlyPaths=/proc /usr/local/bin/daroyan /etc/passwd
ProcSubset=pid
PrivateNetwork=yes
RestrictAddressFamilies=AF_UNIX
PrivateDevices=yes
PrivateIPC=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
ProtectClock=yes
ProtectHostname=yes
RestrictNamespaces=yes
RestrictRealtime=yes
RestrictSUIDSGID=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes
SystemCallArchitectures=native
SystemCallFilter=@system-service seccomp
SystemCallFilter=~@debug @mount process_vm_readv process_vm_writev open_by_handle_at name_to_handle_at
SystemCallErrorNumber=EPERM
UMask=0077
EOF
  else
    systemctl disable --now daroyan-io.socket daroyan-io.service >/dev/null 2>&1 || true
    rm -f /etc/systemd/system/daroyan-io.socket /etc/systemd/system/daroyan-io.service
    rm -rf /run/daroyan-io
  fi

  systemctl daemon-reload
  if [ "$PROCESS_IO" = 1 ]; then
    systemctl enable --now daroyan-io.socket
    systemctl try-restart daroyan-io.service
  fi
  systemctl enable --now daroyan
  systemctl restart daroyan
elif command -v rc-service >/dev/null 2>&1; then
  cat > /etc/init.d/daroyan <<EOF
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
start_pre() {
  $GRANTS
}
EOF
  chmod 755 /etc/init.d/daroyan

  if [ "$PROCESS_IO" = 1 ]; then
    cat > /etc/init.d/daroyan-io <<'EOF'
#!/sbin/openrc-run
description="Daroyan disk I/O helper"
supervisor=supervise-daemon
command=/usr/local/bin/daroyan
command_args="io-helper"
command_user="daroyan-io:daroyan-io"
capabilities="^cap_sys_ptrace,^cap_dac_read_search"
no_new_privs=yes
start_pre() {
  checkpath -d -o daroyan-io:daroyan -m 0750 /run/daroyan-io
}
EOF
    chmod 755 /etc/init.d/daroyan-io
    rc-update add daroyan-io default >/dev/null
    rc-service daroyan-io restart
  elif [ -f /etc/init.d/daroyan-io ]; then
    rc-service daroyan-io stop >/dev/null 2>&1 || true
    rc-update del daroyan-io default >/dev/null 2>&1 || true
    rm -f /etc/init.d/daroyan-io
    rm -rf /run/daroyan-io
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
