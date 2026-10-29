mod app;
mod database;
mod http;
mod json;
mod php;
mod prefix;
mod system;

use daroyan_protocol::log::LogFormat;
use serde_json::{Map, Value};

use app::{
    read_elasticsearch, read_flask, read_level_and_logger, read_python,
    read_python_dashes, read_python_traceback, read_rails, read_spring,
};
use database::{read_mysql, read_mysql_slow_query, read_postgres_duration};
use http::{
    read_apache_error, read_haproxy, read_nginx_access, read_quoted_request, read_request,
};
pub(crate) use json::read_level_value;
pub use json::{normalize_json_line, take_json_time};
use json::{read_fields, split_logfmt};
use php::{read_laravel, read_php_error, read_php_fpm};
use prefix::{strip_prefixes, take_bracket_date};
use system::{
    read_cron, read_fail2ban, read_oom_kill, read_ssh_failure, read_sudo, read_ufw,
};

const SEVERITY_DEBUG: u8 = 5;
const SEVERITY_INFO: u8 = 9;
const SEVERITY_WARN: u8 = 13;
const SEVERITY_ERROR: u8 = 17;
const SEVERITY_FATAL: u8 = 21;
const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ParsedLine {
    pub at: Option<i64>,
    pub severity: Option<u8>,
    pub message: String,
    pub attributes: Map<String, Value>,
}

pub(crate) fn read_level_word(word: &str) -> Option<u8> {
    match word.to_ascii_lowercase().as_str() {
        "fatal" | "panic" | "crit" | "critical" | "emerg" | "alert" => {
            Some(SEVERITY_FATAL)
        }
        "error" | "err" => Some(SEVERITY_ERROR),
        "warn" | "warning" => Some(SEVERITY_WARN),
        "info" | "notice" | "log" | "statement" => Some(SEVERITY_INFO),
        "debug" | "trace" => Some(SEVERITY_DEBUG),
        _ => None,
    }
}

fn is_time(token: &str) -> bool {
    let clock = token
        .trim_end_matches(',')
        .split(['.', ','])
        .next()
        .unwrap_or_default();
    clock.len() == 8 && clock.as_bytes()[2] == b':' && clock.as_bytes()[5] == b':'
}

fn is_digits(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(|byte| byte.is_ascii_digit())
}

fn rest_after(text: &str, count: usize) -> &str {
    let mut rest = text.trim_start();
    for _ in 0..count {
        rest = rest
            .find(char::is_whitespace)
            .map_or("", |end| rest[end..].trim_start());
    }
    rest
}

type Reader = fn(&str, &mut ParsedLine) -> Option<String>;

const BLOCK_READERS: [(&[LogFormat], Reader); 2] = [
    (&[LogFormat::Mysql], read_mysql_slow_query),
    (&[LogFormat::Python], read_python_traceback),
];

const LINE_READERS: [(&[LogFormat], Reader); 3] = [
    (&[LogFormat::Rails], read_rails),
    (&[LogFormat::Python], read_python),
    (&[LogFormat::Elasticsearch], read_elasticsearch),
];

const BRACKET_READERS: [(&[LogFormat], Reader); 7] = [
    (&[LogFormat::Laravel], read_laravel),
    (&[LogFormat::Php], read_php_error),
    (&[LogFormat::PhpFpm], read_php_fpm),
    (&[LogFormat::Apache], read_apache_error),
    (&[LogFormat::Envoy, LogFormat::Python], read_quoted_request),
    (&[LogFormat::Python], read_flask),
    (&[LogFormat::Kafka], read_level_and_logger),
];

const BODY_READERS: [(&[LogFormat], Reader); 7] = [
    (&[LogFormat::Nginx, LogFormat::Apache, LogFormat::Traefik], read_nginx_access),
    (&[LogFormat::Mysql], read_mysql),
    (&[LogFormat::Haproxy], read_haproxy),
    (&[LogFormat::Java], read_spring),
    (&[LogFormat::Python], read_python_dashes),
    (&[LogFormat::System], read_fail2ban),
    (&[], read_request),
];

