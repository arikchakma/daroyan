use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hash;
use std::path::Path;
use std::time::Instant;

use daroyan_protocol::report::{
    Container, Event, EventKind, ProcessState, ProcessStatus,
};
use log::{info, warn};
use sysinfo::{Pid, Process, System, Uid, Users};

use crate::check::docker_events::{DockerEvents, read_exit_code, was_oom, was_stopped};
use crate::collect::{now_millis, per_sec};
use crate::command::{HeldProcesses, hold};
use crate::config::{ProcessCheck, Restart, Source};
use crate::service::{
    Controller, find_container_id, inspect_container, resolve_controller,
};

pub struct ProcessWatch {
    checks: Vec<ProcessCheck>,
    controllers: HashMap<String, Controller>,
    uids: HashMap<String, Option<Uid>>,
    container_ids: HashMap<String, String>,
    running: HashMap<String, bool>,
    main_pids: HashMap<String, Pid>,
    restarts: HashMap<String, Vec<Instant>>,
    gave_up: HashSet<String>,
    held: HeldProcesses,
    commands: bool,
    families: HashMap<String, HashSet<Pid>>,
    started: HashMap<String, u64>,
    docker: Option<DockerEvents>,
    containers: HashMap<String, Container>,
    last_containers: HashMap<String, Container>,
}

impl ProcessWatch {
    pub fn new(checks: Vec<ProcessCheck>, commands: bool, held: HeldProcesses) -> Self {
        if checks.iter().any(|check| matches!(check.source, Source::Systemd(_)))
            && !Path::new("/run/systemd/system").exists()
        {
            warn!(
                "this server does not run systemd, so systemd entries always show as stopped"
            );
        }
        let users = Users::new_with_refreshed_list();
        let uids = checks
            .iter()
            .filter_map(|check| {
                let user = check.user.as_ref()?;
                let uid = users.iter().find(|u| u.name() == user).map(|u| u.id().clone());
                if uid.is_none() {
                    warn!(
                        "{} watches processes of {user}, but that user does not exist",
                        check.name
                    );
                }
                Some((check.name.clone(), uid))
            })
            .collect();
        let controllers = checks
            .iter()
            .map(|check| (check.name.clone(), resolve_controller(check)))
            .collect();
        let docker = checks
            .iter()
            .any(|check| matches!(check.source, Source::Docker(_)))
            .then(DockerEvents::start);

        Self {
            checks,
            controllers,
            uids,
            container_ids: HashMap::new(),
            running: HashMap::new(),
            main_pids: HashMap::new(),
            restarts: HashMap::new(),
            gave_up: HashSet::new(),
            held,
            commands,
            families: HashMap::new(),
            started: HashMap::new(),
            docker,
            containers: HashMap::new(),
            last_containers: HashMap::new(),
        }
    }

    pub fn check_processes(
        &mut self,
        system: &System,
        elapsed_secs: f64,
        events: &mut Vec<Event>,
    ) -> Vec<ProcessState> {
        let needs_cgroups = self
            .checks
            .iter()
            .any(|check| !matches!(check.source, Source::Process(_)));
        let cgroups = if needs_cgroups { read_cgroups(system) } else { HashMap::new() };
        let checks = self.checks.clone();
        checks
            .iter()
            .map(|check| {
                self.check_process(check, system, &cgroups, elapsed_secs, events)
            })
            .collect()
    }

