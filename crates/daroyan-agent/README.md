# Daroyan Agent

The `daroyan` binary runs on each server. It keeps one WebSocket open to the API. Every 10 seconds it sends a report: the host info when it changes, the state (CPU, load, memory, disks, NICs, connections), the watched processes and ports, and events. The API sends commands down the same socket.

## Run

```sh
cargo run -p daroyan-agent -- start --config agent.yml
cargo build --release -p daroyan-agent
```

`start` runs in the foreground. On a server, systemd runs it with `daroyan.service`, and `daroyan stop`, `daroyan restart` and `daroyan status` call `systemctl`.

## Install

`install.sh` installs the agent as a service. Run it as root with an install token from the Add Server page, or a long-lived key from Settings → Agents:

```sh
curl -fsSL https://daroyan.com/install.sh | sudo DAROYAN_TOKEN=dy_agent_... sh
```

| Variable | Default | Use |
| --- | --- | --- |
| `DAROYAN_TOKEN` | required | The install token |
| `DAROYAN_API_URL` | `https://daroyan.com` | The API |
| `DAROYAN_BINARY` | download | A local binary to install instead of the download |
| `DAROYAN_COMMANDS` | `0` | `1` lets the agent start, stop and restart units and containers, for `restart` and `agent.commands` |
| `DAROYAN_PROCESS_IO` | `0` | `1` installs the `daroyan-io` helper, which reads other users' `/proc/<pid>/io` for per-process disk I/O |

The script creates the `daroyan` user, writes `/etc/daroyan/agent.yml` once (a rerun keeps it), and swaps the binary in with a rename, so a rerun upgrades a running agent safely. It installs a systemd unit, or an OpenRC service on Alpine. With Docker installed, it allows the read-only `docker ps/inspect/logs/events` through sudo.

The agent never holds capabilities, and under systemd runs with `NoNewPrivileges` when it needs no sudo. The two flags add more:

- `DAROYAN_COMMANDS=1` lets the agent start, stop and restart the `systemd` and `docker` entries in `processes`, and nothing else. Before each start, `daroyan grants` runs as root. It writes a polkit rule that names those units, or a sudoers entry if polkit is not installed. It also writes a sudoers entry for `docker start/stop/restart` on those containers. An entry with only `restart` gets only `restart`. If you change `processes`, run `daroyan restart` to update the rules.

  The sudoers entry names each container. For a `*` pattern, `compose` or `labels`, the agent lists the matching containers when it starts. A container that is created later is added on the next `daroyan restart`. Kamal puts a version in each new container name, so run `daroyan restart` after each Kamal deploy. If you do not, the agent cannot restart the new container.
- `DAROYAN_PROCESS_IO=1` adds a separate `daroyan-io` service that holds `CAP_SYS_PTRACE` and `CAP_DAC_READ_SEARCH`, the two capabilities `/proc/<pid>/io` needs. It runs as its own `daroyan-io` user with no network, reads nothing from its socket, and answers only `daroyan` and root. Each connection gets one line per process, `<pid> <read_bytes> <write_bytes>`, on `/run/daroyan-io/io.sock`. It blocks `ptrace`, `execve`, new sockets and `open_by_handle_at` with seccomp, and under systemd it also sees only `/proc`, `/etc/passwd` and its own binary. Without it, the agent reports disk I/O only for its own processes.

A rerun without a flag keeps what the server already has. Set a flag to `0` to remove it.

Build the Linux binary on a Mac with `cargo zigbuild --release -p daroyan-agent --target aarch64-unknown-linux-musl` (or `x86_64-unknown-linux-musl`). The musl build is static and runs on Ubuntu, Debian and Alpine.

## Config

`/etc/daroyan/agent.yml`. See `agent.example.yml`. Host metrics need no config. Unknown keys fail at start, so a typo stops the agent with a message.

