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
| `DAROYAN_BINARY` | download | A local binary to install instead of the download. `daroyan-docker-read` must be in the same folder |
| `DAROYAN_COMMANDS` | `0` | `1` lets the agent start, stop and restart units and containers, for `restart` and `agent.commands` |
| `DAROYAN_PROCESS_IO` | `0` | `1` installs the `daroyan-io` helper, which reads other users' `/proc/<pid>/io` for per-process disk I/O |

The script creates the `daroyan` user, writes `/etc/daroyan/agent.yml` once (a rerun keeps it), and swaps the binary in with a rename, so a rerun upgrades a running agent safely. It installs a systemd unit, or an OpenRC service on Alpine. With Docker installed, `daroyan grants` lets the `daroyan` user run one program through sudo: `/usr/local/bin/daroyan-docker-read`. The user gets no direct access to `docker`.

`daroyan-docker-read` is a small separate program that runs as root. It uses only the Rust standard library, and reads only what the agent needs from Docker:

- It reads the container list, the container state, the logs and the container events. It does not read environment variables or command lines, because these often contain passwords and keys.
- It accepts only the containers of the `docker` entries in `agent.yml`. `daroyan grants` writes these entries to `/etc/daroyan/docker-read.allow` on every start, so the helper does not read `agent.yml`. The `daroyan` user cannot read the logs or details of other containers.
- It rejects any argument that it does not expect, and runs `docker` with a fixed path and an empty environment.
- It refuses to run if a user other than root owns `docker-read.allow` or its folder, or if other users can write to them.
- sudo does not write a log line for each of its calls.

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
| `agent.log_budget` | `1MB` | The most log data sent per minute, as `512KB` or `2MB`. Each log source gets an equal share. Lines over a share are dropped and counted |
| `agent.access_logs` | `false` | Send logins, `sudo`, `su` and account changes from the journal as the `access` source. See Access logs |
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

Mark a `docker` or `systemd` process with `logs: true`, or list a file under `files`. The agent reads new lines, finds the level of each line, and sends the lines over its connection. It follows Docker logs and the journal as they are written. It reads files every 2 seconds.

- Docker: one `docker logs --follow --timestamps` process for each running container that matches, so the agent reads both colors of a blue-green deploy. A follower starts when a container starts and stops when the container stops or is removed. If a follower stops for a different reason, the agent starts it again from the last line it read. The wait between tries starts at 1 second and doubles up to 30 seconds. To start again, the agent reads the last 10,000 lines with `--tail` and skips the lines it already sent, so Docker does not read the whole log file. If more than 10,000 lines were written in between, the agent reads once from the saved time with `--since`. Every minute the agent asks Docker for the newest line of each container. If a follower sent nothing in that minute but Docker has a line the follower did not send, the agent starts the follower again. A container that appears later is read from its first line. A line that Docker splits into 16 KB parts is joined again. Lines longer than 64 KB are cut.
- systemd: one `journalctl --unit --follow --all --output json` process for each unit, from the saved cursor. When the agent starts, it first reads to the end of the journal without `--follow`, because `--follow` shows only the current boot. This way it also sends the lines that a unit wrote before a reboot. A new unit starts at the time the agent first sees it. If the process stops, the agent starts it again after the last line it read, with the same wait as Docker. If `journalctl` cannot use the saved cursor, the agent logs a warning and starts the unit again from the current time. The level comes from the journal priority.
- Files: read from a saved offset. A new inode or a shorter file means rotation, and reading starts again at 0.
- Level: a JSON line uses its `level`, `severity`, `lvl` or `levelname` field, and its other fields become attributes. Otherwise a level word near the start of the line. Otherwise stderr is a warning and anything else is info.
- Multi-line entries are joined into one line. The agent sends a held entry after 1 second without a new line from the same container or unit. Joined entries: indented lines, Java `Caused by:` and `... N more`, PHP and Laravel stack frames (`#0 …`, `[stacktrace]`), Python tracebacks with their final exception line, and MySQL slow query blocks up to the query's `;`.
- The API reads the rest: nginx, Apache, Caddy, HAProxy, Envoy and Traefik requests; Laravel, PHP, PHP-FPM, Rails, Django, Flask, uvicorn, Spring Boot, Logback and log4j app logs; MySQL, PostgreSQL, MongoDB, Redis, Kafka, RabbitMQ and Elasticsearch; sshd, sudo, cron, UFW, fail2ban and kernel OOM kills; and OpenTelemetry, Elastic ECS, structlog and Docker JSON.
- Format: by default (`auto`) every reader is tried on each line. Set `format` to read a source with only one family: `plain`, `json`, `logfmt`, `nginx`, `apache`, `caddy`, `haproxy`, `envoy`, `traefik`, `laravel`, `php`, `php-fpm`, `rails`, `python`, `java`, `mysql`, `postgresql`, `mongodb`, `redis`, `kafka`, `rabbitmq`, `elasticsearch` or `system`. `plain` keeps lines as they are. Lines get a `log.format` attribute, and a line the format cannot read is kept with `log.parse_failed: true`. An unknown name logs a warning and falls back to `auto`.
- A new source starts at its newest line. Read positions live in `log_positions.json` in `agent.state_path`. The agent saves them every 5 seconds and when it stops, so a restart does not lose or repeat lines. If the agent is killed, it can send the lines of the last 5 seconds again.
- Budget: each source (one `processes` or `files` entry, and the `access` source) gets an equal share of `agent.log_budget` every minute. A noisy container cannot use up the share of another service, for example the journal of `sshd`. The agent counts the lines over a share and sends that count. Successful SSH logins and the lines of `sudo`, `su`, `login` and the account programs do not count against the budget and are never dropped. Only the other `sshd` lines count, because anyone on the internet can make `sshd` write them. So an attacker cannot fill the share with failed logins to hide a later successful one.
- Memory: the agent holds at most 8 MB of lines that it read but did not process. If the connection is slow, the followers wait and do not read more. Docker and the journal keep the lines until the agent reads them.
- While the API cannot be reached, the agent keeps up to 10 MB of lines and drops the oldest past that.

