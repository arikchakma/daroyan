use std::borrow::Cow;

pub const MASK: &str = "[REDACTED]";
const QUOTES: [char; 2] = ['\'', '"'];
const SECRET_WORDS: [&str; 10] = [
    "password",
    "passwd",
    "pass",
    "passphrase",
    "secret",
    "token",
    "bearer",
    "apikey",
    "credential",
    "credentials",
];
const SECRET_ENDINGS: [&str; 6] =
    ["password", "passwd", "passphrase", "secret", "token", "apikey"];
const SECRET_PAIRS: [(&str, &str); 5] = [
    ("api", "key"),
    ("access", "key"),
    ("secret", "key"),
    ("private", "key"),
    ("mysql", "pwd"),
];
const NOT_VALUES: [&str; 3] = ["file", "path", "dir"];
const AUTH_HEADERS: [&str; 2] = ["authorization", "proxy-authorization"];
const AUTH_SCHEMES: [&str; 4] = ["bearer", "basic", "digest", "token"];
const TOKEN_PREFIXES: [&str; 23] = [
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
];
const MIN_TOKEN_LEN: usize = 16;
const MYSQL_COMMANDS: [&str; 7] = [
    "mysql",
    "mysqldump",
    "mysqladmin",
    "mysqlimport",
    "mysqlcheck",
    "mariadb",
    "mariadb-dump",
];
const LOGIN_COMMANDS: [&str; 2] = ["docker", "podman"];
const VALUE_FLAGS: [(&str, &str); 6] = [
    ("curl", "-u"),
    ("curl", "--user"),
    ("redis-cli", "-a"),
    ("sshpass", "-p"),
    ("docker login", "-p"),
    ("podman login", "-p"),
];

fn is_secret_name(name: &str) -> bool {
    let name = name.trim_start_matches('-').to_ascii_lowercase();
    if name.starts_with("no-") {
        return false;
    }

    let words: Vec<&str> = name.split(['-', '_', '.']).collect();
    if words.last().is_some_and(|word| NOT_VALUES.contains(word)) {
        return false;
    }

    words.iter().any(|word| {
        SECRET_WORDS.contains(word)
            || SECRET_ENDINGS.iter().any(|ending| word.ends_with(ending))
    }) || words
        .windows(2)
        .any(|pair| SECRET_PAIRS.contains(&(pair[0], pair[1])))
}

fn is_known_token(word: &str) -> bool {
    if word.len() < MIN_TOKEN_LEN {
        return false;
    }

    TOKEN_PREFIXES.iter().any(|prefix| word.starts_with(prefix))
        || (word.starts_with("AKIA") && word.len() == 20)
        || (word.starts_with("eyJ") && word.matches('.').count() == 2)
}

fn program(word: &str) -> &str {
    let word = word.strip_prefix("COMMAND=").unwrap_or(word);
    word.rsplit('/').next().unwrap_or(word)
}

fn mask_url_password(word: &str) -> Option<String> {
    let start = word.find("://")? + 3;
    let rest = &word[start..];
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let at = rest[..host_end].rfind('@')?;
    let user = &rest[..at];
    let secret_start = match user.find(':') {
        Some(colon) if colon + 1 < at => colon + 1,
        Some(_) => return None,
        None if is_known_token(user) => 0,
        None => return None,
    };

    Some(format!("{}{MASK}{}", &word[..start + secret_start], &rest[at..]))
}

#[derive(Default)]
struct Masker {
    is_pending: bool,
    is_mysql: bool,
    program: String,
}