The agent also reads every `*.yml` file in `/etc/daroyan/conf.d/`, in name order, and adds its `processes`, `ports` and `files` to the main config. Each app can keep its own file there, for example `conf.d/linkity.yml`. A snippet takes only `processes`, `ports` and `files`, and process names must be unique across all files. Restart the agent after you change a file.

```yaml
processes:
  - name: Inboxing Worker        # the label in the dashboard
    systemd: inboxing-worker     # watch and control this unit
    restart:
      attempts: 3                # at most 3 restarts
      within: 10m                # in any 10 minutes
  - docker: redis-cache          # watch and control this container
  - name: Shop Web
    docker:
      compose: shop/web          # every container of this Compose service
    logs: true
  - process: "*celery worker*"   # watch only, by command line
    user: app
ports:
  - name: Redis
    host: redis-1
    port: 6379
```

Each process entry has exactly one source. The source decides how the agent finds it and whether the dashboard can control it:

| Source | Finds | Control |
| --- | --- | --- |
| `systemd: <unit>` | Every process in the unit's cgroup. `.service` is added when the name has no suffix | Start, stop, restart with `systemctl` |
| `docker: <container>` | Every process in each running container that matches. See Docker containers | Start, stop, restart with `docker` |
| `process: <pattern>` | `name` exact, `name*` prefix, `*name` suffix (process name or executable), `*text*` anywhere in the command line. Quote patterns that start with `*` | Watch only |

| Key | Default | Use |
| --- | --- | --- |
| `api.url` | `https://daroyan.com` | The API |
| `api.token` | `DAROYAN_TOKEN` | The install token. The agent uses it only once, to enroll |
| `server.name` | hostname | The name in the dashboard when the server enrolls. Later changes are ignored; rename the server in the dashboard |
| `server.labels` | `[]` | Labels in the dashboard |
| `processes[].name` | the source value | The label in the dashboard. Names must be unique. Required for `compose` and `labels` |
| `processes[].user` | any | Count only processes owned by this user |
| `processes[].restart.attempts` | no restarts | Restart a stopped `systemd` or `docker` entry up to this many times. An error on `process` entries. Needs `DAROYAN_COMMANDS=1` at install |
| `processes[].logs` | `false` | Send this process's logs. `docker` reads `docker logs`, `systemd` reads the journal. An error on `process` entries. `logs: { format: nginx }` sets the format; see Logs |
| `processes[].min_running` | none | The number of containers that must run for a `docker` entry. If fewer run, the entry is degraded. An error on other entries. See Docker containers |
| `processes[].restart.within` | `10m` | The window for `attempts`, as `30s`, `10m` or `1h`. Once spent, the agent sends a `process_restart_gave_up` event and stops trying until the process runs again |
| `files[].name`, `files[].path` | none | A log file to send. The agent follows it and handles rotation. The `daroyan` user must be able to read it; see Logs |
| `files[].format` | `auto` | The file's log format; see Logs |
| `ports[].name` | none | The label in the dashboard |
| `ports[].host` | `localhost` | The host to connect to |
| `ports[].port` | required | Checked with a TCP connect, 3 second timeout |
| `host.disks` | `[]` | Report only these mount points. Empty skips overlay, tmpfs, devtmpfs, squashfs, nsfs and autofs |
| `host.networks` | `[]` | Report only these interfaces. Empty skips `lo*`, `docker*`, `veth*`, `br-*`, `virbr*`, `cni*`, `flannel*`, `cali*` |
| `agent.commands` | `false` | `true` lets admins start, stop and restart `systemd` and `docker` entries from the dashboard. Auto-restart (`restart`) works either way. Both need `DAROYAN_COMMANDS=1` at install |
| `agent.log_level`, `agent.log_path` | `info`, `/var/log/daroyan` | Rotated daily or at 20 MB, 7 files kept, `daroyan.log` points to the current file |
| `agent.log_budget` | `1MB` | The most log data sent per minute, as `512KB` or `2MB`. Lines over it are dropped and counted |
| `agent.state_path` | `/var/lib/daroyan` | Holds `server.json`, the server id and token, mode 600 |