`install.sh` adds the `daroyan` user to `systemd-journal` and `adm`. The agent reads Docker logs through `daroyan docker-read`. For a file neither group can read, give the user read access with an ACL, and a default ACL on the folder so rotated files keep it:

```sh
sudo setfacl -m u:daroyan:rx /var/www/app/storage/logs
sudo setfacl -m u:daroyan:r /var/www/app/storage/logs/laravel.log
sudo setfacl -d -m u:daroyan:r /var/www/app/storage/logs
```

## Access logs

The agent can send the logins and the use of root on the server as the `access` source. It does not do this by default. Set `agent.access_logs: true` to turn it on.

Do not turn it on before you read what it sends. The lines include the full command line of each `sudo` command, which can contain a password or a token, for example `sudo mysql -p…`. They also include the user name of each failed SSH login, and people sometimes type a password at the user name prompt. Every person who can sign in to Daroyan can read these lines. The login alert rules need access logs, and they send nothing for a server that does not send them.

The agent runs one `journalctl` process with a match for each of these programs: `sshd`, `sshd-session`, `sshd-auth`, `sudo`, `su`, `login`, `useradd`, `userdel`, `usermod`, `groupadd`, `groupdel`, `groupmod`, `gpasswd`, `passwd` and `chpasswd`. It follows the journal like a `systemd` entry, from a saved cursor. The source starts at the time the agent first runs with it, and it does not read older lines.

Any user can write a journal line that says it comes from `sshd`, for example with `logger -t sshd`. So the agent does not trust the identifier that the program gives. It uses the fields that journald itself adds:

- `_COMM` must be one of the programs above.
- The program must run as root: `_UID` is `0`. journald reads the user ID from the kernel, as the host sees it, so a program in a user namespace cannot fake it.
- `sudo`, `su`, `passwd` and `gpasswd` are setuid programs. They run with the user ID of the person who called them, so for these four the agent also accepts a line when `_CAP_EFFECTIVE` is not zero and `_EXE` is that program in `/usr/bin`, `/bin`, `/usr/sbin` or `/sbin`. A program in a user namespace has capabilities, but its `_EXE` is the real file, such as `/usr/bin/python3`, so a renamed program fails this check.

