# Daroyan API

One binary with axum, SeaORM on SQLite, and DuckDB. Agents keep a WebSocket open to it. It needs no other service.

## Run

```sh
cp .env.example .env                       # set the Google sign-in values
cargo run -p daroyan-api
```

At start the API runs the SQLite and DuckDB migrations, then listens on `PORT` (default 8900). `data/` holds `daroyan.db` (SQLite, WAL mode) and `daroyan.duckdb`.

The release binary embeds the dashboard from `apps/web/build/client` and `install.sh`, so build the web app first. Any path outside `/v1` gets the dashboard. `/install.sh` downloads the agent from the GitHub release matching the API's version.

## Sign In

Users sign in with Google. Create an OAuth client in Google Cloud with the redirect URI `APP_URL/v1/auth/google/callback`, then set `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` and `APP_URL`. `ALLOWED_EMAILS` lists the owners, as emails or `@domain`. They are always admins and cannot be removed from the app. A session lasts 30 days.

Admins invite more people in Settings › Members (`/v1/members`) as an admin or a member. No email is sent: an invited person signs in with the Google account for that email. Members see everything and can handle incidents, mutes and check runs. Adding, changing or deleting servers, checks, crons, alert rules, status pages, integrations, backups, telemetry tokens and members needs an admin, which the `AdminUser` extractor enforces on those routes, so MCP tokens follow the same rules. Access is checked on every request, so removing someone signs them out and stops their MCP tokens right away.

## Release

Push a `v*` tag. The `Release` workflow builds the agent for x86_64 and aarch64 and the API for x86_64, adds the binaries to a GitHub release, and pushes `ghcr.io/arikchakma/daroyan/api` with the version and `latest` tags. Deploy it with the `daroyan_api` role in `infra-ansible`.

The image only copies files that the workflow built. It needs the Linux API binary in `dist/api/`.

## Run the Binary

Every release has `daroyan-api-x86_64-linux-gnu`, built on Ubuntu 22.04. It runs on Linux with glibc 2.35 or later (Ubuntu 22.04, Debian 12 and newer) and needs nothing else.

```sh
mkdir -p /opt/daroyan && cd /opt/daroyan
gh release download vX.Y.Z --repo arikchakma/daroyan --pattern 'daroyan-api-*'
mv daroyan-api-* daroyan-api && chmod +x daroyan-api
```

Set `APP_URL`, the Google sign-in values and `ALLOWED_EMAILS` (see Sign In). The databases go to `data/` in the working folder unless `DATABASE_URL` and `DUCKDB_PATH` say otherwise. Run it with systemd:

```ini
[Unit]
Description=Daroyan
After=network-online.target

[Service]
WorkingDirectory=/opt/daroyan
EnvironmentFile=/opt/daroyan/.env
ExecStart=/opt/daroyan/daroyan-api
Restart=always

[Install]
WantedBy=multi-user.target
```

Put a reverse proxy with TLS in front (see Client IP Behind Cloudflare and Caddy). The binary does not back up its data on its own. Set it up as described in Backups: for Litestream, `ExecStart=litestream replicate -exec /opt/daroyan/daroyan-api` runs both from one unit.

## Crates

| Crate | Holds |
| --- | --- |
| `daroyan-core` | `db/` (SeaORM connection, generated `entities/`, `ids`, `queries/`), `analytics/` (DuckDB: one file per table), `servers/`, `alerts/` (rules, evaluation, integrations, the engine), `commands.rs`, `agents.rs` (the live sockets), config, errors |
| `daroyan-api` | `app.rs`, `layers/`, `extract/`, `routes/` (one file per resource), `views/` |
| `daroyan-migration` | SQLite migrations on SeaORM's schema builder, and DuckDB SQL in `duckdb/NNNN_name.sql` |

SQLite holds the rows people see: servers, install tokens, commands, each server's last report, integrations, alert rules and alerts. DuckDB holds every state sample and event. Reports are buffered and written every 5 seconds with the Appender. Rows older than `RETENTION_DAYS` are deleted every hour. Log lines are agent messages of type `logs`, stored in `log_lines` and deleted after `LOG_RETENTION_DAYS` (7).

## Routes

`/openapi.json` serves the full spec, and `packages/client` is the typed client generated from it. Dashboard routes take the `daroyan_session` cookie that Google sign-in sets, or an MCP token (see MCP). Routes that change servers, checks, crons, alert rules, status pages, integrations, backups, tokens or members need an admin (see Sign In). The agent enrolls at `POST /v1/agent/enroll` with an install token and then keeps `GET /v1/agent/connect` open with its server token; the API pings it every 20 seconds.

