use chrono::Utc;
use daroyan_protocol::report::State;
use duckdb::{Connection, OptionalExt, params};
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

pub async fn find_disk_used_before(
    analytics: &Analytics,
    server_id: String,
    before: i64,
) -> Result<Option<u64>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT disk_used FROM server_states \
                 WHERE server_id = ? AND taken_at <= ? ORDER BY taken_at DESC LIMIT 1",
            )?;
            statement
                .query_row(params![server_id, before], |row| row.get(0))
                .optional()
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
