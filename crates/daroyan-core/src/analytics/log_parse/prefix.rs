use chrono::{DateTime, NaiveDateTime};
use serde_json::Value;

use super::{
    MONTHS, ParsedLine, SEVERITY_DEBUG, SEVERITY_ERROR, SEVERITY_FATAL, SEVERITY_INFO,
    SEVERITY_WARN, is_digits, is_time, read_level_word, rest_after,
};

pub(super) fn take_iso_timestamp(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(3).collect();
    let first = *tokens.first()?;
    if first.len() >= 19 && first.as_bytes().get(10) == Some(&b'T') {
        if let Ok(time) = DateTime::parse_from_rfc3339(first) {
            parsed.at = Some(time.timestamp_millis());
            return Some(1);
        }
        return NaiveDateTime::parse_from_str(first, "%Y-%m-%dT%H:%M:%S%.f")
            .ok()
            .map(|_| 1);
    }
    let second = *tokens.get(1)?;
    let is_date =
        first.len() == 10 && (first.as_bytes()[4] == b'-' || first.as_bytes()[4] == b'/');
    if !is_date || !is_time(second) {
        return None;
    }
    if let Some(offset_at) = second
        .get(8..)
        .and_then(|tail| tail.find(['+', '-']))
        .map(|index| index + 8)
    {
        let (clock, offset) = second.split_at(offset_at);
        let with_zone = format!(
            "{} {} {}",
            first.replace('/', "-"),
            clock.replace(',', "."),
            offset.replace(':', "")
        );
        if let Ok(time) = DateTime::parse_from_str(&with_zone, "%Y-%m-%d %H:%M:%S%.f %z")
        {
            parsed.at = Some(time.timestamp_millis());
        }
        return Some(2);
    }
    let joined = format!(
        "{} {}",
        first.replace('/', "-"),
        second.trim_end_matches(',').replace(',', ".")
    );
    let zone = tokens.get(2).copied().unwrap_or_default();
    if zone == "UTC" || zone == "Z" {
        if let Ok(time) = NaiveDateTime::parse_from_str(&joined, "%Y-%m-%d %H:%M:%S%.f") {
            parsed.at = Some(time.and_utc().timestamp_millis());
        }
        return Some(3);
    }
    let is_offset = zone.len() >= 5
        && (zone.starts_with('+') || zone.starts_with('-'))
        && zone[1..].bytes().all(|byte| byte.is_ascii_digit() || byte == b':');
    if is_offset {
        let with_zone = format!("{joined} {}", zone.replace(':', ""));
        if let Ok(time) = DateTime::parse_from_str(&with_zone, "%Y-%m-%d %H:%M:%S%.f %z")
        {
            parsed.at = Some(time.timestamp_millis());
        }
        return Some(3);
    }
    Some(2)
}

pub(super) fn take_syslog_header(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(5).collect();
    if tokens.len() < 5
        || !MONTHS.contains(&tokens[0])
        || !is_digits(tokens[1])
        || !is_time(tokens[2])
    {
        return None;
    }
    let process = tokens[4].strip_suffix(':')?;
    read_process(process, parsed);
    Some(5)
}

pub(super) fn read_process(token: &str, parsed: &mut ParsedLine) {
    match token.split_once('[') {
        Some((name, pid)) => {
            parsed.attributes.insert("process.name".into(), Value::from(name));
            if let Some(pid) =
                pid.strip_suffix(']').and_then(|pid| pid.parse::<u64>().ok())
            {
                parsed.attributes.insert("process.pid".into(), Value::from(pid));
            }
        }
        None => {
            parsed.attributes.insert("process.name".into(), Value::from(token));
        }
    }
}

pub(super) fn take_redis_header(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(6).collect();
    let (pid, role) = tokens.first()?.split_once(':')?;
    if tokens.len() < 6
        || !is_digits(pid)
        || role.len() != 1
        || !MONTHS.contains(&tokens[2])
        || !is_time(tokens[4])
    {
        return None;
    }
    let severity = match tokens[5] {
        "#" => SEVERITY_WARN,
        "." => SEVERITY_DEBUG,
        "*" | "-" => SEVERITY_INFO,
        _ => return None,
    };
    parsed.severity = Some(severity);
    parsed
        .attributes
        .insert("process.pid".into(), Value::from(pid.parse::<u64>().ok()?));
    Some(6)
}

pub(super) fn read_priority(pri: u64) -> u8 {
    match pri % 8 {
        0..=2 => SEVERITY_FATAL,
        3 => SEVERITY_ERROR,
        4 => SEVERITY_WARN,
        5 | 6 => SEVERITY_INFO,
        _ => SEVERITY_DEBUG,
    }
}

pub(super) fn take_priority(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let end = text.strip_prefix('<')?.find('>')? + 1;
    let pri: u64 = text[1..end].parse().ok()?;
    parsed.severity = Some(read_priority(pri));
    Some(end + 1)
}

