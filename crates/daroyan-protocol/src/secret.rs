use std::borrow::Cow;

const MASK: &str = "[REDACTED]";
const QUOTES: [char; 3] = ['\'', '"', '\\'];
const SECRET_WORDS: [&str; 5] =
    ["pass", "bearer", "credential", "credentials", "authkey"];
const SECRET_ENDINGS: [&str; 6] =
    ["password", "passwd", "passphrase", "secret", "token", "apikey"];
const SECRET_PAIRS: [(&str, &str); 6] = [
    ("api", "key"),
    ("access", "key"),
    ("secret", "key"),
    ("private", "key"),
    ("auth", "key"),
    ("mysql", "pwd"),
];
const NOT_VALUES: [&str; 4] = ["file", "path", "dir", "stdin"];
const AUTH_HEADERS: [&str; 3] = ["authorization", "proxy-authorization", "cookie"];
const AUTH_SCHEMES: [&str; 4] = ["bearer", "basic", "digest", "token"];
const TOKEN_PREFIXES: [&str; 26] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xoxs-",
    "sk_live_",
    "rk_live_",
    "sk-",
    "vck_",
    "hf_",
    "api_org_",
    "gsk_",
    "xai-",
    "r8_",
    "pplx-",
    "AIza",
    "ibx",
    "tskey-",
    "hvs.",
];
const MIN_TOKEN_LEN: usize = 16;
const AWS_KEY_ID_LEN: usize = 20;
const ENCODED_TOKEN_LEN: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Program {
    Mysql,
    Curl,
    Redis,
    Sshpass,
    Docker,
    DockerLogin,
}

impl Program {
    fn from_word(word: &str) -> Option<Self> {
        let word = word.strip_prefix("COMMAND=").unwrap_or(word);
        if word.contains('/') && !word.contains("/bin/") {
            return None;
        }

        match word.rsplit('/').next().unwrap_or(word) {
            "mysql" | "mysqldump" | "mysqladmin" | "mysqlimport" | "mysqlcheck"
            | "mariadb" | "mariadb-dump" => Some(Self::Mysql),
            "curl" => Some(Self::Curl),
            "redis-cli" => Some(Self::Redis),
            "sshpass" => Some(Self::Sshpass),
            "docker" | "podman" => Some(Self::Docker),
            _ => None,
        }
    }