### Docker containers

You can pick containers in four ways:

| Write | Matches | Use it for |
| --- | --- | --- |
| `docker: redis-cache` | The container with this exact name | One container with a fixed name |
| `docker: app-*` | Each container with a matching name. `*` matches any text | Names that change, when no other container has a similar name |
| `docker: { compose: shop/web }` | Each container of the Compose service `web` in the project `shop` | Apps that you run with Docker Compose |
| `docker: { labels: { service: shop } }` | Each container that has all of these labels | Kamal, blue-green deploys, `docker run --label` |

A name pattern can match more than you want. For example, `web*` also matches `web-site`. Use `compose` or `labels` to prevent this.

A label is a key and a value on a container. Docker Compose puts the project and the service labels on each container that it starts, so `compose` needs no change to your app. Kamal puts `service`, `role` and `destination` on each container. To see the labels of a container, run:

```sh
docker inspect --format '{{json .Config.Labels}}' <container>
```

An entry with `compose` or `labels` must have a `name`. You can use `compose` and `labels` together. Then a container must match both.

```yaml
processes:
  - name: Shop Web
    docker:
      compose: shop/web
  - name: Shop
    docker:
      labels:
        service: shop
        role: web
  - name: Shop Blue
    docker:
      compose: shop/web
      labels:
        color: blue
```

#### When more than one container matches

The agent uses only the containers that run. A blue-green deploy, `docker-rollout` and Kamal keep the old container on the server, stopped, next to the new one. These stopped containers do not make the entry show as stopped. The entry is stopped only when no matching container runs.

- The CPU, memory and disk numbers are the totals of all running containers.
- The dashboard shows the newest running container.
- Stop and restart act on each running container.
- If no container runs, start and restart act on the container that started last. In a blue-green deploy, this is the color that was live.

#### Set how many containers must run

The labels on a container do not tell how many containers its service needs. If a service runs more than one replica, set `min_running` on the entry:

```yaml
processes:
  - name: Shop Worker
    docker:
      compose: shop/worker
    min_running: 3
    restart:
      attempts: 3
```

- If 3 or more containers run, the entry is running. More is not a problem, because a deploy can start new containers before it stops the old ones.
- If 1 or 2 containers run, the entry is degraded. The dashboard shows "1 of 3" and the server shows a warning. Use the "Process degraded" alert rule to get a message.
- If no container runs, the entry is stopped, as before.
- If no container runs, start and restart act on the 3 containers that started last.
- If the entry has `restart` and stays degraded for 30 seconds, the agent restarts stopped containers that use the same image as the running ones, up to `min_running`. It does not touch the running ones, and it does not start an old blue-green color. Each try counts as one of the `attempts`. If no such container exists, the agent does nothing and does not use an attempt.

Without `min_running`, one running container is enough. For a blue-green deploy, set `min_running` to the number of containers in one color, not in both.

## Logs

Mark a `docker` or `systemd` process with `logs: true`, or list a file under `files`. Every 2 seconds the agent reads new lines, finds each line's level, and sends them over its connection.

