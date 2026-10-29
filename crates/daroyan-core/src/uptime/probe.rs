use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDateTime, Utc};
use http_body_util::{BodyExt, Empty, Limited};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper::header::{HOST, LOCATION, USER_AGENT};
use hyper_util::rt::TokioIo;
use reqwest::Url;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::db::entities::uptime_checks::Model as UptimeCheck;
use crate::net::resolve_reachable;

pub const STATUS_UP: &str = "up";
pub const STATUS_DOWN: &str = "down";
pub const STATUS_CONFIRMING: &str = "confirming";

const MAX_REDIRECTS: usize = 5;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const USER_AGENT_VALUE: &str = concat!("Daroyan-Uptime/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Default)]
pub struct Timings {
    pub dns_ms: Option<u32>,
    pub connect_ms: Option<u32>,
    pub tls_ms: Option<u32>,
    pub ttfb_ms: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Probe {
    pub at: DateTime<Utc>,
    pub status: &'static str,
    pub code: Option<u16>,
    pub response_ms: Option<u32>,
    pub error: Option<String>,
    pub cert_expires_at: Option<DateTime<Utc>>,
    pub timings: Timings,
}

pub fn build_tls_config() -> Arc<ClientConfig> {
    let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let mut config = ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("ring supports the default TLS versions")
    .with_root_certificates(roots)
    .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

fn to_ms(since: Instant) -> u32 {
    u32::try_from(since.elapsed().as_millis()).unwrap_or(u32::MAX)
}

fn add(total: Option<u32>, value: u32) -> Option<u32> {
    Some(total.unwrap_or_default().saturating_add(value))
}

pub async fn run_probe(tls: &Arc<ClientConfig>, check: &UptimeCheck) -> Probe {
    let at = Utc::now();
    let started = Instant::now();
    let timeout = Duration::from_secs(u64::try_from(check.timeout_secs).unwrap_or(10));
    let mut timings = Timings::default();
    let outcome = tokio::time::timeout(timeout, async {
        match check.kind.as_str() {
            "tcp" => probe_tcp(&check.target, &mut timings).await,
            _ => probe_http(tls, check, &mut timings).await,
        }
    })
    .await
    .unwrap_or_else(|_| {
        Err(Failure::new(format!("Timed out after {}s", timeout.as_secs())))
    });
    let mut probe = match outcome {
        Ok(passed) => Probe {
            at,
            status: STATUS_UP,
            code: passed.code,
            response_ms: Some(to_ms(started)),
            error: None,
            cert_expires_at: passed.cert_expires_at,
            timings: Timings::default(),
        },
        Err(failure) => Probe {
            at,
            status: STATUS_DOWN,
            code: failure.code,
            response_ms: None,
            error: Some(failure.error),
            cert_expires_at: failure.cert_expires_at,
            timings: Timings::default(),
        },
    };
    probe.timings = timings;
    probe
}

#[derive(Default)]
struct Passed {
    code: Option<u16>,
    cert_expires_at: Option<DateTime<Utc>>,
}

struct Failure {
    error: String,
    code: Option<u16>,
    cert_expires_at: Option<DateTime<Utc>>,
}

impl Failure {
    fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: None,
            cert_expires_at: None,
        }
    }
}

async fn connect(
    host: &str,
    port: u16,
    timings: &mut Timings,
) -> Result<TcpStream, Failure> {
    let started = Instant::now();
    let addresses = resolve_reachable(host, port).await.map_err(|error| {
        Failure::new(match error.kind() {
            std::io::ErrorKind::PermissionDenied => error.to_string(),
            _ => format!("Could not resolve {host}"),
        })
    })?;
    timings.dns_ms = add(timings.dns_ms, to_ms(started));
    let started = Instant::now();
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect(address).await {
            Ok(stream) => {
                timings.connect_ms = add(timings.connect_ms, to_ms(started));
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(Failure::new(match last_error {
        Some(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            "Connection refused".to_string()
        }
        Some(error) => format!("Could not connect: {error}"),
        None => format!("Could not resolve {host}"),
    }))
}

async fn probe_tcp(target: &str, timings: &mut Timings) -> Result<Passed, Failure> {
    let (host, port) = target
        .rsplit_once(':')
        .and_then(|(host, port)| Some((host, port.parse().ok()?)))
        .ok_or_else(|| Failure::new("The target is not host:port"))?;
    connect(host.trim_start_matches('[').trim_end_matches(']'), port, timings).await?;
    Ok(Passed::default())
}

struct Reply {
    code: u16,
    reason: String,
    location: Option<String>,
    body: Option<String>,
}

async fn send<S>(
    stream: S,
    method: &hyper::Method,
    url: &Url,
    read_body: bool,
    timings: &mut Timings,
) -> Result<Reply, Failure>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, connection) = http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|error| Failure::new(format!("HTTP handshake failed: {error}")))?;
    tokio::spawn(connection);
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    let path = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    };
    let request = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header(HOST, host)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .body(Empty::<Bytes>::new())
        .map_err(|error| Failure::new(error.to_string()))?;
    let started = Instant::now();
    let response = sender
        .send_request(request)
        .await
        .map_err(|error| Failure::new(format!("The request failed: {error}")))?;
    timings.ttfb_ms = add(timings.ttfb_ms, to_ms(started));
    let code = response.status().as_u16();
    let reason = response.status().canonical_reason().unwrap_or_default().to_string();
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = if read_body {
        let bytes = Limited::new(response.into_body(), MAX_BODY_BYTES)
            .collect()
            .await
            .map_err(|error| Failure::new(format!("Could not read the body: {error}")))?
            .to_bytes();
        Some(String::from_utf8_lossy(&bytes).into_owned())
    } else {
        None
    };
    Ok(Reply { code, reason, location, body })
}

