# Deploying Daroyan

Daroyan ships as a **single API binary that embeds the dashboard**. There is no
separate frontend to host: you run `daroyan-api`, and the dashboard is served at
`/`, the JSON API under `/v1`, and public status pages under `/v1/status/...`.
This is the same whether you use Docker or not — the only difference is how the
binary is packaged and supervised.

The server stores everything in two files on local disk (SQLite for app data,
DuckDB for metrics/logs/traces), so a single host with a volume is enough to
start. Put TLS in front of it with a reverse proxy.

---

## 1. Prerequisites

- A domain pointing at the host, e.g. `https://monitor.example.com`.
- A reverse proxy terminating TLS (Caddy, nginx, or Cloudflare). The app speaks
  plain HTTP and expects TLS to be terminated in front of it.
- A **Google OAuth client** — Google sign-in is the only login method.
  - Create one at <https://console.cloud.google.com/apis/credentials>.
  - Authorized redirect URI: `https://YOUR_DOMAIN/v1/auth/google/callback`
  - Note the Client ID and Client Secret.

---

## 2. Environment variables

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `APP_URL` | **Yes** | – | Public base URL. Used for OAuth redirects and status-page links. Must match the Google redirect URI host. |
| `GOOGLE_CLIENT_ID` | **Yes** | – | Google OAuth client ID (needed to log in). |
| `GOOGLE_CLIENT_SECRET` | **Yes** | – | Google OAuth client secret. |
| `ALLOWED_EMAILS` | **Yes (first run)** | empty | Comma-separated emails, or `@domain` for a whole domain, that are **admins** automatically. Set this to your own email to bootstrap the first admin, then invite others from the UI. |
| `HOST` | No | `127.0.0.1` | Bind address. Set to `0.0.0.0` (or `::`) when a proxy on another host/container must reach it. The Docker image defaults to `::`. |
| `PORT` | No | `8900` | Listen port. |
| `DATABASE_URL` | No | `sqlite://data/daroyan.db?mode=rwc` | SQLite app database. Point it at your persistent volume. |
| `DUCKDB_PATH` | No | `data/daroyan.duckdb` | DuckDB metrics/logs/traces store. Put it on the same volume. |
| `DUCKDB_BACKUP_DIR` | No | – | If set, periodic DuckDB backups are written here. |
| `RETENTION_DAYS` | No | `30` | Days of metrics/traces kept. |
| `LOG_RETENTION_DAYS` | No | `7` | Days of logs kept. |
| `OTLP_LOG_BUDGET_KB` | No | `1024` | The most OpenTelemetry log data each ingest token can send per minute, in KB. Batches over it get `429` with `Retry-After`, so SDKs retry them. Settings → Telemetry counts them. |
| `TRUSTED_PROXIES` | No | `127.0.0.1,::1` | CIDRs allowed to set the real-client-IP header. See the warning below. |
| `CLIENT_IP_HEADER` | No | `cf-connecting-ip` | Header your proxy sets with the real client IP. Use `x-forwarded-for` for most proxies. |
| `ALLOW_PRIVATE_TARGETS` | No | `false` | Allow uptime checks against private/localhost addresses. **Keep `false` in production** (it is the SSRF guard). Only enable if you monitor LAN hosts. |
| `SLACK_CLIENT_ID` / `SLACK_CLIENT_SECRET` | No | – | Slack integration. |
| `DISCORD_CLIENT_ID` / `DISCORD_CLIENT_SECRET` | No | – | Discord integration. |
| `TELEGRAM_BOT_TOKEN` | No | – | Telegram integration. |

> **Security — `TRUSTED_PROXIES`:** the client IP drives rate limiting and audit
> logs. Only list the exact address your reverse proxy connects *from*. Never add
> a broad or shared network (e.g. a shared Docker bridge like `172.16.0.0/12`),
> or a co-located container could spoof `CLIENT_IP_HEADER` and bypass rate limits.
> If the proxy is on the same host over loopback, the default is fine.

---

## 3. Option A — Docker

The image is published to `ghcr.io/arikchakma/daroyan/api` and is **linux/amd64
only**. On ARM hosts, use the no-Docker path below (or build your own image).

`compose.yml`:

```yaml
services:
  daroyan:
    image: ghcr.io/arikchakma/daroyan/api:0.0.1
    restart: unless-stopped
    environment:
      APP_URL: https://monitor.example.com
      GOOGLE_CLIENT_ID: your-client-id
      GOOGLE_CLIENT_SECRET: your-client-secret
      ALLOWED_EMAILS: you@example.com
      DATABASE_URL: sqlite:///app/data/daroyan.db?mode=rwc
      DUCKDB_PATH: /app/data/daroyan.duckdb
      DUCKDB_BACKUP_DIR: /app/data/backups
      CLIENT_IP_HEADER: x-forwarded-for
      TRUSTED_PROXIES: 127.0.0.1,::1
    volumes:
      - ./data:/app/data
    ports:
      - "127.0.0.1:8900:8900"   # only the reverse proxy needs to reach it
```

```sh
docker compose up -d
docker compose logs -f daroyan   # confirm "listening on ..."
```

`install.sh` served from your domain downloads the agent from the GitHub
release matching your server version.

---

## 4. Option B — Without Docker (systemd, one command)

The dashboard is inside the binary, so this is just the API binary plus a
service unit. `install-server.sh` does all of it — downloads the binary, creates the `daroyan` user and data dir, writes the env file and
a systemd unit, and starts the service:

```sh
curl -fsSL https://raw.githubusercontent.com/arikchakma/daroyan/main/crates/daroyan-api/install-server.sh | \
  sudo DAROYAN_APP_URL=https://monitor.example.com \
       GOOGLE_CLIENT_ID=your-client-id \
       GOOGLE_CLIENT_SECRET=your-client-secret \
       ALLOWED_EMAILS=you@example.com sh
```

