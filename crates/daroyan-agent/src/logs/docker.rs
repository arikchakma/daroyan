use std::collections::HashMap;
use std::process::Command;

use daroyan_protocol::log::LogStream;

use crate::logs::parse::RawLine;
use crate::service::{find_container_ids, is_permission_error};

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub fn parse_timestamp(text: &str) -> Option<i64> {
    let (date, clock) = text.split_once('T')?;
    let clock = clock.strip_suffix('Z')?;
    let mut date_parts = date.split('-').map(str::parse::<i64>);
    let (year, month, day) =
        (date_parts.next()?.ok()?, date_parts.next()?.ok()?, date_parts.next()?.ok()?);
    let (time, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut time_parts = time.split(':').map(str::parse::<i64>);
    let (hour, minute, second) =
        (time_parts.next()?.ok()?, time_parts.next()?.ok()?, time_parts.next()?.ok()?);
    let digits: String = fraction.chars().take(9).collect();
    let nanos = if digits.is_empty() {
        0
    } else {
        format!("{digits:0<9}").parse::<i64>().ok()?
    };
    let seconds =
        days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(seconds * 1_000_000_000 + nanos)
}

fn run_logs(id: &str, since: Option<&str>) -> Option<(String, String)> {
    let mut args = vec!["logs", "--timestamps"];
    if let Some(since) = since {
        args.extend(["--since", since]);
    }
    args.push(id);
    let output = Command::new("docker").args(&args).output().ok()?;
    let output = if !output.status.success()
        && is_permission_error(&String::from_utf8_lossy(&output.stderr))
    {
        Command::new("sudo")
            .arg("-n")
            .arg("docker")
            .args(&args)
            .output()
            .ok()?
    } else {
        output
    };
    output.status.success().then(|| {
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    })
}

fn read_container(id: &str, since: i64) -> Vec<(i64, RawLine)> {
    let since_arg = (since > 0).then(|| {
        format!(
            "{}.{:09}",
            since.div_euclid(1_000_000_000),
            since.rem_euclid(1_000_000_000)
        )
    });
    let Some((stdout, stderr)) = run_logs(id, since_arg.as_deref()) else {
        return Vec::new();
    };
    [(stdout, LogStream::Stdout), (stderr, LogStream::Stderr)]
        .into_iter()
        .flat_map(|(text, stream)| {
            text.lines()
                .filter_map(|line| {
                    let (stamp, message) = line.split_once(' ').unwrap_or((line, ""));
                    let nanos = parse_timestamp(stamp)?;
                    (nanos > since).then(|| {
                        let at = u64::try_from(nanos / 1_000_000).unwrap_or_default();
                        (
                            nanos,
                            RawLine {
                                at,
                                stream,
                                text: message.to_string(),
                                severity: None,
                            },
                        )
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn read_docker(
    pattern: &str,
    positions: &mut Option<HashMap<String, i64>>,
    now_nanos: i64,
) -> Vec<RawLine> {
    let ids = find_container_ids(pattern);
    let Some(known) = positions else {
        *positions = Some(ids.into_iter().map(|id| (id, now_nanos)).collect());
        return Vec::new();
    };
    known.retain(|id, _| ids.contains(id));
    let mut lines = Vec::new();
    for id in ids {
        let since = known.get(&id).copied().unwrap_or(0);
        let read = read_container(&id, since);
        if let Some(last) = read.iter().map(|(nanos, _)| *nanos).max() {
            known.insert(id, last);
        } else {
            known.entry(id).or_insert(since);
        }
        lines.extend(read);
    }
    lines.sort_by_key(|(nanos, _)| *nanos);
    lines.into_iter().map(|(_, line)| line).collect()
}

#[cfg(test)]
mod tests {
    use super::parse_timestamp;

    #[test]
    fn parses_docker_timestamps() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:01Z"), Some(1_000_000_000));
        assert_eq!(
            parse_timestamp("2026-10-16T01:02:03.5Z"),
            Some(1_792_112_523_500_000_000)
        );
        assert_eq!(
            parse_timestamp("2026-10-16T01:02:03.123456789Z").map(|n| n % 1_000_000_000),
            Some(123_456_789)
        );
        assert_eq!(parse_timestamp("not a time"), None);
    }
}