const PREFIX_FORMATS: [LogFormat; 8] = [
    LogFormat::Nginx,
    LogFormat::Java,
    LogFormat::Mysql,
    LogFormat::Postgresql,
    LogFormat::Mongodb,
    LogFormat::Redis,
    LogFormat::Rabbitmq,
    LogFormat::System,
];

fn read_with(
    readers: &[(&[LogFormat], Reader)],
    format: LogFormat,
    text: &str,
    parsed: &mut ParsedLine,
) -> Option<(String, LogFormat)> {
    let before = parsed.clone();
    readers.iter().find_map(|(formats, reader)| {
        if !format.is_auto() && !formats.contains(&format) {
            return None;
        }
        let Some(message) = reader(text, parsed) else {
            parsed.clone_from(&before);
            return None;
        };
        let found = match formats {
            [only] => *only,
            _ => LogFormat::Auto,
        };
        Some((message, found))
    })
}

fn label(
    mut parsed: ParsedLine,
    format: LogFormat,
    found: Option<LogFormat>,
) -> ParsedLine {
    let name = if format.is_auto() { found.unwrap_or_default() } else { format };
    if !name.is_auto() {
        parsed.attributes.insert("log.format".into(), name.as_str().into());
    }
    if !format.is_auto() && found.is_none() {
        parsed.attributes.insert("log.parse_failed".into(), true.into());
    }
    parsed
}

pub fn parse_text_line(text: &str, format: LogFormat) -> ParsedLine {
    if format == LogFormat::Plain {
        let parsed = ParsedLine { message: text.to_string(), ..ParsedLine::default() };
        return label(parsed, format, Some(format));
    }
    let first_line = text.lines().next().unwrap_or_default();
    if matches!(format, LogFormat::Auto | LogFormat::Logfmt | LogFormat::Traefik)
        && let Some(fields) = split_logfmt(first_line)
    {
        return label(read_fields(fields, first_line), format, Some(LogFormat::Logfmt));
    }
    let mut parsed = ParsedLine::default();
    if let Some((message, found)) = read_with(&BLOCK_READERS, format, text, &mut parsed) {
        parsed.message = message;
        return label(parsed, format, Some(found));
    }
    if let Some((message, found)) =
        read_with(&LINE_READERS, format, first_line, &mut parsed)
            .or_else(|| read_bracketed_line(first_line, format, &mut parsed))
    {
        return with_continuation(text, message, label(parsed, format, Some(found)));
    }
    let mut body = strip_prefixes(first_line, &mut parsed);
    let stream_tag = body.split_whitespace().take(2).collect::<Vec<_>>();
    if matches!(stream_tag.as_slice(), ["stdout" | "stderr", "F" | "P"]) {
        body = rest_after(&body, 2).to_string();
    }
    if body.starts_with('{')
        && let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(&body)
    {
        let mut inner = read_fields(fields, &body);
        inner.at = inner.at.or(parsed.at);
        return label(inner, format, Some(LogFormat::Json));
    }
    let matched = read_with(&BODY_READERS, format, &body, &mut parsed);
    let mut found = matched.as_ref().map(|(_, found)| *found);
    let mut message = matched.map_or(body, |(message, _)| message);
    if matches!(format, LogFormat::Auto | LogFormat::System) {
        message = read_ufw(&message, &mut parsed).unwrap_or(message);
        read_ssh_failure(&message, &mut parsed);
        read_sudo(&message, &mut parsed);
        read_cron(&message, &mut parsed);
        read_oom_kill(&message, &mut parsed);
    }
    if matches!(format, LogFormat::Auto | LogFormat::Postgresql) {
        read_postgres_duration(&message, &mut parsed);
    }
    if PREFIX_FORMATS.contains(&format)
        && (parsed.severity.is_some() || parsed.at.is_some())
    {
        found = found.or(Some(format));
    }
    with_continuation(text, message, label(parsed, format, found))
}

fn read_bracketed_line(
    text: &str,
    format: LogFormat,
    parsed: &mut ParsedLine,
) -> Option<(String, LogFormat)> {
    let rest = take_bracket_date(text, parsed)?;
    read_with(&BRACKET_READERS, format, rest, parsed)
}

