use daroyan_protocol::host::Host;

use crate::db::Database;
use crate::db::entities::servers::Model as Server;
use crate::db::queries::servers::NewServer;
use crate::error::{Error, ErrorName, Result};
use crate::id::{Prefix, new_id};
use crate::token::{SERVER_TOKEN_PREFIX, generate_token, hash_token};

pub struct EnrolledServer {
    pub server: Server,
    pub token: String,
}

pub async fn enroll_server(
    database: &Database,
    install_token: &str,
    host: &Host,
) -> Result<EnrolledServer> {
    if !database.claim_install_token(&hash_token(install_token)).await? {
        return Err(Error::new(
            ErrorName::Unauthorized,
            "The install token is not valid, has expired or has been used up",
        ));
    }

    let token = generate_token(SERVER_TOKEN_PREFIX);
    let server = database
        .create_server(NewServer {
            id: new_id(Prefix::Server),
            name: host.name.clone(),
            labels: serde_json::to_value(&host.labels)?,
            host: serde_json::to_value(host)?,
            token_hash: hash_token(&token),
        })
        .await?;
    Ok(EnrolledServer { server, token })
}
