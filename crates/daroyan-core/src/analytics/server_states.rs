use chrono::Utc;
use daroyan_protocol::report::State;
use duckdb::{Connection, params};
use serde::Serialize;
use utoipa::ToSchema;

use crate::analytics::{Analytics, clamp_future_ms};
use crate::db::ids::ServerId;
use crate::error::Result;

pub struct StateRow {
    server_id: String,
    taken_at: i64,
    uptime: u64,
    cpu: f32,
    load1: f64,
    load5: f64,
    load15: f64,
    mem_used: u64,
    swap_used: u64,
    process_count: u32,
    tcp_conn_count: u32,
    udp_conn_count: u32,
    disk_total: u64,
    disk_used: u64,
    disk_read_speed: u64,
    disk_write_speed: u64,
    net_in_speed: u64,
    net_out_speed: u64,
    net_in_transfer: u64,
    net_out_transfer: u64,
}

impl StateRow {
    pub fn from_state(server_id: &str, state: &State) -> Self {
        Self {
            server_id: server_id.to_string(),
            taken_at: clamp_future_ms(
                i64::try_from(state.taken_at).unwrap_or(i64::MAX),
                Utc::now().timestamp_millis(),
            ),
            uptime: state.uptime,
            cpu: state.cpu,
            load1: state.load1,
            load5: state.load5,
            load15: state.load15,
            mem_used: state.mem_used,
            swap_used: state.swap_used,
            process_count: state.process_count,
            tcp_conn_count: state.tcp_conn_count,
            udp_conn_count: state.udp_conn_count,
            disk_total: state
                .disks
                .iter()
                .map(|disk| disk.disk_total)
                .fold(0, u64::saturating_add),
            disk_used: state
                .disks
                .iter()
                .map(|disk| disk.disk_used)
                .fold(0, u64::saturating_add),
            disk_read_speed: state
                .disks
                .iter()
                .map(|disk| disk.read_speed)
                .fold(0, u64::saturating_add),
            disk_write_speed: state
                .disks
                .iter()
                .map(|disk| disk.write_speed)
                .fold(0, u64::saturating_add),
            net_in_speed: state
                .nics
                .iter()
                .map(|nic| nic.net_in_speed)
                .fold(0, u64::saturating_add),
            net_out_speed: state
                .nics
                .iter()
                .map(|nic| nic.net_out_speed)
                .fold(0, u64::saturating_add),
            net_in_transfer: state
                .nics
                .iter()
                .map(|nic| nic.net_in_transfer)
                .fold(0, u64::saturating_add),
            net_out_transfer: state
                .nics
                .iter()
                .map(|nic| nic.net_out_transfer)
                .fold(0, u64::saturating_add),
        }
    }
}

pub struct DiskRow {
    server_id: String,
    taken_at: i64,
    mount: String,
    filesystem: String,
    disk_total: u64,
    disk_used: u64,
}

pub fn to_disk_rows(server_id: &str, state: &State) -> Vec<DiskRow> {
    let taken_at = clamp_future_ms(
        i64::try_from(state.taken_at).unwrap_or(i64::MAX),
        Utc::now().timestamp_millis(),
    );

    state
        .disks
        .iter()
        .map(|disk| DiskRow {
            server_id: server_id.to_string(),
            taken_at,
            mount: disk.mount.clone(),
            filesystem: disk.filesystem.clone(),
            disk_total: disk.disk_total,
            disk_used: disk.disk_used,
        })
        .collect()
}

pub fn append_disks(conn: &Connection, rows: &[DiskRow]) -> duckdb::Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut appender = conn.appender("disk_states")?;
    for row in rows {
        appender.append_row(params![
            row.server_id,
            row.taken_at,
            row.mount,
            row.filesystem,
            row.disk_total,
            row.disk_used,
        ])?;
    }

    appender.flush()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiskTrend {
    pub bytes_per_hour: f64,
    pub window_ms: i64,
}