async fn probe_http(
    tls: &Arc<ClientConfig>,
    check: &UptimeCheck,
    timings: &mut Timings,
) -> Result<Passed, Failure> {
    let method =
        hyper::Method::from_bytes(check.method.as_bytes()).unwrap_or(hyper::Method::GET);
    let mut url =
        Url::parse(&check.target).map_err(|_| Failure::new("The URL is not valid"))?;
    let mut cert_expires_at = None;
    for _ in 0..=MAX_REDIRECTS {
        let host = url
            .host_str()
            .ok_or_else(|| Failure::new("The URL has no host"))?
            .to_string();
        let port = url.port_or_known_default().unwrap_or(80);
        let stream =
            connect(host.trim_start_matches('[').trim_end_matches(']'), port, timings)
                .await?;
        let read_body = check.body_contains.is_some();
        let reply = if url.scheme() == "https" {
            let name = ServerName::try_from(
                host.trim_start_matches('[').trim_end_matches(']').to_string(),
            )
            .map_err(|_| Failure::new("The host name is not valid"))?;
            let started = Instant::now();
            let stream = TlsConnector::from(Arc::clone(tls))
                .connect(name, stream)
                .await
                .map_err(|error| Failure::new(format!("TLS failed: {error}")))?;
            timings.tls_ms = add(timings.tls_ms, to_ms(started));
            if cert_expires_at.is_none() {
                cert_expires_at = stream
                    .get_ref()
                    .1
                    .peer_certificates()
                    .and_then(|certs| certs.first())
                    .and_then(|cert| read_cert_expiry(cert));
            }
            send(stream, &method, &url, read_body, timings).await
        } else {
            send(stream, &method, &url, read_body, timings).await
        }
        .map_err(|failure| Failure { cert_expires_at, ..failure })?;

        if (300..400).contains(&reply.code)
            && let Some(next) =
                reply.location.as_deref().and_then(|location| url.join(location).ok())
        {
            url = next;
            continue;
        }
        let fail =
            |error: String| Failure { error, code: Some(reply.code), cert_expires_at };
        let is_expected = match check.expected_status {
            Some(expected) => i32::from(reply.code) == expected,
            None => (200..400).contains(&reply.code),
        };
        if !is_expected {
            return Err(fail(
                format!("{} {}", reply.code, reply.reason).trim().to_string(),
            ));
        }
        if let (Some(text), Some(body)) = (&check.body_contains, &reply.body)
            && !body.contains(text.as_str())
        {
            return Err(fail(format!("The body does not contain \"{text}\"")));
        }
        return Ok(Passed { code: Some(reply.code), cert_expires_at });
    }
    Err(Failure::new(format!("More than {MAX_REDIRECTS} redirects")))
}

fn read_tlv(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = data.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        let len = rest[..count]
            .iter()
            .fold(0, |len, &byte| (len << 8) | usize::from(byte));
        (len, &rest[count..])
    };
    (rest.len() >= len).then(|| (tag, &rest[..len], &rest[len..]))
}

pub fn read_cert_expiry(der: &[u8]) -> Option<DateTime<Utc>> {
    let (_, cert, _) = read_tlv(der)?;
    let (_, mut tbs, _) = read_tlv(cert)?;
    if tbs.first() == Some(&0xa0) {
        tbs = read_tlv(tbs)?.2;
    }
    for _ in 0..3 {
        tbs = read_tlv(tbs)?.2;
    }
    let (_, validity, _) = read_tlv(tbs)?;
    let (_, _, rest) = read_tlv(validity)?;
    let (tag, time, _) = read_tlv(rest)?;
    let text = std::str::from_utf8(time).ok()?;
    let format = if tag == 0x17 { "%y%m%d%H%M%SZ" } else { "%Y%m%d%H%M%SZ" };
    NaiveDateTime::parse_from_str(text, format)
        .ok()
        .map(|time| time.and_utc())
}

#[cfg(test)]
mod tests {
    use super::read_cert_expiry;

    fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        if content.len() < 0x80 {
            out.push(u8::try_from(content.len()).unwrap());
        } else {
            out.push(0x82);
            out.extend(u16::try_from(content.len()).unwrap().to_be_bytes());
        }
        out.extend(content);
        out
    }

    #[test]
    fn reads_the_certificate_expiry() {
        let validity =
            [tlv(0x17, b"260101000000Z"), tlv(0x18, b"20261216120000Z")].concat();
        let tbs = [
            tlv(0xa0, &tlv(0x02, &[2])),
            tlv(0x02, &[1, 2, 3]),
            tlv(0x30, &[0; 10]),
            tlv(0x30, &[0; 200]),
            tlv(0x30, &validity),
            tlv(0x30, &[0; 5]),
        ]
        .concat();
        let cert = tlv(0x30, &[tlv(0x30, &tbs), tlv(0x30, &[0; 3])].concat());
        assert_eq!(
            read_cert_expiry(&cert).unwrap().to_rfc3339(),
            "2026-12-16T12:00:00+00:00"
        );
        assert!(read_cert_expiry(&cert[..20]).is_none());
    }
}
