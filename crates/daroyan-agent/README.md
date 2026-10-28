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

The script creates the `daroyan` user, writes `/etc/daroyan/agent.yml` once (a rerun keeps it), and swaps the binary in with a rename, so a rerun upgrades a running agent safely. It installs a systemd unit, or an OpenRC service on Alpine. For process restarts it adds a polkit rule, or a sudoers entry for `systemctl start/stop/restart` when polkit is not installed. With Docker installed, it also allows `docker start/stop/restart/ps` through sudo.

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
| `docker: <container>` | Every process in the container's cgroup. The id comes from `docker ps`. `*` matches any text, for example `app-*` for blue-green containers | Start, stop, restart with `docker` |
| `process: <pattern>` | `name` exact, `name*` prefix, `*name` suffix (process name or executable), `*text*` anywhere in the command line. Quote patterns that start with `*` | Watch only |

| Key | Default | Use |
| --- | --- | --- |
| `api.url` | `https://daroyan.com` | The API |
| `api.token` | `DAROYAN_TOKEN` | The install token. The agent uses it only once, to enroll |
| `server.name` | hostname | The name in the dashboard when the server enrolls. Later changes are ignored; rename the server in the dashboard |
| `server.labels` | `[]` | Labels in the dashboard |
| `processes[].name` | the source value | The label in the dashboard. Names must be unique |
| `processes[].user` | any | Count only processes owned by this user |
| `processes[].restart.attempts` | no restarts | Restart a stopped `systemd` or `docker` entry up to this many times. An error on `process` entries |
| `processes[].logs` | `false` | Send this process's logs. `docker` reads `docker logs`, `systemd` reads the journal. An error on `process` entries |
| `processes[].restart.within` | `10m` | The window for `attempts`, as `30s`, `10m` or `1h`. Once spent, the agent sends a `process_restart_gave_up` event and stops trying until the process runs again |
| `files[].name`, `files[].path` | none | A log file to send. The agent follows it and handles rotation. The `daroyan` user must be able to read it |
| `ports[].name` | none | The label in the dashboard |
| `ports[].host` | `localhost` | The host to connect to |
| `ports[].port` | required | Checked with a TCP connect, 3 second timeout |
| `host.disks` | `[]` | Report only these mount points. Empty skips overlay, tmpfs, devtmpfs, squashfs, nsfs and autofs |
| `host.networks` | `[]` | Report only these interfaces. Empty skips `lo*`, `docker*`, `veth*`, `br-*`, `virbr*`, `cni*`, `flannel*`, `cali*` |
| `agent.commands` | `false` | `true` lets admins start, stop and restart `systemd` and `docker` entries from the dashboard. Auto-restart (`restart`) works either way |
| `agent.log_level`, `agent.log_path` | `info`, `/var/log/daroyan` | Rotated daily or at 20 MB, 7 files kept, `daroyan.log` points to the current file |
| `agent.log_budget` | `1MB` | The most log data sent per minute, as `512KB` or `2MB`. Lines over it are dropped and counted |
| `agent.state_path` | `/var/lib/daroyan` | Holds `server.json`, the server id and token, mode 600 |

## Logs

Mark a `docker` or `systemd` process with `logs: true`, or list a file under `files`. Every 2 seconds the agent reads new lines, finds each line's level, and sends them over its connection.

- Docker: `docker logs --timestamps`, for every container that matches the name, so both colors of a blue-green deploy are read. A container that appears later is read from its first line.
- systemd: `journalctl --unit --output json`, from a saved cursor. The level comes from the journal priority.
- Files: read from a saved offset. A new inode or a shorter file means rotation, and reading starts again at 0.
- Level: a JSON line uses its `level`, `severity`, `lvl` or `levelname` field, and its other fields become attributes. Otherwise a level word near the start of the line. Otherwise stderr is a warning and anything else is info.
- A new source starts at its newest line. Read positions live in `log_positions.json` in `agent.state_path`, so a restart neither loses nor repeats lines.
- While the API cannot be reached, the agent keeps up to 10 MB of lines and drops the oldest past that.

`install.sh` adds the `daroyan` user to `systemd-journal` and `adm`, and allows `docker logs` through sudo.

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
