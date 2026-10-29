use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::Path;

use daroyan_protocol::report::TopProcess;
use sysinfo::{Pid, Process, System};

const TOP_COUNT: usize = 10;
const INTERPRETERS: [&str; 12] = [
    "node", "nodejs", "bun", "deno", "python", "ruby", "php", "java", "perl", "dotnet",
    "bash", "sh",
];

pub fn find_top(
    system: &System,
    families: &HashMap<String, HashSet<Pid>>,
) -> Vec<TopProcess> {
    let owners: HashMap<Pid, &str> = families
        .iter()
        .flat_map(|(name, pids)| pids.iter().map(move |pid| (*pid, name.as_str())))
        .collect();
    let mut groups: HashMap<String, TopProcess> = HashMap::new();

    for (pid, process) in system.processes() {
        if process.thread_kind().is_some() {
            continue;
        }

        let name = owners
            .get(pid)
            .map_or_else(|| read_name(process), ToString::to_string);
        let group = groups.entry(name.clone()).or_insert_with(|| TopProcess {
            name,
            count: 0,
            cpu: 0.0,
            mem_used: 0,
        });

        group.count += 1;
        group.cpu += process.cpu_usage();
        group.mem_used += process.memory();
    }

    pick_top(groups.into_values().collect())
}

fn pick_top(mut groups: Vec<TopProcess>) -> Vec<TopProcess> {
    groups.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let mut top: Vec<TopProcess> = groups.iter().take(TOP_COUNT).cloned().collect();

    groups.sort_by_key(|group| std::cmp::Reverse(group.mem_used));
    for group in groups.into_iter().take(TOP_COUNT) {
        if !top.iter().any(|item| item.name == group.name) {
            top.push(group);
        }
    }

    top
}

fn read_name(process: &Process) -> String {
    let name = process.name().to_string_lossy().to_string();

    if is_interpreter(&name)
        && let Some(script) = find_script(&name, process.cmd())
    {
        return script;
    }

    name
}

fn is_interpreter(name: &str) -> bool {
    let base = name.trim_end_matches(|character: char| {
        character.is_ascii_digit() || character == '.'
    });
    INTERPRETERS.contains(&base)
}

fn find_script(interpreter: &str, cmd: &[OsString]) -> Option<String> {
    let is_php = interpreter.starts_with("php");
    let mut args = cmd.iter().skip(1).map(|arg| arg.to_string_lossy());

    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "-m" | "-jar" => return to_script_name(&args.next()?),
            "-r" if is_php => return None,
            "-c" | "-e" | "-E" | "-p" | "--eval" | "--print" | "--command" => {
                return None;
            }
            "-r" => {
                args.next();
            }
            "-cp" | "-classpath" | "--class-path" | "-W" | "-X" | "-I" | "--require"
            | "--import" | "--loader" => {
                args.next();
            }
            flag if flag.starts_with('-') => {}
            script => return to_script_name(script),
        }
    }

    None
}

fn to_script_name(arg: &str) -> Option<String> {
    let name = to_file_name(arg);
    let is_name = !name.is_empty()
        && name.len() <= 64
        && !name.chars().any(|character| {
            character.is_whitespace() || "'\"`;$(){}=".contains(character)
        });

    is_name.then_some(name)
}

fn to_file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |name| name.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn group(name: &str, cpu: f32, mem_used: u64) -> TopProcess {
        TopProcess { name: name.to_string(), count: 1, cpu, mem_used }
    }

    #[test]
    fn names_interpreted_programs_after_their_script() {
        assert!(is_interpreter("python3.12"));
        assert!(is_interpreter("node"));
        assert!(!is_interpreter("nginx"));

        assert_eq!(
            find_script(
                "node",
                &cmd(&["node", "--max-old-space-size=512", "/app/dist/server.js"])
            ),
            Some("server.js".to_string())
        );
        assert_eq!(
            find_script("python3", &cmd(&["python3", "-m", "celery", "worker"])),
            Some("celery".to_string())
        );
        assert_eq!(
            find_script(
                "java",
                &cmd(&["java", "-Xmx1g", "-jar", "/opt/app/billing.jar"])
            ),
            Some("billing.jar".to_string())
        );
        assert_eq!(find_script("bash", &cmd(&["bash", "-c", "sleep 1"])), None);
        assert_eq!(find_script("php", &cmd(&["php", "-r", "echo getenv('KEY');"])), None);
        assert_eq!(find_script("node", &cmd(&["node", "--eval", "run()"])), None);
        assert_eq!(
            find_script("node", &cmd(&["node", "-r", "dotenv/config", "app.js"])),
            Some("app.js".to_string())
        );
        assert_eq!(
            find_script("ruby", &cmd(&["ruby", "-r", "bundler/setup", "worker.rb"])),
            Some("worker.rb".to_string())
        );
        assert_eq!(
            find_script("java", &cmd(&["java", "-cp", "/opt/x.jar", "com.acme.Main"])),
            Some("com.acme.Main".to_string())
        );
        assert_eq!(find_script("sh", &cmd(&["sh", "echo $TOKEN; run"])), None);
        assert_eq!(find_script("node", &cmd(&["node"])), None);
    }

    #[test]
    fn keeps_the_top_by_cpu_and_by_memory() {
        let mut groups: Vec<TopProcess> = (0..12)
            .map(|index| group(&format!("busy-{index}"), 50.0 + index as f32, 1))
            .collect();
        groups.push(group("cache", 0.1, 4_000_000_000));

        let top = pick_top(groups);
        assert_eq!(top.len(), TOP_COUNT + 1);
        assert_eq!(top[0].name, "busy-11");
        assert_eq!(top.last().unwrap().name, "cache");
    }
}