impl Masker {
    fn mask(&mut self, word: &str) -> Option<String> {
        if word.is_empty() {
            return None;
        }
        if self.is_pending {
            if AUTH_SCHEMES.contains(&word.to_ascii_lowercase().as_str()) {
                return None;
            }
            self.is_pending = false;
            return Some(MASK.to_string());
        }

        let name = program(word);
        if MYSQL_COMMANDS.contains(&name) {
            self.is_mysql = true;
        }
        if LOGIN_COMMANDS.contains(&name)
            || VALUE_FLAGS.iter().any(|(program, _)| *program == name)
        {
            self.program = name.to_string();
        }
        if word == "login" && LOGIN_COMMANDS.contains(&self.program.as_str()) {
            self.program = format!("{} login", self.program);
        }

        if is_known_token(word) {
            return Some(MASK.to_string());
        }
        if let Some(masked) = mask_url_password(word) {
            return Some(masked);
        }
        if let Some((name, value)) = word.split_once('=') {
            return (!value.is_empty() && is_secret_name(name))
                .then(|| format!("{name}={MASK}"));
        }
        if let Some(name) = word.strip_suffix(':') {
            self.is_pending = AUTH_HEADERS.contains(&name.to_ascii_lowercase().as_str())
                || is_secret_name(name);
            return None;
        }
        if let Some((name, value)) = word.split_once(':') {
            return (!value.is_empty() && is_secret_name(name))
                .then(|| format!("{name}:{MASK}"));
        }
        if self.is_mysql && word.len() > 2 && word.starts_with("-p") {
            return Some(format!("-p{MASK}"));
        }
        if word.starts_with('-') {
            self.is_pending = is_secret_name(word)
                || VALUE_FLAGS
                    .iter()
                    .any(|(program, flag)| *program == self.program && *flag == word);
        }

        None
    }
}

pub fn mask_secrets(text: &str) -> Cow<'_, str> {
    let mut masker = Masker::default();
    let mut masked: Option<String> = None;
    let mut done = 0;
    let mut start = 0;

    for word in text.split(' ') {
        let core = word.trim_matches(QUOTES);
        let core_start = start + word.len() - word.trim_start_matches(QUOTES).len();
        if let Some(replacement) = masker.mask(core) {
            let out = masked.get_or_insert_with(|| String::with_capacity(text.len()));
            out.push_str(&text[done..core_start]);
            out.push_str(&replacement);
            done = core_start + core.len();
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
                "/usr/bin/psql postgres://app:pa55@db.test:5432/app",
                "/usr/bin/psql postgres://app:[REDACTED]@db.test:5432/app",
            ),
            (
                "/usr/bin/docker login -u bot -p s3cret ghcr.io",
                "/usr/bin/docker login -u bot -p [REDACTED] ghcr.io",
            ),
            (
                "/usr/bin/docker login -u bot --password s3cret ghcr.io",
                "/usr/bin/docker login -u bot --password [REDACTED] ghcr.io",
            ),
            (
                "/usr/bin/app --api-key=k3y --verbose",
                "/usr/bin/app --api-key=[REDACTED] --verbose",
            ),
            (
                "/usr/bin/env PGPASSWORD=pa55 AWS_SECRET_ACCESS_KEY=abc psql",
                "/usr/bin/env PGPASSWORD=[REDACTED] AWS_SECRET_ACCESS_KEY=[REDACTED] psql",
            ),
            ("/usr/bin/redis-cli -a pa55 ping", "/usr/bin/redis-cli -a [REDACTED] ping"),
            (
                "/usr/bin/env MYSQL_PWD=pa55 mysql",
                "/usr/bin/env MYSQL_PWD=[REDACTED] mysql",
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
                "/usr/bin/app xai-0123456789abcdef AIzaSy0123456789abcdef",
                "/usr/bin/app [REDACTED] [REDACTED]",
            ),
            (
                "/usr/bin/curl -H 'Authorization: Bearer abc' https://api.test",
                "/usr/bin/curl -H 'Authorization: Bearer [REDACTED]' https://api.test",
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
    fn keeps_ordinary_commands() {
        let commands = [
            "/usr/bin/mkdir -p /srv/app",
            "/usr/bin/cp -pr a b",
            "/usr/bin/ssh -p2222 -o PasswordAuthentication=no host",
            "/usr/bin/systemctl restart nginx",
            "/usr/bin/kubectl create secret generic app --from-file=key.pem",
            "/usr/bin/mysql --no-password app",
            "/usr/bin/vault login -token-file /etc/token",
            "/usr/bin/curl https://example.test/path?q=1",
            "/usr/bin/docker run -p 8080:80 nginx",
            "/usr/bin/git clone https://git@github.com/a/b",
        ];

        for command in commands {
            let line = sudo(command);
            assert_eq!(mask_secrets(&line), line, "{command}");
        }
    }

    #[test]
    fn masking_twice_changes_nothing() {
        let once = mask_secrets(&sudo("/usr/bin/app --password s3cret -pX")).into_owned();
        assert_eq!(mask_secrets(&once), once);
    }
}
