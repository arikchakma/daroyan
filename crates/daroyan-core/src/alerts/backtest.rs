use chrono::Utc;
use daroyan_protocol::host::Host;
use duckdb::params;

use crate::alerts::rules::{Condition, Scope, Target, read_log_severity};
use crate::analytics::Analytics;
use crate::analytics::log_lines::{LogMatch, list_busy_windows};
use crate::db::Database;
use crate::error::Result;
use crate::servers::status::get_percent;

const MAX_SAMPLE_GAP_MS: i64 = 60_000;
const MAX_HITS: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct BacktestHit {
    pub server_id: String,
    pub server_name: String,
    pub subject: String,
    pub value: String,
    pub at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    at: i64,
    value: f64,
}

struct MetricRow {
    at: i64,
    cpu: f64,
    mem_used: u64,
    load1: f64,
    disk_used: u64,
    disk_total: u64,
}

pub async fn backtest_rule(
    database: &Database,
    analytics: &Analytics,
    condition: &Condition,
    scope: &Scope,
    for_secs: u32,
    days: i64,
) -> Result<Option<Vec<BacktestHit>>> {
    if matches!(condition, Condition::PortClosed { .. })
        || condition.target() != Target::Server
    {
        return Ok(None);
    }
    let since = Utc::now().timestamp_millis() - days * 86_400_000;
    let for_ms = i64::from(for_secs) * 1000;
    let servers = database.list_servers().await?;
    let mut hits = Vec::new();
    for server in &servers {
        let labels: Vec<String> =
            serde_json::from_value(server.labels.clone()).unwrap_or_default();
        if !scope.includes(&server.id, &labels) {
            continue;
        }
        let mem_total = serde_json::from_value::<Host>(server.host.clone())
            .map(|host| host.mem_total)
            .unwrap_or_default();
        let make_hit = |subject: &str, value: String, at: i64| BacktestHit {
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            subject: subject.to_string(),
            value,
            at,
        };
        if let Condition::Logs { level, count, within_secs, service, text } = condition {
            let matcher = LogMatch {
                min_severity: read_log_severity(level).unwrap_or(17),
                service: service.clone(),
                text: text.clone(),
            };
            let window_ms = i64::from(*within_secs) * 1000;
            let subject = service.as_deref().unwrap_or("Logs");
            for (at, found) in list_busy_windows(
                analytics,
                server.id.clone(),
                matcher,
                since,
                window_ms,
                u64::from(*count),
            )
            .await?
            {
                hits.push(make_hit(subject, format!("{found} {level} lines"), at));
            }
            continue;
        }
        if let Condition::ProcessStopped { name } = condition {
            for (at, process) in read_process_exits(analytics, &server.id, since).await? {
                if name.as_ref().is_none_or(|name| name == &process) {
                    hits.push(make_hit(&process, "Stopped".to_string(), at));
                }
            }
            continue;
        }
        let rows = read_metric_rows(analytics, &server.id, since).await?;
        match condition {
            Condition::Offline => {
                for at in
                    find_gaps(&rows.iter().map(|row| row.at).collect::<Vec<_>>(), for_ms)
                {
                    hits.push(make_hit("Server", "Offline".to_string(), at));
                }
            }
            Condition::Cpu { above } => {
                let samples = to_samples(&rows, |row| row.cpu);
                for (at, peak) in find_runs(&samples, *above, for_ms) {
                    hits.push(make_hit("CPU", format!("{peak:.0}%"), at));
                }
            }
            Condition::Memory { above } => {
                let samples =
                    to_samples(&rows, |row| get_percent(row.mem_used, mem_total));
                for (at, peak) in find_runs(&samples, *above, for_ms) {
                    hits.push(make_hit("Memory", format!("{peak:.0}%"), at));
                }
            }
            Condition::Disk { above } => {
                let samples =
                    to_samples(&rows, |row| get_percent(row.disk_used, row.disk_total));
                for (at, peak) in find_runs(&samples, *above, for_ms) {
                    hits.push(make_hit("Disk", format!("{peak:.0}%"), at));
                }
            }
            Condition::Load { above } => {
                let samples = to_samples(&rows, |row| row.load1);
                for (at, peak) in find_runs(&samples, *above, for_ms) {
                    hits.push(make_hit("Load", format!("{peak:.2}"), at));
                }
            }
            _ => {}
        }
    }
    hits.sort_by_key(|hit| std::cmp::Reverse(hit.at));
    hits.truncate(MAX_HITS);
    Ok(Some(hits))
}