**Prefer a file for secrets.** Inline vars land in your shell history and in
`ps` output. Write a prepared env file (see the table in section 2 for the keys)
and pass it instead — the installer copies it to `/etc/daroyan.env` with `600`
perms:

```sh
cat > /root/daroyan.env <<'EOF'
APP_URL=https://monitor.example.com
GOOGLE_CLIENT_ID=your-client-id
GOOGLE_CLIENT_SECRET=your-client-secret
ALLOWED_EMAILS=you@example.com
CLIENT_IP_HEADER=x-forwarded-for
DATABASE_URL=sqlite:///var/lib/daroyan/daroyan.db?mode=rwc
DUCKDB_PATH=/var/lib/daroyan/daroyan.duckdb
DUCKDB_BACKUP_DIR=/var/lib/daroyan/backups
EOF

curl -fsSL https://raw.githubusercontent.com/arikchakma/daroyan/main/crates/daroyan-api/install-server.sh | \
  sudo DAROYAN_ENV_FILE=/root/daroyan.env sh
```

When you pass a file it is used **verbatim** (not merged), so include the data
paths above — otherwise the DB lands in a default subpath. Any key you leave out falls back to its
built-in default from section 2. It installs to
`/usr/local/bin/daroyan-api`, keeps data in `/var/lib/daroyan`, and config in
`/etc/daroyan.env`. Pin a release with `DAROYAN_VERSION=v0.0.1` (default:
`latest`). Re-running upgrades the binary and **keeps** your existing
`/etc/daroyan.env` (unless you pass `DAROYAN_ENV_FILE` again).

```sh
sudo journalctl -u daroyan-api -f   # confirm "listening on ..."
```

Change any setting by editing `/etc/daroyan.env`, then
`sudo systemctl restart daroyan-api`. (See the env table in section 2.)

> The manual equivalent is just: download `daroyan-api` from the release, create
> a service user + data dir, write `/etc/daroyan.env`, and drop a systemd unit
> with `EnvironmentFile=/etc/daroyan.env` and `ExecStart=/usr/local/bin/daroyan-api`.
> The script is only automating those five steps.

---

## 5. Reverse proxy (TLS)

The proxy terminates TLS and forwards the real client IP. Example with Caddy —
note it sets `X-Forwarded-For`, which matches `CLIENT_IP_HEADER=x-forwarded-for`:

```
monitor.example.com {
    encode gzip
    reverse_proxy 127.0.0.1:8900 {
        header_up X-Forwarded-For {remote_host}
    }
    header Strict-Transport-Security "max-age=31536000; includeSubDomains"
}
```

The app sends its own CSP, `X-Frame-Options`, `X-Content-Type-Options`, and
`Referrer-Policy`. Add HSTS at the proxy as shown (the app does not set it,
since it does not terminate TLS).

---

## 6. First login

1. Open `https://monitor.example.com`.
2. Sign in with Google using an address listed in `ALLOWED_EMAILS` — you become
   an admin.
3. Invite teammates from **Settings → Members** (they sign in with Google; you
   choose admin or member).

`ALLOWED_EMAILS` admins are always admins and cannot be removed from the UI;
everyone else is managed through the members list.

---

## 7. Connecting agents

On each host you want to monitor, create an install token in the dashboard
(**admin only**), then:

```sh
curl -fsSL https://monitor.example.com/install.sh | \
  sudo DAROYAN_TOKEN=dy_agent_... DAROYAN_API_URL=https://monitor.example.com sh
```

- Install tokens are **single-use and last 24 hours by default**. For Ansible,
  cloud-init or other automation, create a key in **Settings → Agents** that
  never expires and works for any number of servers. Revoking a key stops new
  enrollments; servers that already enrolled keep their own token.
- The agent downloads itself from the GitHub release that matches your server
  version, so the host needs outbound HTTPS to `github.com`.
- To let admins start/stop/restart processes from the dashboard, set
  `agent: commands: true` in `/etc/daroyan/agent.yml` on the host.
- Everything else the agent watches (processes, ports, log files, disks) is set
  in `/etc/daroyan/agent.yml` and `/etc/daroyan/conf.d/*.yml`. See the
  [agent config reference](./crates/daroyan-agent/README.md#config) and
  [`agent.example.yml`](./crates/daroyan-agent/agent.example.yml). Restart the
  agent after a change: `sudo systemctl restart daroyan`.

---

## 8. Data and backups

Everything lives under your data volume/dir:

- `daroyan.db` — SQLite: users, servers, checks, incidents, settings.
- `daroyan.duckdb` — DuckDB: metrics, logs, traces.
- `backups/` — periodic DuckDB backups when `DUCKDB_BACKUP_DIR` is set.

Back up the whole data directory. For the Docker setup, back up the mounted
`./data` volume.

---

## 9. Pre-production checklist

- [ ] **Confirm the server starts** — watch the logs for `listening on ...`
      after first boot. (First start opens DuckDB; on a fresh host it may fetch a
      DuckDB extension, so allow outbound HTTPS the first time.)
- [ ] TLS terminates at the proxy; the app is **not** exposed directly
      (`HOST=127.0.0.1` with the proxy on the same host, or bound only to the
      proxy's network).
- [ ] `CLIENT_IP_HEADER` matches what your proxy sets, and `TRUSTED_PROXIES`
      lists only the proxy — never a shared network.
- [ ] `ALLOW_PRIVATE_TARGETS` is unset/`false` unless you intentionally monitor
      private hosts.
- [ ] Google redirect URI exactly equals `${APP_URL}/v1/auth/google/callback`.
- [ ] The data directory is on persistent storage and included in backups.
