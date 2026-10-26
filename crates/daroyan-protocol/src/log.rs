use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SEVERITY_DEBUG: u8 = 5;
pub const SEVERITY_INFO: u8 = 9;
pub const SEVERITY_WARN: u8 = 13;
pub const SEVERITY_ERROR: u8 = 17;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LogBatch {
    pub lines: Vec<LogLine>,
    #[serde(default)]
    pub dropped: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LogLine {
    pub at: u64,
    pub service: String,
    pub stream: LogStream,
    pub severity: u8,
    pub message: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attributes: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
    Journal,
    File,
}

impl LogStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Journal => "journal",
            Self::File => "file",
        }
    }
}
