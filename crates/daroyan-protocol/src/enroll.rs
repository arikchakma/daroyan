use serde::{Deserialize, Serialize};

use crate::host::Host;

#[derive(Serialize, Deserialize, Debug)]
pub struct EnrollRequest {
    pub host: Host,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EnrollResponse {
    pub server_id: String,
    pub token: String,
}