- Docker: `docker logs --timestamps` for each matching container, so the agent reads both colors of a blue-green deploy. A container that appears later is read from its first line.
- systemd: `journalctl --unit --output json`, from a saved cursor. The level comes from the journal priority.
- Files: read from a saved offset. A new inode or a shorter file means rotation, and reading starts again at 0.
- Level: a JSON line uses its `level`, `severity`, `lvl` or `levelname` field, and its other fields become attributes. Otherwise a level word near the start of the line. Otherwise stderr is a warning and anything else is info.
- Multi-line entries are joined into one line: indented lines, Java `Caused by:` and `... N more`, PHP and Laravel stack frames (`#0 …`, `[stacktrace]`), Python tracebacks with their final exception line, and MySQL slow query blocks up to the query's `;`.
- The API reads the rest: nginx, Apache, Caddy, HAProxy, Envoy and Traefik requests; Laravel, PHP, PHP-FPM, Rails, Django, Flask, uvicorn, Spring Boot, Logback and log4j app logs; MySQL, PostgreSQL, MongoDB, Redis, Kafka, RabbitMQ and Elasticsearch; sshd, sudo, cron, UFW, fail2ban and kernel OOM kills; and OpenTelemetry, Elastic ECS, structlog and Docker JSON.
- Format: by default (`auto`) every reader is tried on each line. Set `format` to read a source with only one family: `plain`, `json`, `logfmt`, `nginx`, `apache`, `caddy`, `haproxy`, `envoy`, `traefik`, `laravel`, `php`, `php-fpm`, `rails`, `python`, `java`, `mysql`, `postgresql`, `mongodb`, `redis`, `kafka`, `rabbitmq`, `elasticsearch` or `system`. `plain` keeps lines as they are. Lines get a `log.format` attribute, and a line the format cannot read is kept with `log.parse_failed: true`. An unknown name logs a warning and falls back to `auto`.
- A new source starts at its newest line. Read positions live in `log_positions.json` in `agent.state_path`, so a restart neither loses nor repeats lines.
- While the API cannot be reached, the agent keeps up to 10 MB of lines and drops the oldest past that.

`install.sh` adds the `daroyan` user to `systemd-journal` and `adm`, and allows `docker logs` through sudo. For a file neither group can read, give the user read access with an ACL, and a default ACL on the folder so rotated files keep it:

```sh
sudo setfacl -m u:daroyan:rx /var/www/app/storage/logs
sudo setfacl -m u:daroyan:r /var/www/app/storage/logs/laravel.log
sudo setfacl -d -m u:daroyan:r /var/www/app/storage/logs
```

## Enrollment

On the first start the agent sends its host info to `POST /v1/agent/enroll` with the install token. It saves the server token that comes back. Delete `server.json` to enroll again; that needs an install token with a use left.

## Connection

The agent opens `GET /v1/agent/connect` as a WebSocket with `Authorization: Bearer <server token>`. Every frame is JSON text with a `type`:

| Direction | `type` | Body |
| --- | --- | --- |
| agent to API | `report` | `host`, `state`, `processes`, `ports`, `events` |
| agent to API | `command_result` | `id`, `ok`, `error` |
| agent to API | `logs` | `lines`, `dropped` |
| API to agent | `command` | `id`, `action` (`start_process`, `stop_process`, `restart_process`), `name` |

- A command works only on a `systemd` or `docker` entry in `processes`. A process stopped from the dashboard is held: the agent does not restart it until a `start_process` or `restart_process`.
- The API must send a ping at least every 60 seconds. With no frame for 60 seconds, the agent reconnects.
- The agent reconnects after 1 second, then doubles the wait up to 60 seconds. The first report on a new connection always carries `host`.

## Limits

- The agent keeps up to 60 reports (10 minutes) in memory while it is disconnected, then drops the oldest.
- Process control (auto-restart and dashboard commands) works for `systemd` and `docker` entries. It runs `systemctl` or `docker`, and falls back to `sudo -n` when that is denied. On OpenRC (Alpine) use `process` entries; they are watch-only.
- Per-process disk I/O reads `/proc/<pid>/io`, which needs `CAP_SYS_PTRACE` and `CAP_DAC_READ_SEARCH`. The systemd unit grants them; on OpenRC the script sets them on the binary with `setcap`.
- A `process` entry includes its child processes in CPU, memory and disk I/O, but its count is only the matching processes. A child that starts and ends between two reports (every 10 seconds) is not counted.
- Disks are listed once per device. Virtual, host-share (`virtiofs`, `9p`, `fuse.*`) and network (`nfs`, `cifs`) filesystems are skipped unless `host.disks` names them, because a hung network mount would block the agent.
- ARM Linux reports no CPU model, so the dashboard shows only the core count there.
- Port checks are TCP only. Redis and HTTP protocol checks come later.
- TCP and UDP counts read `/proc/net`, so they are 0 on macOS.
