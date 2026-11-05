use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::args::GrantsArgs;
use crate::check::process::to_unit_name;
use crate::config::{Config, ProcessCheck, Source, load_config};
use crate::service::find_container_names;

const POLKIT_RULE: &str = "/etc/polkit-1/rules.d/50-daroyan.rules";
const SUDOERS: &str = "/etc/sudoers.d/daroyan";
const ALL_VERBS: &[&str] = &["start", "stop", "restart"];
const RESTART_ONLY: &[&str] = &["restart"];

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Unit(String),
    Container(String),
}

#[derive(Debug, PartialEq, Eq)]
struct Grant {
    target: Target,
    verbs: &'static [&'static str],
}

pub fn write_grants(args: &GrantsArgs) -> Result<()> {
    if !nix::unistd::geteuid().is_root() {
        bail!("grants writes polkit and sudoers files, so it must run as root");
    }

    let config = load_config(&args.config)?;
    let grants = if args.commands {
        collect_grants(&config, find_container_names)
    } else {
        Vec::new()
    };
    let uses_polkit = Path::new("/run/systemd/system").is_dir()
        && Path::new("/etc/polkit-1/rules.d").is_dir();
    let systemctl = find_program("systemctl").filter(|_| !uses_polkit);

    let polkit = uses_polkit.then(|| render_polkit(&grants)).flatten();
    replace_file(Path::new(POLKIT_RULE), polkit.as_deref(), 0o644, None)?;

    let sudoers =
        render_sudoers(&grants, find_program("docker").as_deref(), systemctl.as_deref());
    replace_file(Path::new(SUDOERS), sudoers.as_deref(), 0o440, Some(check_sudoers))
}

fn collect_grants(
    config: &Config,
    find_containers: impl Fn(&str) -> Vec<String>,
) -> Vec<Grant> {
    let mut grants = Vec::new();

    for check in &config.processes {
        let Some(verbs) = allowed_verbs(config, check) else {
            continue;
        };

        for target in resolve_targets(&check.source, &find_containers) {
            grants.push(Grant { target, verbs });
        }
    }

    grants
}

fn allowed_verbs(
    config: &Config,
    check: &ProcessCheck,
) -> Option<&'static [&'static str]> {
    if config.agent.commands {
        return Some(ALL_VERBS);
    }

    check.restart.is_some().then_some(RESTART_ONLY)
}

fn resolve_targets(
    source: &Source,
    find_containers: impl Fn(&str) -> Vec<String>,
) -> Vec<Target> {
    match source {
        Source::Systemd(unit) if is_safe_name(unit) => vec![Target::Unit(unit.clone())],
        Source::Docker(container) if is_safe_name(container) => {
            vec![Target::Container(container.clone())]
        }
        Source::Docker(pattern) if is_safe_pattern(pattern) => find_containers(pattern)
            .into_iter()
            .filter(|name| is_safe_name(name))
            .map(Target::Container)
            .collect(),
        Source::Process(_) => Vec::new(),
        Source::Systemd(name) | Source::Docker(name) => {
            eprintln!("skipping {name}: its name has characters a grant can't hold");
            Vec::new()
        }
    }
}

fn is_safe_name(name: &str) -> bool {
    !name.starts_with('-') && !name.is_empty() && name.chars().all(is_safe_character)
}

fn is_safe_pattern(pattern: &str) -> bool {
    !pattern.starts_with('-')
        && pattern.contains('*')
        && pattern
            .chars()
            .all(|character| character == '*' || is_safe_character(character))
}

fn is_safe_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-' | '@' | ':')
}

fn render_polkit(grants: &[Grant]) -> Option<String> {
    let mut allowed = Map::new();
    for grant in grants {
        if let Target::Unit(unit) = &grant.target {
            allowed.insert(to_unit_name(unit), Value::from(grant.verbs.to_vec()));
        }
    }
    if allowed.is_empty() {
        return None;
    }

    let allowed = Value::Object(allowed);
    Some(format!(
        r#"// Written by `daroyan grants` from /etc/daroyan/agent.yml on every agent start.
polkit.addRule(function (action, subject) {{
  if (action.id != "org.freedesktop.systemd1.manage-units" || subject.user != "daroyan") {{
    return polkit.Result.NOT_HANDLED;
  }}
  var allowed = {allowed};
  var verbs = allowed[action.lookup("unit")] || [];
  if (verbs.indexOf(action.lookup("verb")) >= 0) {{
    return polkit.Result.YES;
  }}
  return polkit.Result.NOT_HANDLED;
}});
"#
    ))
}