    fn attached_flag(self) -> Option<&'static str> {
        match self {
            Self::Mysql | Self::Sshpass | Self::DockerLogin => Some("-p"),
            Self::Curl => Some("-u"),
            Self::Redis => Some("-a"),
            Self::Docker => None,
        }
    }

    fn value_flags(self) -> &'static [&'static str] {
        match self {
            Self::Curl => &["-u", "--user"],
            Self::Redis => &["-a"],
            Self::Sshpass | Self::DockerLogin => &["-p"],
            Self::Mysql | Self::Docker => &[],
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Pending {
    #[default]
    Nothing,
    Value,
    Header,
}

fn is_one_of(word: &str, list: &[&str]) -> bool {
    list.iter().any(|item| word.eq_ignore_ascii_case(item))
}

fn ends_with_ignore_case(word: &str, ending: &str) -> bool {
    word.len() >= ending.len()
        && word.as_bytes()[word.len() - ending.len()..]
            .eq_ignore_ascii_case(ending.as_bytes())
}

fn is_secret_name(name: &str) -> bool {
    let name = name.trim_start_matches('-');
    if name.get(..3).is_some_and(|start| start.eq_ignore_ascii_case("no-")) {
        return false;
    }

    let mut is_secret = false;
    let mut previous = "";
    let mut last = "";
    for word in name.split(['-', '_', '.']) {
        is_secret = is_secret
            || is_one_of(word, &SECRET_WORDS)
            || SECRET_ENDINGS
                .iter()
                .any(|ending| ends_with_ignore_case(word, ending))
            || SECRET_PAIRS.iter().any(|(first, second)| {
                previous.eq_ignore_ascii_case(first) && word.eq_ignore_ascii_case(second)
            });
        previous = word;
        last = word;
    }

    is_secret && !is_one_of(last, &NOT_VALUES)
}

fn is_known_token(word: &str) -> bool {
    if word.len() < MIN_TOKEN_LEN {
        return false;
    }

    TOKEN_PREFIXES.iter().any(|prefix| word.starts_with(prefix))
        || (word.starts_with("AKIA") && word.len() == AWS_KEY_ID_LEN)
        || (word.starts_with("eyJ") && word.len() >= ENCODED_TOKEN_LEN)
}

fn mask_url(word: &str) -> Option<String> {
    let start = word.find("://")? + 3;
    let mut out = String::new();
    let mut done = 0;
    let mut hide = |from: usize, to: usize, out: &mut String| {
        out.push_str(&word[done..from]);
        out.push_str(MASK);
        done = to;
    };

    let rest = &word[start..];
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if let Some(at) = rest[..host_end].rfind('@') {
        let user = &rest[..at];
        let secret_start = match user.find(':') {
            Some(colon) if colon + 1 < at => Some(colon + 1),
            Some(_) => None,
            None => is_known_token(user).then_some(0),
        };
        if let Some(secret_start) = secret_start {
            hide(start + secret_start, start + at, &mut out);
        }
    }

    if let Some(question) = word.find('?') {
        let mut position = question + 1;
        for pair in word[question + 1..].split('&') {
            if let Some((name, value)) = pair.split_once('=')
                && !value.is_empty()
                && (is_secret_name(name) || is_known_token(value))
            {
                let value_start = position + name.len() + 1;
                hide(value_start, value_start + value.len(), &mut out);
            }
            position += pair.len() + 1;
        }
    }

    (done > 0).then(|| {
        out.push_str(&word[done..]);
        out
    })
}

fn mask_assignment(word: &str) -> Option<String> {
    let (name, value) = word.split_once('=')?;
    if value.is_empty() {
        return None;
    }
    if is_secret_name(name) {
        return Some(format!("{name}={MASK}"));
    }

    mask_assignment(value).map(|masked| format!("{name}={masked}"))
}

#[derive(Default)]
struct Masker {
    pending: Pending,
    program: Option<Program>,
}

impl Masker {
    fn mask(&mut self, word: &str) -> Option<String> {
        if word.is_empty() {
            return None;
        }
        match self.pending {
            Pending::Header if is_one_of(word, &AUTH_SCHEMES) => return None,
            Pending::Header => {
                self.pending = Pending::Nothing;
                return Some(MASK.to_string());
            }
            Pending::Value => {
                self.pending = Pending::Nothing;
                if !word.starts_with('-') {
                    return Some(MASK.to_string());
                }
            }
            Pending::Nothing => {}
        }

        if let Some(program) = Program::from_word(word) {
            self.program = Some(program);
        }
        if word == "login" && self.program == Some(Program::Docker) {
            self.program = Some(Program::DockerLogin);
        }

        if is_known_token(word) {
            return Some(MASK.to_string());
        }
        if let Some(masked) = mask_url(word) {
            return Some(masked);
        }
        if word.contains('=') {
            return mask_assignment(word);
        }
        if let Some(name) = word.strip_suffix(':') {
            self.pending = if is_one_of(name, &AUTH_HEADERS) {
                Pending::Header
            } else if is_secret_name(name) {
                Pending::Value
            } else {
                Pending::Nothing
            };
            return None;
        }
        if let Some((name, value)) = word.split_once(':')
            && !value.is_empty()
        {
            if is_one_of(name, &AUTH_HEADERS) && is_one_of(value, &AUTH_SCHEMES) {
                self.pending = Pending::Header;
                return None;
            }
            if is_one_of(name, &AUTH_HEADERS) || is_secret_name(name) {
                return Some(format!("{name}:{MASK}"));
            }
        }

        let program = self.program?;
        if let Some(flag) = program.attached_flag()
            && word.len() > flag.len()
            && word.starts_with(flag)
            && !word.starts_with("--")
        {
            return Some(format!("{flag}{MASK}"));
        }
        if word.starts_with('-') && program.value_flags().contains(&word) {
            self.pending = Pending::Value;
        }

        None
    }

    fn watch_flag(&mut self, word: &str) {
        if self.pending == Pending::Nothing
            && word.starts_with('-')
            && !word.contains(['=', ':'])
            && is_secret_name(word)
        {
            self.pending = Pending::Value;
        }
    }
}

pub(crate) fn mask_secrets(text: &str) -> Cow<'_, str> {
    let mut masker = Masker::default();
    let mut masked: Option<String> = None;
    let mut done = 0;
    let mut start = 0;

    for word in text.split(' ') {
        let core = word.trim_matches(QUOTES);
        let core_start = start + word.len() - word.trim_start_matches(QUOTES).len();
        match masker.mask(core) {
            Some(replacement) => {
                let out = masked.get_or_insert_with(|| String::with_capacity(text.len()));
                out.push_str(&text[done..core_start]);
                out.push_str(&replacement);
                done = core_start + core.len();
            }
            None => masker.watch_flag(core),
        }
        start += word.len() + 1;
    }

    match masked {
        Some(mut out) => {
            out.push_str(&text[done..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::mask_secrets;

    fn sudo(command: &str) -> String {
        format!("arik : TTY=pts/0 ; PWD=/root ; USER=root ; COMMAND={command}")
    }

    #[test]
    fn masks_only_the_secret_values() {
        let cases = [
            ("/usr/bin/mysql -uroot -pS3cretPass", "/usr/bin/mysql -uroot -p[REDACTED]"),
            (
                "/bin/sh -c mysqldump -pS3cret app",
                "/bin/sh -c mysqldump -p[REDACTED] app",
            ),
            (
                "/usr/bin/curl -H Authorization: Bearer abc.def -X GET https://api.test",
                "/usr/bin/curl -H Authorization: Bearer [REDACTED] -X GET https://api.test",
            ),
            (
                "/usr/bin/curl -H X-Api-Key: k3y https://api.test",
                "/usr/bin/curl -H X-Api-Key: [REDACTED] https://api.test",
            ),
            (
                "/usr/bin/curl -H x-api-key:k3y https://api.test",
                "/usr/bin/curl -H x-api-key:[REDACTED] https://api.test",
            ),
            (
                "/usr/bin/curl -u admin:hunter2 https://a.test",
                "/usr/bin/curl -u [REDACTED] https://a.test",
            ),
            (
                "/usr/bin/curl -uadmin:hunter2 https://a.test",
                "/usr/bin/curl -u[REDACTED] https://a.test",
            ),
            (
                "/usr/bin/psql postgres://app:pa55@db.test:5432/app",
                "/usr/bin/psql postgres://app:[REDACTED]@db.test:5432/app",
            ),
            (
                "/usr/bin/curl https://api.test/v1?user=a&api_key=S3cret&page=2",
                "/usr/bin/curl https://api.test/v1?user=a&api_key=[REDACTED]&page=2",
            ),
            (
                "/usr/bin/docker login -u bot -p s3cret ghcr.io",
                "/usr/bin/docker login -u bot -p [REDACTED] ghcr.io",
            ),
            (
                "/usr/bin/docker login -u bot --password s3cret ghcr.io",
                "/usr/bin/docker login -u bot --password [REDACTED] ghcr.io",
            ),
            ("/usr/bin/app --password token", "/usr/bin/app --password [REDACTED]"),
            (
                "/usr/bin/app --api-key=k3y --verbose",
                "/usr/bin/app --api-key=[REDACTED] --verbose",
            ),
            (
                "/usr/bin/env PGPASSWORD=pa55 AWS_SECRET_ACCESS_KEY=abc psql",
                "/usr/bin/env PGPASSWORD=[REDACTED] AWS_SECRET_ACCESS_KEY=[REDACTED] psql",
            ),
            (
                "/usr/bin/kubectl create secret generic app --from-literal=password=S3cret",
                "/usr/bin/kubectl create secret generic app --from-literal=password=[REDACTED]",
            ),
            ("/usr/bin/redis-cli -a pa55 ping", "/usr/bin/redis-cli -a [REDACTED] ping"),
            (
                "/usr/bin/sshpass -pS3cret ssh host",
                "/usr/bin/sshpass -p[REDACTED] ssh host",
            ),
            (
                "/usr/bin/env MYSQL_PWD=pa55 mysql",
                "/usr/bin/env MYSQL_PWD=[REDACTED] mysql",
            ),
            (
                "/usr/bin/tailscale up --authkey tskey-auth-0123456789abcdef",
                "/usr/bin/tailscale up --authkey [REDACTED]",
            ),
            (
                "/usr/bin/tailscale up --auth-key=k3y",
                "/usr/bin/tailscale up --auth-key=[REDACTED]",
            ),
            (
                "/usr/bin/cloudflared service install eyJhIjoiMDEyMzQ1Njc4OWFiY2RlZiJ9",
                "/usr/bin/cloudflared service install [REDACTED]",
            ),
            (
                "/usr/bin/git push https://ghp_0123456789abcdefghij@github.com/a/b",
                "/usr/bin/git push https://[REDACTED]@github.com/a/b",
            ),
            (
                "/usr/bin/gh auth login --with-token ghp_0123456789abcdefghij",
                "/usr/bin/gh auth login --with-token [REDACTED]",
            ),
            ("/usr/bin/aws s3 ls AKIAABCDEFGHIJKLMNOP", "/usr/bin/aws s3 ls [REDACTED]"),
            (
                "/usr/bin/env OPENAI_API_KEY=abc /usr/bin/app sk-proj-0123456789abcdef",
                "/usr/bin/env OPENAI_API_KEY=[REDACTED] /usr/bin/app [REDACTED]",
            ),
            (
                "/usr/bin/curl -H x-api-key: sk-ant-api03-0123456789abcdef https://api.test",
                "/usr/bin/curl -H x-api-key: [REDACTED] https://api.test",
            ),
            (
                "/usr/bin/app vck_0123456789abcdef hf_0123456789abcdef gsk_0123456789abcdef",
                "/usr/bin/app [REDACTED] [REDACTED] [REDACTED]",
            ),
            (
                "/usr/bin/app xai-0123456789abcdef AIzaSy0123456789abcdef ibx_0123456789abcdef",
                "/usr/bin/app [REDACTED] [REDACTED] [REDACTED]",
            ),
            (
                "/usr/bin/curl -H 'Authorization: Bearer abc' https://api.test",
                "/usr/bin/curl -H 'Authorization: Bearer [REDACTED]' https://api.test",
            ),
            (
                "/usr/bin/curl -H 'Authorization:Bearer abc' https://api.test",
                "/usr/bin/curl -H 'Authorization:Bearer [REDACTED]' https://api.test",
            ),
            (
                "/usr/bin/curl -H 'Cookie: session=abc' https://api.test",
                "/usr/bin/curl -H 'Cookie: [REDACTED]' https://api.test",
            ),
            (
                "/bin/sh -c 'curl -H \\'Authorization: Bearer S3cret\\' https://a.test'",
                "/bin/sh -c 'curl -H \\'Authorization: Bearer [REDACTED]\\' https://a.test'",
            ),
            (
                "/usr/bin/sh -c 'mysql -uroot -pS3cret'",
                "/usr/bin/sh -c 'mysql -uroot -p[REDACTED]'",
            ),
            (
                "/usr/bin/sh -c 'docker login -p s3cret ghcr.io'",
                "/usr/bin/sh -c 'docker login -p [REDACTED] ghcr.io'",
            ),
            (
                "/usr/bin/app \"--password=s3cret\"",
                "/usr/bin/app \"--password=[REDACTED]\"",
            ),
        ];

        for (command, expected) in cases {
            assert_eq!(mask_secrets(&sudo(command)), sudo(expected), "{command}");
        }
    }

    #[test]
    fn masks_variables_given_to_sudo() {
        let line = "arik : TTY=pts/0 ; PWD=/root ; USER=root ; ENV=PGPASSWORD=S3cret DD_API_KEY=k3y ; COMMAND=/usr/bin/psql";
        assert_eq!(
            mask_secrets(line),
            "arik : TTY=pts/0 ; PWD=/root ; USER=root ; ENV=PGPASSWORD=[REDACTED] DD_API_KEY=[REDACTED] ; COMMAND=/usr/bin/psql"
        );
    }

    #[test]
    fn keeps_ordinary_commands() {
        let commands = [
            "/usr/bin/mkdir -p /srv/app",
            "/usr/bin/cp -pr a b",
            "/usr/bin/ssh -p2222 -o PasswordAuthentication=no host",
            "/usr/bin/systemctl restart nginx",
            "/usr/bin/kubectl create secret generic app --from-file=key.pem",
            "/usr/bin/mysql --no-password app",
            "/usr/bin/mysql -u root --password -h db",
            "/usr/bin/docker login --password-stdin -u bot ghcr.io",
            "/usr/bin/vault login -token-file /etc/token",
            "/usr/bin/curl https://example.test/path?q=1",
            "/usr/bin/docker run -p 8080:80 nginx",
            "/usr/bin/git clone https://git@github.com/a/b",
            "/usr/bin/tar -C /var/lib/mysql -pxf backup.tar",
        ];

        for command in commands {
            let line = sudo(command);
            assert_eq!(mask_secrets(&line), line, "{command}");
        }
    }

    #[test]
    fn keeps_ssh_lines() {
        let lines = [
            "Accepted publickey for arik from 203.0.113.7 port 52144 ssh2: ED25519 SHA256:KZ1Zzo3OxUhz9O3lSyBYUO",
            "Invalid user admin from 203.0.113.7 port 52144",
            "Failed password for invalid user admin from 203.0.113.7 port 52144 ssh2",
        ];

        for line in lines {
            assert_eq!(mask_secrets(line), line);
        }
    }

    #[test]
    fn masking_twice_changes_nothing() {
        let once =
            mask_secrets(&sudo("/usr/bin/mysql --password s3cret -pX")).into_owned();
        assert_eq!(mask_secrets(&once), once);
    }
}
