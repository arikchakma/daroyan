use serde::{Deserialize, Serialize};

use crate::log::LogBatch;
use crate::report::Report;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    Report(Report),
    CommandResult(CommandResult),
    Logs(LogBatch),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApiMessage {
    Command(Command),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Command {
    pub id: String,
    #[serde(flatten)]
    pub action: CommandAction,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CommandAction {
    StartProcess { name: String },
    StopProcess { name: String },
    RestartProcess { name: String },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CommandResult {
    pub id: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_wire_format() {
        let text = r#"{"type":"logs","lines":[{"at":1,"service":"Linkity API","stream":"stderr","severity":17,"message":"boom"}]}"#;
        let AgentMessage::Logs(batch) = serde_json::from_str(text).unwrap() else {
            panic!("expected logs");
        };
        assert_eq!(batch.dropped, 0);
        assert_eq!(batch.lines[0].stream, crate::log::LogStream::Stderr);
        assert!(batch.lines[0].attributes.is_empty());
    }

    #[test]
    fn command_wire_format() {
        let text = r#"{"type":"command","id":"c1","action":"restart_process","name":"inboxing-worker"}"#;
        let message: ApiMessage = serde_json::from_str(text).unwrap();
        assert_eq!(
            message,
            ApiMessage::Command(Command {
                id: "c1".to_string(),
                action: CommandAction::RestartProcess {
                    name: "inboxing-worker".to_string()
                },
            })
        );
    }
}