fn render_sudoers(
    grants: &[Grant],
    docker: Option<&Path>,
    systemctl: Option<&Path>,
) -> Option<String> {
    let mut rules = Vec::new();

    if let Some(docker) = docker {
        let docker = docker.display();
        for read in ["ps", "inspect", "logs", "events"] {
            rules.push(format!("{docker} {read} *"));
        }
    }

    for grant in grants {
        let (program, name) = match &grant.target {
            Target::Container(container) => (docker, container),
            Target::Unit(unit) => (systemctl, unit),
        };
        let Some(program) = program else {
            continue;
        };
        for verb in grant.verbs {
            rules.push(format!("{} {verb} {name}", program.display()));
        }
    }

    if rules.is_empty() {
        return None;
    }

    Some(format!(
        "# Written by `daroyan grants` from /etc/daroyan/agent.yml on every agent start.\ndaroyan ALL=(root) NOPASSWD: {}\n",
        rules.join(", ")
    ))
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

fn check_sudoers(path: &Path) -> Result<()> {
    let status = Command::new("visudo")
        .arg("-cf")
        .arg(path)
        .stdout(Stdio::null())
        .status()
        .context("run visudo")?;
    if !status.success() {
        bail!("visudo rejected the generated sudoers file");
    }

    Ok(())
}

fn replace_file(
    path: &Path,
    contents: Option<&str>,
    mode: u32,
    check: Option<fn(&Path) -> Result<()>>,
) -> Result<()> {
    let Some(contents) = contents else {
        return match fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(error).with_context(|| format!("remove {}", path.display()))
            }
            _ => Ok(()),
        };
    };

    let Some(directory) = path.parent().filter(|directory| directory.is_dir()) else {
        eprintln!("skipping {}: its directory does not exist", path.display());
        return Ok(());
    };

    let staged = directory.join(".daroyan.new");
    let _ = fs::remove_file(&staged);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&staged)
        .with_context(|| format!("create {}", staged.display()))?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;

    if let Some(check) = check
        && let Err(error) = check(&staged)
    {
        let _ = fs::remove_file(&staged);
        return Err(error);
    }

    fs::rename(&staged, path).with_context(|| format!("replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(yaml: &str) -> Config {
        serde_norway::from_str(yaml).unwrap()
    }

    #[test]
    fn grants_only_configured_entries() {
        let config = config(
            "processes:\n  - systemd: nginx\n  - docker: db\n  - process: cron\nagent:\n  commands: true\n",
        );
        let grants = collect_grants(&config, |_| Vec::new());

        assert_eq!(
            grants,
            vec![
                Grant {
                    target: Target::Unit("nginx".into()),
                    verbs: ALL_VERBS
                },
                Grant {
                    target: Target::Container("db".into()),
                    verbs: ALL_VERBS
                },
            ]
        );
    }

    #[test]
    fn lists_each_container_a_pattern_matches() {
        let config = config("processes:\n  - docker: app-*\nagent:\n  commands: true\n");
        let grants = collect_grants(&config, |pattern| {
            assert_eq!(pattern, "app-*");
            vec!["app-blue".into(), "app-green".into(), "bad name".into()]
        });

        assert_eq!(
            grants,
            vec![
                Grant {
                    target: Target::Container("app-blue".into()),
                    verbs: ALL_VERBS
                },
                Grant {
                    target: Target::Container("app-green".into()),
                    verbs: ALL_VERBS
                },
            ]
        );

        let sudoers =
            render_sudoers(&grants, Some(Path::new("/usr/bin/docker")), None).unwrap();
        let rules = sudoers.lines().nth(1).unwrap();
        assert!(rules.contains("/usr/bin/docker restart app-blue,"));
        assert!(!rules.contains("app-*"));
        assert!(!rules.contains("restart *"));
    }

    #[test]
    fn auto_restart_alone_grants_restart() {
        let config = config(
            "processes:\n  - systemd: redis\n    restart:\n      attempts: 3\n  - systemd: web\n",
        );

        assert_eq!(
            collect_grants(&config, |_| Vec::new()),
            vec![Grant {
                target: Target::Unit("redis".into()),
                verbs: RESTART_ONLY
            }]
        );
    }

    #[test]
    fn skips_names_a_rule_could_be_widened_by() {
        let config = config(
            "processes:\n  - systemd: \"nginx --help\"\n  - systemd: \"-x\"\nagent:\n  commands: true\n",
        );

        assert!(collect_grants(&config, |_| Vec::new()).is_empty());
    }

    #[test]
    fn renders_exact_rules() {
        let grants = vec![
            Grant {
                target: Target::Unit("nginx".into()),
                verbs: ALL_VERBS,
            },
            Grant {
                target: Target::Container("app".into()),
                verbs: RESTART_ONLY,
            },
        ];

        let polkit = render_polkit(&grants).unwrap();
        assert!(polkit.contains(r#"{"nginx.service":["start","stop","restart"]}"#));
        assert!(!polkit.contains("\\.service$"));

        let sudoers = render_sudoers(
            &grants,
            Some(Path::new("/usr/bin/docker")),
            Some(Path::new("/usr/bin/systemctl")),
        )
        .unwrap();
        assert!(sudoers.contains("/usr/bin/systemctl stop nginx,"));
        assert!(sudoers.contains("/usr/bin/docker restart app\n"));
        assert!(!sudoers.contains("systemctl start *"));
        assert!(!sudoers.contains("docker stop"));
    }

    #[test]
    fn writes_nothing_without_grants_or_docker() {
        assert!(render_polkit(&[]).is_none());
        assert!(
            render_sudoers(&[], None, Some(Path::new("/usr/bin/systemctl"))).is_none()
        );
    }
}