One gap stays open for these four programs. A user who can make user and mount namespaces can mount another file over `/usr/bin/sudo` in the namespace, and journald then shows `_EXE` as `/usr/bin/sudo`. That user can write a fake `sudo`, `su`, `passwd` or `gpasswd` line. They cannot write a fake `sshd` or `login` line, because those need `_UID` `0`, and the alert rules read only `sshd` lines. To close the gap, turn off namespaces for normal users with `kernel.apparmor_restrict_unprivileged_userns=1` on Ubuntu or `user.max_user_namespaces=0`.

The agent drops a line that fails either check. It also drops lines that repeat other lines or say nothing about a person, for example `Connection closed by <ip>` before a user name, `Received disconnect` and the PAM session lines of `sudo` and `su`.

The agent reads console logins from `login`. A success is the line `pam_unix(login:session): session opened for user …`, because both the shadow and the util-linux versions of `login` write it. A failure is `FAILED LOGIN (1) on '/dev/tty1' FOR 'root', …` from shadow, or `FAILED LOGIN 1 FROM tty1 FOR root, …` from util-linux. The agent does not read `/var/log/wtmp` or `/var/log/btmp`, because Debian 13 does not write them.

The API adds these attributes to each line it can read:

| Attribute | Value |
| --- | --- |
| `event.action` | `ssh_login`, `ssh_logout`, `console_login`, `console_logout`, `sudo`, `su`, `user_add`, `user_delete`, `user_change`, `group_add`, `group_delete` or `password_change` |
| `event.outcome` | `success` or `failure`. A failure is a warning |
| `user` | The person who logged in or ran the command |
| `client.ip`, `client.port` | The address of an SSH client |
| `ssh.method` | `publickey`, `password` or `keyboard-interactive/pam` |
| `ssh.key.type`, `ssh.key.fingerprint` | The key of a `publickey` login, for example `ED25519` and `SHA256:…` |
| `ssh.invalid_user` | `true` when the user name does not exist on the server |
| `login.tty`, `login.host` | The terminal of a console login, and the remote host when `login` gives one |
| `sudo.target_user`, `su.target_user` | The user that `sudo` or `su` switched to |
| `user.target.name`, `group.name` | The account or group that was added, changed or deleted |

An SSH user name, and the key ID of an SSH certificate, can contain the words `from` and `port`. The API reads only an IP address that comes before `port <number>` and then the end of the line or `ssh2`. For a successful login, it takes the first one, because the user name is a real account and certificate text comes after `ssh2`. For a failure, it takes the address only when the line has exactly one, and keeps the failure without an address otherwise.

The API keeps the last login from each address on each server in the `access_sources` table. It does not delete these rows with old logs, so it knows an address for longer than `LOG_RETENTION_DAYS`. Each successful SSH login gets `ssh.ip_last_seen`: the time of the login before it from the same address. A first login from an address has no `ssh.ip_last_seen`.

The server page shows the latest access events in the Access section. Failed SSH logins are not listed there, because a public server gets many of them. The section shows how many failed logins came in the last 24 hours.

Three alert rules use these lines:

- Login from a new address: an SSH login from an address that did not log in to the server in the last N days. The rule learns for 24 hours after the first login that Daroyan sees on a server, and sends nothing in that time.
- Root login: an SSH login as `root`.
- Login after failed attempts: an SSH login after N or more failed attempts from the same address in a time window. One wrong password writes more than one line, so the API groups the lines by `sshd` process and client port, and counts each password or key that `sshd` rejected once. A `Failed none` line is a probe that asks which methods the server allows, so it does not count.

Each rule makes one alert for each address. The alert closes when no new matching login comes from that address for 15 minutes. The rules read only `sshd` lines that the agent marked as access lines. The API stores these lines with the stream `access`. A line from a source that a user named `access`, for example on an older agent, keeps its own stream, so the API does not trust it.

Access logs need journald. On a server without it, the agent logs this at the start and sends no access logs. The agent does not read `/var/log/auth.log` for access logs, because any user can write a forged line there through syslog.

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