    fn check_process(
        &mut self,
        check: &ProcessCheck,
        system: &System,
        cgroups: &HashMap<Pid, String>,
        elapsed_secs: f64,
        events: &mut Vec<Event>,
    ) -> ProcessState {
        let matches = self.find_matches(check, system, cgroups);
        let family = collect_family(system, &matches);
        self.families.insert(
            check.name.clone(),
            family.iter().map(|process| process.pid()).collect(),
        );
        let main = matches
            .iter()
            .min_by_key(|process| (process.start_time(), process.pid()));
        let started_at = main.map(|process| process.start_time());
        let is_running = !matches.is_empty();
        let was_running =
            self.running.insert(check.name.clone(), is_running).unwrap_or(false);

        let controller =
            self.controllers.get(&check.name).cloned().unwrap_or(Controller::None);

        let container = if is_running { self.read_container(check) } else { None };
        if let Some(container) = &container {
            self.last_containers.insert(check.name.clone(), container.clone());
        }

        if was_running && !is_running {
            let mut exit = controller.read_exit();
            let actions = self
                .docker
                .as_ref()
                .zip(self.last_containers.get(&check.name))
                .map(|(docker, last)| docker.list_last_run(&last.id))
                .unwrap_or_default();

            exit.oom |= was_oom(&actions);
            exit.code = exit.code.or_else(|| read_exit_code(&actions));
            let stopped = was_stopped(&actions);

            let ran_secs = exit.ran_secs.or_else(|| {
                let started = self.started.get(&check.name)?;
                (now_millis() / 1000).checked_sub(*started)
            });

            warn!(
                "{} stopped with code {:?}, signal {:?}, out of memory {}",
                check.name, exit.code, exit.signal, exit.oom
            );

            events.push(new_event(EventKind::ProcessExited {
                name: check.name.clone(),
                code: exit.code,
                signal: exit.signal,
                oom: exit.oom,
                stopped,
                ran_secs,
                restarts: exit.restarts,
            }));
        }

        if let Some(started_at) = started_at {
            self.started.insert(check.name.clone(), started_at);
        }

        if is_running {
            self.gave_up.remove(&check.name);
        }

        let old_pid = match main {
            Some(process) => self.main_pids.insert(check.name.clone(), process.pid()),
            None => self.main_pids.remove(&check.name),
        };
        let restarted_here = if is_running {
            old_pid.is_some_and(|pid| Some(pid) != main.map(|process| process.pid()))
                && hold(&self.held).restarted.remove(&check.name)
        } else {
            hold(&self.held).restarted.remove(&check.name);
            false
        };
        if was_running
            && !restarted_here
            && let (Some(old_pid), Some(process)) = (old_pid, main)
            && old_pid != process.pid()
            && !matches.iter().any(|process| process.pid() == old_pid)
        {
            warn!("{} changed PID from {old_pid} to {}", check.name, process.pid());
            events.push(new_event(EventKind::ProcessChanged {
                name: check.name.clone(),
                old_pid: old_pid.as_u32(),
                new_pid: process.pid().as_u32(),
            }));
        }

        let is_held = hold(&self.held).stopped.contains(&check.name);
        if !is_running
            && !is_held
            && controller.is_controllable()
            && let Some(restart) = check.restart
        {
            self.restart_process(&check.name, &controller, restart, events);
        }

        let disk: (u64, u64) = family.iter().fold((0, 0), |(read, write), process| {
            let usage = process.disk_usage();
            (read + usage.read_bytes, write + usage.written_bytes)
        });

        ProcessState {
            name: check.name.clone(),
            status: if is_running {
                ProcessStatus::Running
            } else {
                ProcessStatus::Stopped
            },
            count: matches.len() as u32,
            started_at: started_at.map(|at| at * 1000),
            cpu: family.iter().map(|process| process.cpu_usage()).sum(),
            mem_used: family.iter().map(|process| process.memory()).sum(),
            read_speed: per_sec(disk.0, elapsed_secs),
            write_speed: per_sec(disk.1, elapsed_secs),
            controllable: self.commands && controller.is_controllable(),
            container,
        }
    }

    pub fn families(&self) -> &HashMap<String, HashSet<Pid>> {
        &self.families
    }