OpenTelemetry SDKs send traces to `POST /v1/otlp/v1/traces` and logs to `POST /v1/otlp/v1/logs` with an ingest token, over HTTP as protobuf or JSON, gzip allowed, up to 8 MB a request. Set `OTEL_EXPORTER_OTLP_ENDPOINT` to `<app url>/v1/otlp` and the SDK picks both paths. When the `host.name` resource attribute matches an enrolled server's hostname, with or without its domain, the span or log line belongs to that server. Log lines that match no server are stored with the server id `otlp` and show as "OpenTelemetry" on the Logs page.

A server is `pending` until its first report and `down` while its socket is closed. It is `warning` when CPU, memory or a disk is at 90% or more, a watched process is not running, or a watched port is closed. Otherwise it is `healthy`.

## MCP

`POST /mcp` is an MCP server (streamable HTTP, JSON responses, no sessions) for AI tools like Claude Code and Cursor. It takes an MCP token as `Authorization: Bearer dy_mcp_…`. Each person creates their own in Settings › MCP (`/v1/mcp-tokens`), and a token acts as that user. The session cookie is not accepted, so a web page cannot call it from someone's browser. An MCP token also works on the `/v1` routes that take a session, except `/v1/mcp-tokens` and the member changes under `/v1/members`, which need a signed-in session so a leaked token cannot mint more tokens or grant access.

`src/mcp.rs` lists the tools. Each one names a route, and its input schema comes from that route's OpenAPI parameters and body, so the tools change with the API. A call runs the route in-process with the same token, so validation, errors and responses match the REST API. Responses over 100,000 characters are cut.

Since models rarely know the current time, `since`, `until` and `at` also take a duration back from now (`1h`, `7d`) or an ISO 8601 time, and results show every `at`/`…_at` timestamp, in ms or µs, as ISO 8601. Results with a `ping_key` get a `ping_url`, and `create_install_command` returns the full command. `get_overview` is the one tool without a route: it reads the same endpoints as the overview dashboard and returns what needs attention, with the same muting and dedupe rules. To add a tool, add a line to `list_tool_defs` and bump the count in `tests/mcp_flow.rs`.

## Alerts

Every 15 seconds the engine in `daroyan-core/src/alerts/engine.rs` checks each enabled rule against each server in its scope, using the last report and whether the socket is open:

| Condition | Fires for |
| --- | --- |
| `offline` | A server whose socket is closed |
| `cpu`, `memory`, `load` | The server, when the value is above `above` |
| `disk` | Each mount above `above` |
| `process_stopped`, `port_closed` | Each watched process or port (or the one named) that is down |

A breach must last `for_secs` before the alert fires. Then the rule's integrations get a message, and they get another when the alert resolves. While a server is offline, only `offline` rules are checked. Disabling a rule, or taking a server out of its scope, resolves its alerts without a message. Messages link to `APP_URL/servers/{id}` when `APP_URL` is set.

| Kind | Sends |
| --- | --- |
| `slack` | `chat.postMessage` with the bot token, to the picked channel |
| `discord` | `{ content }` to a channel webhook |
| `telegram` | `sendMessage` with the bot token to `chat_id` |
| `webhook` | `{ event, title, body, url, data }`, where `event` is `alert.firing`, `alert.resolved` or `test` |

Requests time out after 10 seconds and do not follow redirects. Each integration keeps its last delivery status and error.

### One-Click Setup

Slack, Discord and Telegram connect only by signing in, so nobody copies a webhook URL or token. Without these variables, that integration shows as Not Set Up in the dashboard. Webhooks still take a URL. `APP_URL` must be set, because the callbacks go to `APP_URL/v1/integrations/{kind}/callback`.

| Kind | Env | How it connects |
| --- | --- | --- |
| Slack | `SLACK_CLIENT_ID`, `SLACK_CLIENT_SECRET` | OAuth with `chat:write`, `chat:write.public` and `channels:read`. Then the popup lists the public channels and you pick one. Alerts go out with `chat.postMessage` |
| Discord | `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET` | OAuth with the `webhook.incoming` scope. Discord asks for the server and channel |
| Telegram | `TELEGRAM_BOT_TOKEN` | `t.me/<bot>?startgroup=<code>`. The API reads the bot's updates until the code arrives, then saves that chat |