fn with_continuation(text: &str, message: String, mut parsed: ParsedLine) -> ParsedLine {
    let continuation: Vec<&str> = text.lines().skip(1).collect();
    parsed.message = if continuation.is_empty() {
        message
    } else {
        parsed
            .attributes
            .insert("stack.lines".into(), Value::from(continuation.len()));
        format!("{message}\n{}", continuation.join("\n"))
    };
    parsed
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::log::LogFormat;
    use serde_json::json;

    use super::{ParsedLine, normalize_json_line, parse_text_line};

    pub(super) fn parse(text: &str) -> ParsedLine {
        parse_text_line(text, LogFormat::Auto)
    }

    #[test]
    fn strips_app_prefixes_and_reads_requests() {
        let parsed = parse(
            "2026-10-16T01:42:01.546Z [maily-api] INFO: 209.71.83.120 d9f7326e POST /v1/emails 202 84ms",
        );
        assert_eq!(parsed.message, "209.71.83.120 · d9f7326e");
        assert_eq!(parsed.severity, Some(9));
        assert_eq!(parsed.at, Some(1_792_114_921_546));
        assert_eq!(parsed.attributes["http.method"], "POST");
        assert_eq!(parsed.attributes["url.path"], "/v1/emails");
        assert_eq!(parsed.attributes["http.status"], 202);
        assert_eq!(parsed.attributes["duration_ms"], 84.0);
        assert_eq!(parse("93.184.216.34 GET /v1/links 500 2.1s").severity, Some(17));
        let invalid = parse(
            "10.0.1.21 v36 POST /v1/emails 422 9ms validation failed: to[0] must be a valid email",
        );
        assert_eq!(invalid.severity, Some(13));
        assert_eq!(
            invalid.message,
            "10.0.1.21 · v36 · validation failed: to[0] must be a valid email"
        );
    }

    #[test]
    fn reads_caddy_access_logs() {
        let line = r#"{"level":"info","ts":1793152202.5972965,"logger":"http.log.access","msg":"handled request","request":{"remote_ip":"172.69.159.170","client_ip":"172.69.159.170","method":"GET","host":"daroyan.com","uri":"/v1/servers/srv_1/metrics?range=1h","headers":{"User-Agent":["Mozilla/5.0"]}},"duration":0.003048545,"size":2,"status":200}"#;
        let parsed = parse(line);
        assert_eq!(parsed.message, "GET /v1/servers/srv_1/metrics 200");
        assert_eq!(parsed.at, Some(1_793_152_202_597));
        assert_eq!(parsed.severity, Some(9));
        assert_eq!(parsed.attributes["http.method"], "GET");
        assert_eq!(parsed.attributes["url.path"], "/v1/servers/srv_1/metrics");
        assert_eq!(parsed.attributes["url.query"], "range=1h");
        assert_eq!(parsed.attributes["duration_ms"], 3.049);
        assert_eq!(parsed.attributes["client.ip"], "172.69.159.170");
        assert_eq!(parsed.attributes["user_agent.original"], "Mozilla/5.0");

        let mut attributes = json!({
            "logger": "http.log.access.log0",
            "request": { "method": "POST", "uri": "/v1/otlp/v1/traces" },
            "status": 502
        })
        .as_object()
        .cloned()
        .unwrap();
        let mut message = "handled request".to_string();
        let mut severity = Some(9);
        normalize_json_line(&mut attributes, &mut message, &mut severity);
        assert_eq!(message, "POST /v1/otlp/v1/traces 502");
        assert_eq!(severity, Some(17));

        let mut attributes = json!({ "method": "POST", "path": "/users", "status": 201 })
            .as_object()
            .cloned()
            .unwrap();
        let mut message = "user created".to_string();
        normalize_json_line(&mut attributes, &mut message, &mut None);
        assert_eq!(message, "user created");
    }

    #[test]
    fn reads_laravel_and_php_logs() {
        let laravel = parse(
            "[2026-10-28 01:50:02] production.ERROR: SQLSTATE[HY000] [2002] Connection refused {\"userId\":42,\"exception\":\"[object] (PDOException(code: 2002): SQLSTATE[HY000] [2002] Connection refused at /var/www/vendor/laravel/framework/src/Illuminate/Database/Connectors/Connector.php:66)\n[stacktrace]\n#0 /var/www/app.php(12): connect()\n#1 {main}\n\"} ",
        );
        assert_eq!(laravel.severity, Some(17));
        assert!(
            laravel
                .message
                .starts_with("SQLSTATE[HY000] [2002] Connection refused\n[stacktrace]")
        );
        assert_eq!(laravel.attributes["deployment.environment"], "production");
        assert_eq!(laravel.attributes["exception.type"], "PDOException");
        assert_eq!(
            parse("[2026-10-28 01:50:02] local.INFO: Order 42 shipped {\"order\":42} []")
                .attributes["order"],
            42
        );

        let fatal = parse(
            "[28-Oct-2026 01:50:02 UTC] PHP Fatal error:  Uncaught Error: Call to undefined function foo() in /var/www/app.php:12",
        );
        assert_eq!(fatal.severity, Some(17));
        assert_eq!(fatal.at, Some(1_793_152_202_000));
        assert_eq!(
            fatal.message,
            "Uncaught Error: Call to undefined function foo() in /var/www/app.php:12"
        );
        assert_eq!(fatal.attributes["error.type"], "Fatal error");
        assert_eq!(fatal.attributes["code.filepath"], "/var/www/app.php");
        assert_eq!(fatal.attributes["code.lineno"], 12);
        let warning = parse(
            "[28-Oct-2026 01:50:02 Europe/Berlin] PHP Warning:  Undefined variable $user in /var/www/index.php on line 7",
        );
        assert_eq!(warning.severity, Some(13));
        assert_eq!(warning.attributes["code.lineno"], 7);

        let fpm = parse(
            "[28-Oct-2026 01:50:02] WARNING: [pool www] server reached pm.max_children setting (5), consider raising it",
        );
        assert_eq!(fpm.severity, Some(13));
        assert_eq!(fpm.attributes["php_fpm.pool"], "www");
        assert_eq!(
            fpm.message,
            "server reached pm.max_children setting (5), consider raising it"
        );
    }

    #[test]
    fn reads_mysql_and_postgres_logs() {
        let mysql = parse(
            "2026-10-28T01:50:02.123456Z 8 [Warning] [MY-010055] [Server] IP address '10.0.0.5' could not be resolved",
        );
        assert_eq!(mysql.severity, Some(13));
        assert_eq!(mysql.message, "IP address '10.0.0.5' could not be resolved");
        assert_eq!(mysql.attributes["mysql.error_code"], "MY-010055");
        assert_eq!(mysql.attributes["mysql.subsystem"], "Server");
        let old_mysql = parse(
            "2026-10-28T01:50:02.123456Z 0 [ERROR] InnoDB: Unable to lock ./ibdata1",
        );
        assert_eq!(old_mysql.severity, Some(17));
        assert_eq!(old_mysql.message, "InnoDB: Unable to lock ./ibdata1");

        let slow = parse(
            "2026-10-28 01:50:02.123 UTC [4512] LOG:  duration: 1523.412 ms  statement: SELECT * FROM links",
        );
        assert_eq!(slow.attributes["duration_ms"], 1523.412);
        assert_eq!(slow.attributes["db.statement"], "SELECT * FROM links");
        let prepared = parse(
            "2026-10-28 01:50:02.123 UTC [4512] LOG:  duration: 12.5 ms  execute <unnamed>: SELECT 1",
        );
        assert_eq!(prepared.attributes["db.statement"], "SELECT 1");
    }

    #[test]
    fn reads_nginx_lines() {
        let access = parse(
            r#"93.184.216.34 - - [16/Oct/2026:01:41:57 +0000] "GET /r/abc123 HTTP/1.1" 302 0 "-" "Mozilla/5.0""#,
        );
        assert_eq!(access.message, "GET /r/abc123 302");
        assert_eq!(access.attributes["client.ip"], "93.184.216.34");
        assert_eq!(access.attributes["http.status"], 302);
        let error = parse(
            "2026/10/16 01:41:55 [error] 8312#8312: *4931 connect() failed (111: Connection refused) while connecting to upstream",
        );
        assert_eq!(error.severity, Some(17));
        assert_eq!(error.attributes["process.pid"], 8312);
        assert!(error.message.starts_with("connect() failed"));
    }

    #[test]
    fn reads_system_lines() {
        let ssh = parse(
            "Oct 16 01:41:50 worker-1 sshd[23014]: Failed password for root from 45.155.205.233 port 50122 ssh2",
        );
        assert_eq!(
            ssh.message,
            "Failed password for root from 45.155.205.233 port 50122 ssh2"
        );
        assert_eq!(ssh.severity, Some(13));
        assert_eq!(ssh.attributes["user"], "root");
        assert_eq!(ssh.attributes["client.ip"], "45.155.205.233");
        assert_eq!(ssh.attributes["process.name"], "sshd");
        let cron = parse(
            "Oct 16 01:40:01 worker-1 CRON[22987]: (root) CMD (/usr/local/bin/backup.sh)",
        );
        assert_eq!(cron.message, "(root) CMD (/usr/local/bin/backup.sh)");
        assert_eq!(cron.attributes["process.pid"], 22987);
        let oom = parse(
            "Oct 16 01:39:44 worker-1 kernel: [812345.120] Out of memory: Killed process 4121 (node) total-vm:2148392kB",
        );
        assert_eq!(oom.severity, Some(17));
        assert_eq!(oom.attributes["process.name"], "node");
        let redis =
            parse("77:M 16 Oct 2026 01:39:12.004 * Background saving started by pid 77");
        assert_eq!(redis.message, "Background saving started by pid 77");
        assert_eq!(redis.attributes["process.pid"], 77);
        let postgres = parse(
            r#"2026-10-16 01:38:40.112 UTC [1882] ERROR:  duplicate key value violates unique constraint "links_slug_key""#,
        );
        assert_eq!(postgres.severity, Some(17));
        assert_eq!(postgres.attributes["process.pid"], 1882);
        assert!(postgres.message.starts_with("duplicate key value"));
    }

    #[test]
    fn keeps_plain_lines_and_stack_traces() {
        let plain = parse("something happened while syncing contacts");
        assert_eq!(plain.message, "something happened while syncing contacts");
        assert_eq!(plain.severity, None);
        assert!(plain.attributes.is_empty());
        let mixed_case = parse(
            "Error: read ECONNRESET\n    at TLSSocket.onClose\n    at node:net:337:12",
        );
        assert!(mixed_case.message.starts_with("Error: read ECONNRESET\n"));
        assert_eq!(mixed_case.attributes["stack.lines"], json!(2));
    }

    #[test]
    fn handles_formats_from_gpu_log() {
        let java = parse(
            "2024-01-15 10:30:00,123 [main] INFO  com.example.Foo - Started server on port 8080",
        );
        assert_eq!(java.message, "Started server on port 8080");
        assert_eq!(java.severity, Some(9));
        assert_eq!(java.attributes["thread"], "main");
        assert_eq!(java.attributes["logger"], "com.example.Foo");

        let logfmt =
            parse(r#"ts=2024-01-15T10:30:00Z level=warn msg="disk full" path=/var"#);
        assert_eq!(logfmt.message, "disk full");
        assert_eq!(logfmt.severity, Some(13));
        assert_eq!(logfmt.at, Some(1_705_314_600_000));
        assert_eq!(logfmt.attributes["path"], "/var");
        assert!(!parse("2024-01-15 INFO msg=hi").attributes.contains_key("msg"));

        let cri = parse(
            r#"2024-01-15T10:30:00.123456789Z stdout F {"level":"error","msg":"boom"}"#,
        );
        assert_eq!(cri.message, "boom");
        assert_eq!(cri.severity, Some(17));
        assert_eq!(cri.at, Some(1_705_314_600_123));

        let pino = parse(
            r#"{"level":30,"time":1705314600123,"pid":42,"msg":"hello","reqId":"abc"}"#,
        );
        assert_eq!(pino.message, "hello");
        assert_eq!(pino.severity, Some(9));
        assert_eq!(pino.at, Some(1_705_314_600_123));
        assert_eq!(pino.attributes["reqId"], "abc");

        let pri =
            parse("<134>Jan 15 10:30:00 myhost sshd[1234]: Accepted publickey for alice");
        assert_eq!(pri.severity, Some(9));
        assert_eq!(pri.message, "Accepted publickey for alice");
        assert_eq!(pri.attributes["process.pid"], 1234);
        assert_eq!(parse("<11>app: disk failed").severity, Some(17));

        let glog =
            parse("E0115 10:30:00.123456   4242 server.go:42] connection reset by peer");
        assert_eq!(glog.severity, Some(17));
        assert_eq!(glog.message, "connection reset by peer");
        assert_eq!(glog.attributes["code.location"], "server.go:42");

        let offset = parse("2024-01-15 10:30:00.123 +05:30 job finished");
        assert_eq!(offset.at, Some(1_705_294_800_123));
        assert_eq!(offset.message, "job finished");

        let windows = parse(
            "2016-09-28 04:30:31, Error                 CSI    00000001 WcpInitialize called",
        );
        assert_eq!(windows.severity, Some(17));
        assert!(windows.message.starts_with("CSI"));

        let access = parse(
            r#"10.0.0.1 - - [15/Jan/2024:10:30:00 +0000] "GET /index.html HTTP/1.1" 200 1234 "-" "curl/8.4.0""#,
        );
        assert_eq!(access.attributes["url.path"], "/index.html");
        assert_eq!(access.attributes["http.status"], 200);

        let with_query = parse(
            r#"10.0.0.1 - - [15/Jan/2024:10:30:00 +0000] "GET /roadmap?tab=open&page=2 HTTP/1.1" 200 1234 "-" "curl/8.4.0""#,
        );
        assert_eq!(with_query.attributes["url.path"], "/roadmap");
        assert_eq!(with_query.attributes["url.query"], "tab=open&page=2");

        let plain_dash = parse("deploy - started by alice");
        assert_eq!(plain_dash.message, "deploy - started by alice");
    }

    #[test]
    fn limits_parsing_to_the_configured_format() {
        let nginx = r#"93.184.216.34 - - [16/Oct/2026:01:41:57 +0000] "GET /r/abc123 HTTP/1.1" 302 0 "-" "Mozilla/5.0""#;
        let parsed = parse_text_line(nginx, LogFormat::Nginx);
        assert_eq!(parsed.message, "GET /r/abc123 302");
        assert_eq!(parsed.attributes["log.format"], "nginx");
        assert!(!parsed.attributes.contains_key("log.parse_failed"));
        assert!(!parse(nginx).attributes.contains_key("log.format"));

        let wrong = parse_text_line(nginx, LogFormat::Laravel);
        assert_eq!(wrong.attributes["log.parse_failed"], true);
        assert!(!wrong.attributes.contains_key("http.status"));

        let plain = parse_text_line("level=error msg=boom", LogFormat::Plain);
        assert_eq!(plain.message, "level=error msg=boom");
        assert_eq!(plain.severity, None);
        assert_eq!(plain.attributes["log.format"], "plain");

        let redis = parse_text_line(
            "77:M 16 Oct 2026 01:39:12.004 * Background saving started by pid 77",
            LogFormat::Redis,
        );
        assert!(!redis.attributes.contains_key("log.parse_failed"));
        let not_redis = parse_text_line("hello there", LogFormat::Redis);
        assert_eq!(not_redis.attributes["log.parse_failed"], true);

        let ssh = "Oct 16 01:41:50 worker-1 sshd[23014]: Failed password for root from 45.155.205.233 port 50122 ssh2";
        assert!(
            !parse_text_line(ssh, LogFormat::Postgresql)
                .attributes
                .contains_key("user")
        );
        assert_eq!(parse_text_line(ssh, LogFormat::System).attributes["user"], "root");
    }

    #[test]
    fn reads_requests_after_level_words() {
        let python = parse("INFO: 93.184.216.34 GET /v1/links 500 2.1s");
        assert_eq!(python.severity, Some(17));
        assert_eq!(python.attributes["http.status"], 500);
        let kafka = parse("[2026-10-28 01:50:02,123] ERROR GET /x 503 4ms");
        assert_eq!(kafka.attributes["url.path"], "/x");
        assert_eq!(kafka.attributes["http.status"], 503);
    }

    #[test]
    fn drops_what_a_failed_reader_set() {
        let parsed = parse_text_line("ERROR:not a python line", LogFormat::Python);
        assert_eq!(parsed.severity, None);
        assert_eq!(parsed.attributes["log.parse_failed"], true);
    }
}