pub async fn fit_disk_trend(
    analytics: &Analytics,
    server_id: String,
    mount: String,
    since: i64,
) -> Result<Option<DiskTrend>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT regr_slope(disk_used, taken_at), count(*), max(taken_at) - min(taken_at) \
                 FROM disk_states WHERE server_id = ? AND mount = ? AND taken_at >= ?",
            )?;

            let (slope, count, window_ms): (Option<f64>, u64, Option<i64>) = statement
                .query_row(params![server_id, mount, since], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;

            Ok(slope.zip(window_ms).filter(|_| count >= 2).map(|(slope, window_ms)| {
                DiskTrend { bytes_per_hour: slope * 3_600_000.0, window_ms }
            }))
        })
        .await
}

pub fn append_states(conn: &Connection, rows: &[StateRow]) -> duckdb::Result<()> {
    let mut appender = conn.appender("server_states")?;
    for row in rows {
        appender.append_row(params![
            row.server_id,
            row.taken_at,
            row.uptime,
            row.cpu,
            row.load1,
            row.load5,
            row.load15,
            row.mem_used,
            row.swap_used,
            row.process_count,
            row.tcp_conn_count,
            row.udp_conn_count,
            row.disk_total,
            row.disk_used,
            row.disk_read_speed,
            row.disk_write_speed,
            row.net_in_speed,
            row.net_out_speed,
            row.net_in_transfer,
            row.net_out_transfer,
        ])?;
    }
    appender.flush()
}

#[derive(Serialize, ToSchema, Debug)]
pub struct MetricPoint {
    pub at: i64,
    pub cpu: f64,
    pub mem_used: f64,
    pub swap_used: f64,
    pub load1: f64,
    pub disk_read_speed: f64,
    pub disk_write_speed: f64,
    pub net_in_speed: f64,
    pub net_out_speed: f64,
    pub tcp_conn_count: f64,
    pub udp_conn_count: f64,
}

pub async fn list_metric_points(
    analytics: &Analytics,
    server_id: &ServerId,
    from_ms: i64,
    bucket_ms: i64,
) -> Result<Vec<MetricPoint>> {
    let server_id = server_id.to_string();
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT (taken_at // ?) * ? AS at, avg(cpu), avg(mem_used), avg(swap_used), \
                 avg(load1), avg(disk_read_speed), avg(disk_write_speed), avg(net_in_speed), \
                 avg(net_out_speed), avg(tcp_conn_count), avg(udp_conn_count) \
                 FROM server_states WHERE server_id = ? AND taken_at >= ? \
                 GROUP BY 1 ORDER BY 1",
            )?;
            statement
                .query_map(params![bucket_ms, bucket_ms, server_id, from_ms], |row| {
                    Ok(MetricPoint {
                        at: row.get(0)?,
                        cpu: row.get(1)?,
                        mem_used: row.get(2)?,
                        swap_used: row.get(3)?,
                        load1: row.get(4)?,
                        disk_read_speed: row.get(5)?,
                        disk_write_speed: row.get(6)?,
                        net_in_speed: row.get(7)?,
                        net_out_speed: row.get(8)?,
                        tcp_conn_count: row.get(9)?,
                        udp_conn_count: row.get(10)?,
                    })
                })?
                .collect()
        })
        .await
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = TrendPoint)]
pub struct TrendPoint {
    pub at: i64,
    pub value: f64,
}

pub async fn list_fleet_cpu(
    analytics: &Analytics,
    from_ms: i64,
    bucket_ms: i64,
) -> Result<Vec<TrendPoint>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT (taken_at // ?) * ? AS at, avg(cpu) FROM server_states \
                 WHERE taken_at >= ? GROUP BY 1 ORDER BY 1",
            )?;
            statement
                .query_map(params![bucket_ms, bucket_ms, from_ms], |row| {
                    Ok(TrendPoint { at: row.get(0)?, value: row.get(1)? })
                })?
                .collect()
        })
        .await
}

pub async fn aggregate_server_metric(
    analytics: &Analytics,
    expression: String,
    since: i64,
) -> Result<std::collections::HashMap<String, f64>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(&format!(
                "SELECT server_id, {expression} FROM server_states WHERE taken_at >= ? GROUP BY server_id"
            ))?;
            statement
                .query_map([since], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?)))?
                .filter_map(|row| match row {
                    Ok((id, Some(value))) => Some(Ok((id, value))),
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect()
        })
        .await
}