async fn read_metric_rows(
    analytics: &Analytics,
    server_id: &str,
    since: i64,
) -> Result<Vec<MetricRow>> {
    let server_id = server_id.to_string();
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT taken_at, cpu, mem_used, load1, disk_used, disk_total FROM server_states \
                 WHERE server_id = ? AND taken_at >= ? ORDER BY taken_at",
            )?;
            statement
                .query_map(params![server_id, since], |row| {
                    Ok(MetricRow {
                        at: row.get(0)?,
                        cpu: f64::from(row.get::<_, f32>(1)?),
                        mem_used: row.get(2)?,
                        load1: row.get(3)?,
                        disk_used: row.get(4)?,
                        disk_total: row.get(5)?,
                    })
                })?
                .collect()
        })
        .await
}

async fn read_process_exits(
    analytics: &Analytics,
    server_id: &str,
    since: i64,
) -> Result<Vec<(i64, String)>> {
    let server_id = server_id.to_string();
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT occurred_at, data->>'name' FROM server_events \
                 WHERE server_id = ? AND kind = 'process_exited' AND occurred_at >= ? ORDER BY occurred_at",
            )?;
            statement
                .query_map(params![server_id, since], |row| {
                    Ok((row.get(0)?, row.get::<_, Option<String>>(1)?.unwrap_or_default()))
                })?
                .collect()
        })
        .await
}

fn to_samples(rows: &[MetricRow], read: impl Fn(&MetricRow) -> f64) -> Vec<Sample> {
    rows.iter()
        .map(|row| Sample { at: row.at, value: read(row) })
        .collect()
}

fn find_runs(samples: &[Sample], above: f64, for_ms: i64) -> Vec<(i64, f64)> {
    let mut runs = Vec::new();
    let mut current: Option<(Sample, Sample, f64)> = None;
    for sample in samples {
        let is_breach = sample.value > above;
        current = match current {
            Some((first, last, peak))
                if is_breach && sample.at - last.at <= MAX_SAMPLE_GAP_MS =>
            {
                Some((first, *sample, peak.max(sample.value)))
            }
            previous => {
                if let Some((first, last, peak)) = previous
                    && last.at - first.at >= for_ms
                {
                    runs.push((first.at, peak));
                }
                is_breach.then_some((*sample, *sample, sample.value))
            }
        };
    }
    if let Some((first, last, peak)) = current
        && last.at - first.at >= for_ms
    {
        runs.push((first.at, peak));
    }
    runs
}

fn find_gaps(times: &[i64], for_ms: i64) -> Vec<i64> {
    times
        .windows(2)
        .filter(|pair| pair[1] - pair[0] > MAX_SAMPLE_GAP_MS + for_ms)
        .map(|pair| pair[0])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_samples(values: &[(i64, f64)]) -> Vec<Sample> {
        values
            .iter()
            .map(|&(secs, value)| Sample { at: secs * 1000, value })
            .collect()
    }

    #[test]
    fn counts_breaches_that_last_the_window() {
        let samples = make_samples(&[
            (0, 50.0),
            (10, 95.0),
            (20, 97.0),
            (30, 96.0),
            (40, 40.0),
            (50, 99.0),
            (60, 30.0),
        ]);
        assert_eq!(find_runs(&samples, 90.0, 20_000), [(10_000, 97.0)]);
        assert_eq!(find_runs(&samples, 90.0, 0), [(10_000, 97.0), (50_000, 99.0)]);
        assert!(find_runs(&samples, 90.0, 60_000).is_empty());
    }

    #[test]
    fn a_gap_in_the_samples_splits_a_run() {
        let samples = make_samples(&[(0, 95.0), (10, 95.0), (200, 95.0), (210, 95.0)]);
        assert_eq!(find_runs(&samples, 90.0, 10_000), [(0, 95.0), (200_000, 95.0)]);
    }

    #[test]
    fn finds_offline_gaps() {
        assert_eq!(find_gaps(&[0, 10_000, 200_000, 210_000], 60_000), [10_000]);
        assert!(find_gaps(&[0, 10_000, 100_000], 60_000).is_empty());
    }
}
