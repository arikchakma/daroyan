#!/bin/sh
# Installs the Daroyan API + dashboard as a systemd service. Run as root with
# either inline settings:
#   curl -fsSL https://daroyan.com/install-server.sh | sudo \
#     DAROYAN_APP_URL=https://monitor.example.com \
#     GOOGLE_CLIENT_ID=... GOOGLE_CLIENT_SECRET=... ALLOWED_EMAILS=you@example.com sh
# or a prepared env file (keeps secrets out of your shell history and `ps`):
#   curl -fsSL https://daroyan.com/install-server.sh | sudo DAROYAN_ENV_FILE=/root/daroyan.env sh
set -eu

REPO="${DAROYAN_REPO:-arikchakma/daroyan}"
VERSION="${DAROYAN_VERSION:-latest}"
DATA_DIR=/var/lib/daroyan
ENV_FILE=/etc/daroyan.env

if [ "$(id -u)" -ne 0 ]; then
  echo "Run this script as root." >&2
  exit 1
fi
if [ ! -d /run/systemd/system ]; then
  echo "This installer needs systemd. Use Docker instead on other init systems." >&2
  exit 1
fi

# Only an amd64 API binary is published. Refuse elsewhere rather than install a
# binary that cannot run.
case "$(uname -m)" in
  x86_64 | amd64) ;;
  *)
    echo "Only linux/amd64 has a published API binary; this host is $(uname -m)." >&2
    echo "Build from source for this architecture, or run on an amd64 host." >&2
    exit 1
    ;;
esac

if [ "$VERSION" = latest ]; then
  BASE="https://github.com/$REPO/releases/latest/download"
else
  BASE="https://github.com/$REPO/releases/download/$VERSION"
fi

fetch() { curl -fsSL --proto-redir '=https' "$1" -o "$2"; }

# API binary (embeds the dashboard), written then renamed so a running server
# never reads a half-written file.
TMP=/usr/local/bin/.daroyan-api.new
fetch "$BASE/daroyan-api-x86_64-linux-gnu" "$TMP"
chmod 755 "$TMP"
mv -f "$TMP" /usr/local/bin/daroyan-api

id daroyan >/dev/null 2>&1 || \
  useradd --system --no-create-home --shell /usr/sbin/nologin daroyan

mkdir -p "$DATA_DIR/backups"
chown -R daroyan "$DATA_DIR"

# Config precedence: an explicit env file wins; otherwise keep an existing
# /etc/daroyan.env (so re-running to upgrade is safe); otherwise generate one
# from the inline settings.
if [ -n "${DAROYAN_ENV_FILE:-}" ]; then
  [ -f "$DAROYAN_ENV_FILE" ] || { echo "DAROYAN_ENV_FILE not found: $DAROYAN_ENV_FILE" >&2; exit 1; }
  install -m 600 -o root "$DAROYAN_ENV_FILE" "$ENV_FILE"
elif [ ! -f "$ENV_FILE" ]; then
  : "${DAROYAN_APP_URL:?Set DAROYAN_APP_URL, or pass DAROYAN_ENV_FILE=/path to a prepared env file}"
  (umask 077 && cat > "$ENV_FILE") <<EOF
APP_URL=$DAROYAN_APP_URL
GOOGLE_CLIENT_ID=${GOOGLE_CLIENT_ID:-}
GOOGLE_CLIENT_SECRET=${GOOGLE_CLIENT_SECRET:-}
ALLOWED_EMAILS=${ALLOWED_EMAILS:-}
HOST=127.0.0.1
PORT=8900
DATABASE_URL=sqlite://$DATA_DIR/daroyan.db?mode=rwc
DUCKDB_PATH=$DATA_DIR/daroyan.duckdb
DUCKDB_BACKUP_DIR=$DATA_DIR/backups
CLIENT_IP_HEADER=x-forwarded-for
TRUSTED_PROXIES=127.0.0.1,::1
EOF
  chmod 600 "$ENV_FILE"
fi

cat > /etc/systemd/system/daroyan-api.service <<EOF
[Unit]
Description=Daroyan API + dashboard
After=network-online.target
Wants=network-online.target

[Service]
User=daroyan
EnvironmentFile=$ENV_FILE
ExecStart=/usr/local/bin/daroyan-api
Restart=on-failure
RestartSec=5
WorkingDirectory=$DATA_DIR

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now daroyan-api
systemctl restart daroyan-api

APP_URL=$(sed -n 's/^APP_URL=//p' "$ENV_FILE" | head -n1)
echo "Daroyan is running on 127.0.0.1:8900."
echo "Point your reverse proxy at it, then open ${APP_URL:-your public URL}."
echo "Edit $ENV_FILE to change settings, then: systemctl restart daroyan-api"