Add the callback URL to the Slack app's Redirect URLs and the Discord app's OAuth2 Redirects. Connect codes live in memory for 10 minutes.

To try Slack without a real workspace, run the [emulate](https://github.com/vercel-labs/emulate) Slack emulator and point the API at it with `SLACK_API_URL`:

```sh
npx emulate --service slack --port 4003 --seed slack.seed.yaml   # an oauth_apps entry with the callback URL
SLACK_API_URL=http://localhost:4003 SLACK_CLIENT_ID=… SLACK_CLIENT_SECRET=… cargo run -p daroyan-api
```

### Backtest

The New Rule page shows how often a rule would have fired in the last 7 days. CPU, memory, load and disk use the stored samples (disk uses the total of all mounts). Offline counts gaps in the samples. Process stopped counts `process_exited` events. Port closed cannot be checked, because port results are not stored.

## Rate Limits

All limits live in memory in `daroyan-core/src/limits.rs`. That is safe because only one API process runs. With more than one, move them to Redis in that file.

| Limit | Key | Over the limit |
| --- | --- | --- |
| 50 requests/s, burst 100 | client IP | `429 rate_limit_exceeded` |
| 10 failed sign-ins (admin, install or server token) in 60 s | client IP | `429` for every auth attempt until the window ends, even with a valid token |
| `POST /v1/agent/enroll`, 10/min | client IP | `429` |
| `GET /v1/agent/connect`, 1 every 10 s, burst 10 | server id | `429`, and the agent backs off |
| Agent messages, 2/s, burst 120 | the socket | The API closes the socket. The burst lets an agent flush 10 minutes of queued reports |
| OTLP log data, `OTLP_LOG_BUDGET_KB` (1 MB) a minute | ingest token | Records over it are dropped. The response carries `partial_success.rejected_log_records`, and Settings → Telemetry shows the total |

### Client IP Behind Cloudflare and Caddy

The API reads the client IP from `CLIENT_IP_HEADER` (`cf-connecting-ip`) only when the TCP peer is in `TRUSTED_PROXIES` (`127.0.0.1,::1`, Caddy on the same host). From any other peer, it uses the peer address, so a forged header does nothing. `TRUSTED_PROXIES` takes IPs and CIDR ranges, for example `172.16.0.0/12` when Caddy runs in Docker.

`CLIENT_IP_HEADER` must be a header your proxy overwrites on every request. When you are not behind Cloudflare, set it to `x-forwarded-for` or `x-real-ip`. Otherwise clients can spoof their IP.

```caddyfile
daroyan.com {
    reverse_proxy 127.0.0.1:8900
}
```

- Set `HOST=127.0.0.1`, so that only Caddy can reach the API.
- Let only Cloudflare reach Caddy: allow ports 80 and 443 only from [Cloudflare's IP ranges](https://www.cloudflare.com/ips/), or use Cloudflare Tunnel or Authenticated Origin Pulls. If anyone can reach Caddy directly, they can set `CF-Connecting-IP` to any value.
- Caddy passes the header and WebSockets through unchanged. Cloudflare closes an idle WebSocket after 100 seconds, and the API pings every 20 seconds, which keeps it open.

## Migrations

```sh
vp run db:migrate                          # SQLite, also run at start
vp run db:duckdb                           # DuckDB, also run at start
vp run db:entities                         # regenerate the SeaORM entities after a SQLite migration
```

Never edit a generated entity or an applied DuckDB file. Add a new migration. Stop the API before you run the migration binary on DuckDB, because only one process can open the file for writing.

## Backups

SQLite: run [Litestream](https://litestream.io) next to the API. It streams every change, so a restore loses at most a few seconds.

DuckDB: set `DUCKDB_BACKUP_DIR`. Every 6 hours the API copies the database into `daroyan-<time>.duckdb` in that folder with `COPY FROM DATABASE`, while it holds the only connection, so the copy is consistent. It writes `<name>.partial` first and renames it when done, so an uploader can move every `daroyan-*.duckdb` file safely. The `daroyan_api` role in `infra-ansible` runs both and uploads to R2.

## Limits

- One API process. The live sockets are a map in memory, and DuckDB allows one writer.
- Integration secrets (webhook URLs, bot tokens) are stored as plain text in SQLite. Protect the file.
- How long a breach has lasted is kept in memory, so an API restart starts each `for_secs` over.
- The latest events can take up to 5 seconds to appear, because writes are buffered.
