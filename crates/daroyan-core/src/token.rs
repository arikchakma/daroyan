use rand::Rng;
use rand::distr::Alphanumeric;
use sha2::{Digest, Sha256};

pub const INSTALL_TOKEN_PREFIX: &str = "dy_agent";
pub const SERVER_TOKEN_PREFIX: &str = "dy_server";
pub const INGEST_TOKEN_PREFIX: &str = "dy_ingest";
pub const MCP_TOKEN_PREFIX: &str = "dy_mcp";

pub fn generate_secret(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

pub fn generate_token(prefix: &str) -> String {
    format!("{prefix}_{}", generate_secret(32))
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