    fn find_matches<'a>(
        &mut self,
        check: &ProcessCheck,
        system: &'a System,
        cgroups: &HashMap<Pid, String>,
    ) -> Vec<&'a Process> {
        let Source::Docker(container) = &check.source else {
            return self.match_processes(check, system, cgroups, None);
        };

        let cached = self.container_ids.get(container).cloned();
        let id = self.find_container_id(container);
        let matches = self.match_processes(check, system, cgroups, id.as_deref());

        if !matches.is_empty() {
            return matches;
        }

        self.container_ids.remove(container);
        if cached.is_none() {
            return matches;
        }

        let fresh = self.find_container_id(container);
        if fresh.is_none() || fresh == cached {
            self.container_ids.remove(container);
            return matches;
        }

        let matches = self.match_processes(check, system, cgroups, fresh.as_deref());
        if matches.is_empty() {
            self.container_ids.remove(container);
        }

        matches
    }

    fn match_processes<'a>(
        &self,
        check: &ProcessCheck,
        system: &'a System,
        cgroups: &HashMap<Pid, String>,
        container_id: Option<&str>,
    ) -> Vec<&'a Process> {
        let uid = self.uids.get(&check.name);
        let in_cgroup = |process: &Process, is_part: &dyn Fn(&str) -> bool| {
            cgroups
                .get(&process.pid())
                .is_some_and(|cgroup| has_cgroup_part(cgroup, is_part))
        };
        system
            .processes()
            .values()
            .filter(|process| process.thread_kind().is_none())
            .filter(|process| uid.is_none_or(|uid| uid.as_ref() == process.user_id()))
            .filter(|process| match &check.source {
                Source::Systemd(unit) => {
                    let unit = to_unit_name(unit);
                    in_cgroup(process, &|part| part == unit)
                }
                Source::Docker(_) => container_id
                    .is_some_and(|id| in_cgroup(process, &|part| part.contains(id))),
                Source::Process(pattern) => is_matched(process, pattern),
            })
            .collect()
    }

    fn read_container(&mut self, check: &ProcessCheck) -> Option<Container> {
        let Source::Docker(container) = &check.source else { return None };
        let id = self.container_ids.get(container)?.clone();

        if let Some(info) = self.containers.get(&id) {
            return Some(info.clone());
        }

        let info = inspect_container(&id)?;
        self.containers.retain(|_, cached| {
            self.last_containers.values().any(|last| last.id == cached.id)
        });

        self.containers.insert(id, info.clone());
        Some(info)
    }

    fn find_container_id(&mut self, container: &str) -> Option<String> {
        if let Some(id) = self.container_ids.get(container) {
            return Some(id.clone());
        }
        let id = find_container_id(container)?;
        self.container_ids.insert(container.to_string(), id.clone());
        Some(id)
    }

    fn restart_process(
        &mut self,
        name: &str,
        controller: &Controller,
        restart: Restart,
        events: &mut Vec<Event>,
    ) {
        if self.gave_up.contains(name) {
            return;
        }
        let attempts = self.restarts.entry(name.to_string()).or_default();
        if !take_attempt(attempts, Instant::now(), restart) {
            warn!("{name} is still down after {} restarts, giving up", restart.attempts);
            self.gave_up.insert(name.to_string());
            events.push(new_event(EventKind::ProcessRestartGaveUp {
                name: name.to_string(),
                attempts: restart.attempts,
            }));
            return;
        }
        let attempt = attempts.len();

        match controller.run("restart") {
            Ok(()) => {
                info!("restarted {name}, attempt {attempt} of {}", restart.attempts);
                events.push(new_event(EventKind::ProcessRestarted {
                    name: name.to_string(),
                }));
            }
            Err(error) => {
                warn!("restart {name} failed: {error:#}");
                events.push(new_event(EventKind::ProcessRestartFailed {
                    name: name.to_string(),
                }));
            }
        }
    }
}

fn take_attempt(attempts: &mut Vec<Instant>, now: Instant, restart: Restart) -> bool {
    attempts.retain(|at| now.duration_since(*at) < restart.within);
    if attempts.len() as u32 >= restart.attempts {
        return false;
    }
    attempts.push(now);
    true
}

fn read_cgroups(system: &System) -> HashMap<Pid, String> {
    system
        .processes()
        .iter()
        .filter(|(_, process)| process.thread_kind().is_none())
        .filter_map(|(pid, _)| {
            Some((*pid, fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?))
        })
        .collect()
}

fn has_cgroup_part(cgroup: &str, is_part: &dyn Fn(&str) -> bool) -> bool {
    cgroup
        .lines()
        .filter_map(|line| line.splitn(3, ':').nth(2))
        .filter(|path| !path.split('/').any(|part| part.starts_with("user@")))
        .any(|path| path.split('/').any(is_part))
}

fn to_unit_name(unit: &str) -> String {
    if unit.contains('.') { unit.to_string() } else { format!("{unit}.service") }
}

fn is_matched(process: &Process, pattern: &str) -> bool {
    let name = process.name().to_string_lossy();
    let exe = process
        .exe()
        .and_then(|exe| exe.file_name())
        .map(|exe| exe.to_string_lossy());
    let command = process
        .cmd()
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let names: Vec<&str> =
        [Some(name.as_ref()), exe.as_deref()].into_iter().flatten().collect();
    matches_pattern(pattern, &names, &command)
}

fn matches_pattern(pattern: &str, names: &[&str], command: &str) -> bool {
    if let Some(inner) = pattern.strip_prefix('*').and_then(|rest| rest.strip_suffix('*'))
    {
        return command.contains(inner);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return names.iter().any(|name| name.starts_with(prefix));
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return names.iter().any(|name| name.ends_with(suffix));
    }
    names.contains(&pattern)
}

fn collect_family<'a>(system: &'a System, matches: &[&Process]) -> Vec<&'a Process> {
    let parents: HashMap<Pid, Option<Pid>> = system
        .processes()
        .iter()
        .filter(|(_, process)| process.thread_kind().is_none())
        .map(|(pid, process)| (*pid, process.parent()))
        .collect();
    let roots: HashSet<Pid> = matches.iter().map(|process| process.pid()).collect();
    find_family(&parents, &roots)
        .into_iter()
        .filter_map(|pid| system.process(pid))
        .collect()
}

