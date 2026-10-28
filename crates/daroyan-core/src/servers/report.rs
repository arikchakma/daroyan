use daroyan_protocol::report::{PortState, ProcessState, Report, State, TopProcess};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::analytics::Analytics;
use crate::db::Database;
use crate::db::ids::ServerId;
use crate::db::queries::servers::ServerReport;
use crate::error::Result;

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct LastReport {
    pub state: State,
    pub processes: Vec<ProcessState>,
    pub ports: Vec<PortState>,
    #[serde(default)]
    pub top: Vec<TopProcess>,
}

pub async fn save_report(
    database: &Database,
    analytics: &Analytics,
    server_id: &ServerId,
    report: Report,
) -> Result<()> {
    analytics.buffer_report(server_id.as_str(), &report);

    let host = report.host.as_ref();
    let last_report = LastReport {
        state: report.state,
        processes: report.processes,
        ports: report.ports,
        top: report.top,
    };
    database
        .update_server_report(
            server_id,
            ServerReport {
                labels: host
                    .map(|host| serde_json::to_value(&host.labels))
                    .transpose()?,
                host: host.map(serde_json::to_value).transpose()?,
                last_report: serde_json::to_value(&last_report)?,
            },
        )
        .await?;
    Ok(())
}
