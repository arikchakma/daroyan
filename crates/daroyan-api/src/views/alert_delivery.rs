use chrono::{DateTime, Utc};
use daroyan_core::db::entities::alert_deliveries::Model as AlertDelivery;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = AlertDelivery)]
pub struct AlertDeliveryView {
    pub id: String,
    pub alert_id: Option<String>,
    pub integration_id: String,
    pub integration_name: String,
    pub message: String,
    #[schema(value_type = String, example = "sent")]
    pub status: String,
    pub error: Option<String>,
    #[serde(with = "daroyan_core::time")]
    pub sent_at: DateTime<Utc>,
}

impl From<AlertDelivery> for AlertDeliveryView {
    fn from(delivery: AlertDelivery) -> Self {
        Self {
            id: delivery.id,
            alert_id: delivery.alert_id,
            integration_id: delivery.integration_id,
            integration_name: delivery.integration_name,
            message: delivery.message,
            status: delivery.status,
            error: delivery.error,
            sent_at: delivery.sent_at,
        }
    }
}