fn find_family<T: Copy + Eq + Hash>(
    parents: &HashMap<T, Option<T>>,
    roots: &HashSet<T>,
) -> HashSet<T> {
    let mut family = roots.clone();
    for pid in parents.keys() {
        let mut chain = vec![*pid];
        let mut current = *pid;
        while let Some(Some(parent)) = parents.get(&current) {
            if family.contains(parent) {
                family.extend(chain);
                break;
            }
            if chain.contains(parent) {
                break;
            }
            chain.push(*parent);
            current = *parent;
        }
    }
    family
}

fn new_event(kind: EventKind) -> Event {
    Event { at: now_millis(), kind }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn counts_children_and_grandchildren_of_a_watched_process() {
        let parents: HashMap<u32, Option<u32>> = HashMap::from([
            (1, None),
            (10, Some(1)),
            (11, Some(10)),
            (12, Some(11)),
            (20, Some(1)),
            (21, Some(20)),
        ]);
        let family = find_family(&parents, &HashSet::from([10]));
        assert_eq!(family, HashSet::from([10, 11, 12]));
    }

    #[test]
    fn matches_names_and_command_lines() {
        let names = ["containerd-shim", "containerd-shim-runc-v2"];
        let command = "/usr/bin/python3 -m celery worker -A app";
        assert!(matches_pattern("containerd-shim-runc-v2", &names, command));
        assert!(!matches_pattern("containerd", &names, command));
        assert!(matches_pattern("containerd*", &names, command));
        assert!(matches_pattern("*runc-v2", &names, command));
        assert!(matches_pattern("*celery worker*", &names, command));
        assert!(!matches_pattern("*celery beat*", &names, command));
    }

    #[test]
    fn finds_units_and_containers_in_cgroups() {
        let unit = "0::/system.slice/nginx.service\n";
        let nested = "12:pids:/system.slice/docker.service/x\n1:name=systemd:/system.slice/docker.service\n";
        let container = "0::/system.slice/docker-4f2a9c.scope\n";
        assert!(has_cgroup_part(unit, &|part| part == "nginx.service"));
        assert!(has_cgroup_part(nested, &|part| part == "docker.service"));
        assert!(!has_cgroup_part(container, &|part| part == "docker.service"));
        assert!(has_cgroup_part(container, &|part| part.contains("4f2a9c")));
        let user_unit =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/nginx.service";
        assert!(!has_cgroup_part(user_unit, &|part| part == "nginx.service"));
        assert_eq!(to_unit_name("nginx"), "nginx.service");
        assert_eq!(to_unit_name("backup.timer"), "backup.timer");
    }

    #[test]
    fn reports_a_main_process_that_changed_pid_between_checks() {
        let check = ProcessCheck {
            name: "sleeper".to_string(),
            source: Source::Process("*sleep 3007.25*".to_string()),
            user: None,
            restart: None,
            logs: None,
        };
        let mut watch = ProcessWatch::new(vec![check], false, HeldProcesses::default());
        let spawn =
            || std::process::Command::new("sleep").arg("3007.25").spawn().unwrap();
        let check_now = |watch: &mut ProcessWatch| {
            let mut system = System::new();
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::All,
                true,
                sysinfo::ProcessRefreshKind::nothing()
                    .with_cmd(sysinfo::UpdateKind::Always),
            );
            let mut events = Vec::new();
            watch.check_processes(&system, 1.0, &mut events);
            events
        };

        let mut first = spawn();
        assert!(check_now(&mut watch).is_empty());
        first.kill().unwrap();
        first.wait().unwrap();
        let mut second = spawn();
        let events = check_now(&mut watch);
        second.kill().unwrap();
        second.wait().unwrap();

        let expected = EventKind::ProcessChanged {
            name: "sleeper".to_string(),
            old_pid: first.id(),
            new_pid: second.id(),
        };
        assert_eq!(
            events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
            [&expected]
        );

        let mut third = spawn();
        check_now(&mut watch);
        hold(&watch.held).restarted.insert("sleeper".to_string());
        third.kill().unwrap();
        third.wait().unwrap();
        let mut fourth = spawn();
        let events = check_now(&mut watch);
        fourth.kill().unwrap();
        fourth.wait().unwrap();
        assert!(events.is_empty());
        assert!(hold(&watch.held).restarted.is_empty());
    }

    #[test]
    fn restarts_only_while_the_window_has_attempts_left() {
        let restart = Restart { attempts: 2, within: Duration::from_secs(600) };
        let start = Instant::now();
        let mut attempts = Vec::new();
        assert!(take_attempt(&mut attempts, start, restart));
        assert!(take_attempt(&mut attempts, start + Duration::from_secs(10), restart));
        assert!(!take_attempt(&mut attempts, start + Duration::from_secs(20), restart));
        assert!(take_attempt(&mut attempts, start + Duration::from_secs(605), restart));
    }
}
