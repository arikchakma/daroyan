use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;

use anyhow::{Context, Result};
use daroyan_protocol::enroll::{EnrollRequest, EnrollResponse};
use daroyan_protocol::host::Host;
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Serialize, Deserialize)]
pub struct ServerState {
    pub server_id: String,
    pub token: String,
}

pub async fn load_or_enroll(config: &Config, host: &Host) -> Result<ServerState> {
    let path = config.agent.state_path.join("server.json");
    match fs::read_to_string(&path) {
        Ok(text) => {
            return serde_json::from_str(&text)
                .with_context(|| format!("parse {}", path.display()));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("read {}", path.display()));
        }
    }

    let install_token = config
        .api
        .token
        .clone()
        .or_else(|| env::var("DAROYAN_TOKEN").ok())
        .context("no install token: set api.token in the config or DAROYAN_TOKEN")?;
    let response = enroll_server(&config.api.url, &install_token, host)
        .await
        .context("enroll this server")?;
    let state = ServerState {
        server_id: response.server_id,
        token: response.token,
    };

    fs::create_dir_all(&config.agent.state_path)
        .with_context(|| format!("create {}", config.agent.state_path.display()))?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut file| file.write_all(&serde_json::to_vec(&state)?))
        .with_context(|| format!("write {}", path.display()))?;

    Ok(state)
}

async fn enroll_server(
    api_url: &str,
    install_token: &str,
    host: &Host,
) -> Result<EnrollResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("daroyan-agent/", env!("CARGO_PKG_VERSION")))
        .build()?;
    Ok(client
        .post(format!("{api_url}/v1/agent/enroll"))
        .bearer_auth(install_token)
        .json(&EnrollRequest { host: host.clone() })
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}