pub(super) fn take_glog_header(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(4).collect();
    let first = *tokens.first()?;
    let severity = match first.chars().next()? {
        'I' => SEVERITY_INFO,
        'W' => SEVERITY_WARN,
        'E' => SEVERITY_ERROR,
        'F' => SEVERITY_FATAL,
        _ => return None,
    };
    if tokens.len() < 4
        || first.len() != 5
        || !is_digits(&first[1..])
        || !is_time(tokens[1])
        || !is_digits(tokens[2])
    {
        return None;
    }
    let location = tokens[3].strip_suffix(']')?;
    parsed.severity = Some(severity);
    parsed
        .attributes
        .insert("process.pid".into(), Value::from(tokens[2].parse::<u64>().ok()?));
    parsed
        .attributes
        .insert("code.location".into(), Value::from(location));
    Some(4)
}

pub(super) fn take_prefix_token(
    text: &str,
    parsed: &mut ParsedLine,
    after_time: bool,
) -> bool {
    let Some(token) = text.split_whitespace().next() else {
        return false;
    };
    if let Some(inner) = token.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        if is_digits(inner) {
            parsed.attributes.insert(
                "process.pid".into(),
                Value::from(inner.parse::<u64>().unwrap_or_default()),
            );
        } else if let Some(severity) = read_level_word(inner) {
            parsed.severity = Some(severity);
        } else if inner.trim().bytes().all(|byte| byte.is_ascii_digit() || byte == b'.')
            && inner.contains('.')
        {
            return true;
        } else if inner.is_empty() || inner.len() > 40 {
            return false;
        } else {
            parsed.attributes.insert("thread".into(), Value::from(inner));
        }
        return true;
    }
    let word = token.trim_end_matches([':', ',']);
    let is_shouted = word.len() > 2 && word.bytes().all(|byte| byte.is_ascii_uppercase());
    if (is_shouted || after_time)
        && let Some(severity) = read_level_word(word)
    {
        parsed.severity = Some(severity);
        return true;
    }
    if let Some((pid, worker)) = token.trim_end_matches(':').split_once('#')
        && is_digits(pid)
        && is_digits(worker)
    {
        parsed.attributes.insert(
            "process.pid".into(),
            Value::from(pid.parse::<u64>().unwrap_or_default()),
        );
        return true;
    }
    if let Some(pid) = token.strip_prefix('<').and_then(|rest| rest.strip_suffix('>'))
        && pid.split('.').count() == 3
        && pid.split('.').all(is_digits)
    {
        parsed.attributes.insert("erlang.pid".into(), Value::from(pid));
        return true;
    }
    if token.starts_with('*') && is_digits(&token[1..]) {
        return true;
    }
    false
}

pub(super) fn take_logger(text: &str, parsed: &mut ParsedLine) -> bool {
    let mut tokens = text.split_whitespace();
    let (Some(logger), Some("-")) = (tokens.next(), tokens.next()) else {
        return false;
    };
    let is_name = logger.chars().all(|character| {
        character.is_alphanumeric() || matches!(character, '.' | '_' | '$' | ':' | '/')
    });
    if !is_name {
        return false;
    }
    parsed.attributes.insert("logger".into(), Value::from(logger));
    true
}

fn take_clock(text: &str, after_time: bool) -> Option<usize> {
    let token = text.split_whitespace().next()?;
    (!after_time && is_time(token)).then_some(1)
}

pub(super) fn strip_prefixes(text: &str, parsed: &mut ParsedLine) -> String {
    let mut rest = text.trim().to_string();
    let mut after_time = false;
    if let Some(end) = take_priority(&rest, parsed) {
        rest = rest[end..].trim_start().to_string();
    }
    for _ in 0..8 {
        if let Some(count) = take_iso_timestamp(&rest, parsed)
            .or_else(|| take_clock(&rest, after_time))
            .or_else(|| take_syslog_header(&rest, parsed))
            .or_else(|| take_redis_header(&rest, parsed))
            .or_else(|| take_glog_header(&rest, parsed))
        {
            rest = rest_after(&rest, count).to_string();
            after_time = true;
            continue;
        }
        if take_prefix_token(&rest, parsed, after_time) {
            rest = rest_after(&rest, 1).to_string();
            continue;
        }
        if parsed.severity.is_some() && take_logger(&rest, parsed) {
            rest = rest_after(&rest, 2).to_string();
            continue;
        }
        break;
    }
    rest
}

pub(super) fn take_bracket_date<'a>(
    text: &'a str,
    parsed: &mut ParsedLine,
) -> Option<&'a str> {
    let inside = text.strip_prefix('[')?;
    let (date, rest) = inside.split_once("] ")?;
    if let Ok(time) = DateTime::parse_from_rfc3339(date) {
        parsed.at = Some(time.timestamp_millis());
        return Some(rest);
    }
    if let Some(local) = date.strip_suffix(" UTC")
        && let Ok(time) = NaiveDateTime::parse_from_str(local, "%d-%b-%Y %H:%M:%S")
    {
        parsed.at = Some(time.and_utc().timestamp_millis());
        return Some(rest);
    }
    let date = date.replace(',', ".");
    let is_date = |text: &str| {
        BRACKET_DATE_FORMATS
            .iter()
            .any(|format| NaiveDateTime::parse_from_str(text, format).is_ok())
    };
    let is_zoned = date.rsplit_once(' ').is_some_and(|(local, _)| is_date(local));
    (is_date(&date) || is_zoned).then_some(rest)
}

const BRACKET_DATE_FORMATS: [&str; 5] = [
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%d-%b-%Y %H:%M:%S",
    "%d/%b/%Y %H:%M:%S",
    "%a %b %d %H:%M:%S%.f %Y",
];
