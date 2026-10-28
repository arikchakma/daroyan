use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json, Router};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use daroyan_core::error::{Error, ErrorName};
use daroyan_core::token::MCP_TOKEN_PREFIX;
use serde_json::{Map, Value, json};
use tower::ServiceExt;

use crate::AppState;
use crate::extract::{CurrentUser, McpCall, read_bearer};
use crate::layers::client_ip::ClientIp;

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_RESULT_CHARS: usize = 100_000;
const MAX_SCHEMA_DEPTH: usize = 12;

const TIME_ARGS: [&str; 3] = ["since", "until", "at"];
const BAD_CRON_STATUSES: [&str; 3] = ["failed", "missed", "late"];
const OVERVIEW_ISSUES: usize = 10;

const INSTRUCTIONS: &str = "Daroyan monitors servers through an agent, uptime checks, crons, alerts, incidents, and OpenTelemetry traces, issues and logs. Times in results are ISO 8601 in UTC. The since, until and at inputs take a duration back from now like 15m, 1h or 7d, an ISO 8601 time, or Unix milliseconds. Start with get_overview to see what is broken right now, then narrow down with search_logs, list_issues and get_trace. Read a check, cron or rule before you replace it, because update_uptime_check and update_cron replace every field. Confirm with the user before you delete anything or run a server command.";

struct ToolDef {
    name: &'static str,
    method: Method,
    path: &'static str,
    description: &'static str,
}

macro_rules! tool {
    ($name:literal, $method:ident, $path:literal, $description:literal) => {
        ToolDef {
            name: $name,
            method: Method::$method,
            path: $path,
            description: $description,
        }
    };
}

fn list_tool_defs() -> Vec<ToolDef> {
    vec![
        tool!(
            "list_servers",
            GET,
            "/v1/servers",
            "List servers with status (healthy, warning, down or pending), CPU, memory and disk percent, uptime and last seen."
        ),
        tool!(
            "get_server",
            GET,
            "/v1/servers/{server_id}",
            "Get one server: host info and its last report with processes, ports and disks."
        ),
        tool!(
            "get_server_metrics",
            GET,
            "/v1/servers/{server_id}/metrics",
            "CPU, memory, disk and load over time for one server, about 120 averaged points."
        ),
        tool!(
            "list_server_events",
            GET,
            "/v1/servers/{server_id}/events",
            "The last 50 events of a server, like processes exiting or the agent reconnecting."
        ),
        tool!(
            "list_events",
            GET,
            "/v1/overview/events",
            "Everything that happened across the fleet in the last 24 hours, newest first: alerts firing and resolving, uptime checks down and up, crons failing and recovering, processes exiting and restarting, agents offline and online."
        ),
        tool!(
            "create_install_command",
            POST,
            "/v1/install-tokens",
            "Create an install command to add servers. Run the returned command as root on each server. name labels the key. expires_in_hours defaults to 24 (up to 2160); null never expires. max_uses defaults to 1 (up to 100); null is unlimited."
        ),
        tool!(
            "list_install_tokens",
            GET,
            "/v1/install-tokens",
            "List install keys that can still enroll servers, with their expiry, uses and last use."
        ),
        tool!(
            "revoke_install_token",
            DELETE,
            "/v1/install-tokens/{token_id}",
            "Revoke an install token so no more servers can enroll with it."
        ),
        tool!(
            "delete_server",
            DELETE,
            "/v1/servers/{server_id}",
            "Remove a server with its metrics, events and logs. The agent on it stops reporting. Cannot be undone."
        ),
        tool!(
            "run_server_command",
            POST,
            "/v1/servers/{server_id}/commands",
            "Start, stop or restart a process on a server through its agent. Returns a command id; read the result with get_server_command."
        ),
        tool!(
            "get_server_command",
            GET,
            "/v1/servers/{server_id}/commands/{command_id}",
            "The status of a server command: sent, then succeeded or failed with the agent's error."
        ),
        tool!(
            "search_logs",
            GET,
            "/v1/logs",
            "Search log lines from servers and OpenTelemetry, newest first. Page with offset while keeping until the same."
        ),
        tool!(
            "get_log_context",
            GET,
            "/v1/logs/context",
            "The log lines just before and after one line, from the same server and source."
        ),
        tool!(
            "list_log_sources",
            GET,
            "/v1/logs/sources",
            "The server and source pairs that have log lines, to use as search_logs filters."
        ),
        tool!(
            "list_traces",
            GET,
            "/v1/traces",
            "List traces, newest first, with their root span, services, duration and error count."
        ),
        tool!(
            "get_trace",
            GET,
            "/v1/traces/{trace_id}",
            "Every span of one trace with timing, attributes, status and exceptions."
        ),
        tool!(
            "list_trace_services",
            GET,
            "/v1/traces/services",
            "Services that sent spans, with span and error counts."
        ),
        tool!(
            "list_issues",
            GET,
            "/v1/issues",
            "Exceptions grouped by type and message, with counts and when they were first and last seen."
        ),
        tool!(
            "get_issue_events",
            GET,
            "/v1/issues/{fingerprint}/events",
            "Recent occurrences of one issue, with stack traces and their trace ids."
        ),
        tool!(
            "list_alerts",
            GET,
            "/v1/alerts",
            "The last 200 alerts with their rule, server or check, and when they fired and resolved."
        ),
        tool!(
            "list_alert_rules",
            GET,
            "/v1/alert-rules",
            "Alert rules with their condition, scope and integrations."
        ),
        tool!("get_alert_rule", GET, "/v1/alert-rules/{rule_id}", "One alert rule."),
        tool!(
            "list_alert_deliveries",
            GET,
            "/v1/alert-rules/{rule_id}/deliveries",
            "The last 50 messages sent for a rule, with delivery errors."
        ),
        tool!(
            "backtest_alert_rule",
            POST,
            "/v1/alert-rules/backtest",
            "How often a rule would have fired in the last 7 days. Use it before create_alert_rule to pick a threshold."
        ),
        tool!(
            "create_alert_rule",
            POST,
            "/v1/alert-rules",
            "Create an alert rule. Call list_integrations for the integration ids to notify."
        ),
        tool!(
            "update_alert_rule",
            PATCH,
            "/v1/alert-rules/{rule_id}",
            "Change an alert rule. Only the fields you send change."
        ),
        tool!(
            "delete_alert_rule",
            DELETE,
            "/v1/alert-rules/{rule_id}",
            "Delete an alert rule and resolve its alerts without a message."
        ),
        tool!("list_alert_mutes", GET, "/v1/alert-mutes", "Active alert mutes."),
        tool!(
            "create_alert_mute",
            POST,
            "/v1/alert-mutes",
            "Mute a rule, optionally for one server, check or cron, until a time or forever."
        ),
        tool!(
            "delete_alert_mute",
            DELETE,
            "/v1/alert-mutes/{mute_id}",
            "End an alert mute."
        ),
        tool!(
            "list_integrations",
            GET,
            "/v1/integrations",
            "Slack, Discord, Telegram and webhook targets that alerts can notify. Secrets are masked."
        ),
        tool!(
            "list_uptime_checks",
            GET,
            "/v1/uptime-checks",
            "Uptime checks with status, uptime over 30 days and last response time."
        ),
        tool!(
            "get_uptime_check",
            GET,
            "/v1/uptime-checks/{check_id}",
            "One uptime check with all its fields."
        ),
        tool!(
            "get_uptime_check_stats",
            GET,
            "/v1/uptime-checks/{check_id}/stats",
            "Uptime, response times, recent results and outages of a check."
        ),
        tool!(
            "create_uptime_check",
            POST,
            "/v1/uptime-checks",
            "Create an HTTP or TCP uptime check. For http the target is a URL; for tcp it is host:port."
        ),
        tool!(
            "update_uptime_check",
            PUT,
            "/v1/uptime-checks/{check_id}",
            "Replace an uptime check. Send every field, so read it with get_uptime_check first. Set paused to pause it."
        ),
        tool!(
            "run_uptime_check",
            POST,
            "/v1/uptime-checks/{check_id}/run",
            "Run a check now and return the result."
        ),
        tool!(
            "delete_uptime_check",
            DELETE,
            "/v1/uptime-checks/{check_id}",
            "Delete an uptime check and its history."
        ),
        tool!(
            "list_crons",
            GET,
            "/v1/crons",
            "Crons with status, schedule, last run and next expected run."
        ),
        tool!(
            "get_cron",
            GET,
            "/v1/crons/{cron_id}",
            "One cron with its ping URL and all its fields."
        ),
        tool!(
            "get_cron_stats",
            GET,
            "/v1/crons/{cron_id}/stats",
            "Recent runs, durations and failures of a cron."
        ),
        tool!(
            "preview_cron_schedule",
            GET,
            "/v1/crons/preview",
            "Check a cron expression and list its next run times."
        ),
        tool!(
            "create_cron",
            POST,
            "/v1/crons",
            "Create a cron monitor. The job pings the returned URL each time it runs."
        ),
        tool!(
            "update_cron",
            PUT,
            "/v1/crons/{cron_id}",
            "Replace a cron. Send every field, so read it with get_cron first. Set paused to pause it."
        ),
        tool!(
            "delete_cron",
            DELETE,
            "/v1/crons/{cron_id}",
            "Delete a cron and its history."
        ),
        tool!(
            "list_incidents",
            GET,
            "/v1/incidents",
            "Incidents and maintenance windows, newest first."
        ),
        tool!(
            "get_incident",
            GET,
            "/v1/incidents/{incident_id}",
            "One incident with its components and status pages."
        ),
        tool!(
            "list_incident_updates",
            GET,
            "/v1/incidents/{incident_id}/updates",
            "The updates posted on an incident."
        ),
        tool!(
            "create_incident",
            POST,
            "/v1/incidents",
            "Open an incident or schedule maintenance."
        ),
        tool!(
            "post_incident_update",
            POST,
            "/v1/incidents/{incident_id}/updates",
            "Post an update on an incident and change its status. Resolving it ends the incident."
        ),
        tool!(
            "update_incident",
            PUT,
            "/v1/incidents/{incident_id}",
            "Replace an incident's details. Read it with get_incident first."
        ),
        tool!(
            "delete_incident",
            DELETE,
            "/v1/incidents/{incident_id}",
            "Delete an incident and its updates."
        ),
    ]
}

enum BodyShape {
    None,
    Fields,
    Whole,
}

struct Tool {
    def: ToolDef,
    path_names: Vec<String>,
    query_names: Vec<String>,
    body: BodyShape,
    listing: Value,
}

pub struct Mcp {
    api: Router,
    app_url: String,
    tools: Vec<Tool>,
    overview: Value,
}

impl Mcp {
    pub fn new(api: Router, spec: &Value, app_url: Option<&str>) -> Self {
        let app_url = app_url.unwrap_or("https://your-daroyan").trim_end_matches('/');
        let components = &spec["components"]["schemas"];
        let tools = list_tool_defs()
            .into_iter()
            .filter_map(|def| {
                let operation =
                    &spec["paths"][def.path][def.method.as_str().to_lowercase()];
                if operation.is_null() {
                    tracing::error!(
                        tool = def.name,
                        "[mcp] the route is not in the spec"
                    );
                    return None;
                }
                Some(build_tool(def, operation, components))
            })
            .collect();
        let overview = json!({
            "name": "get_overview",
            "description": "What needs attention right now, like the overview dashboard: open incidents, firing alerts that are not muted, servers that are down or warning, failing uptime checks and crons, and exceptions from the last 24 hours. Start here when something seems wrong.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true, "idempotentHint": true, "openWorldHint": false },
        });
        Self { api, app_url: app_url.to_owned(), tools, overview }
    }
}

fn inline_refs(value: &Value, components: &Value, depth: usize) -> Value {
    if depth > MAX_SCHEMA_DEPTH {
        return json!({});
    }
    match value {
        Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|path| path.strip_prefix("#/components/schemas/"))
            {
                return inline_refs(&components[name], components, depth + 1);
            }
            Value::Object(
                object
                    .iter()
                    .map(|(key, item)| {
                        (key.clone(), inline_refs(item, components, depth))
                    })
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| inline_refs(item, components, depth))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn merge_all_of(schema: Value) -> Value {
    let Some(parts) = schema["allOf"].as_array() else { return schema };
    if !parts.iter().all(|part| part["properties"].is_object()) {
        return schema;
    }
    let mut properties = Map::new();
    let mut required = Vec::new();
    for part in parts {
        properties.extend(part["properties"].as_object().cloned().unwrap_or_default());
        required.extend(part["required"].as_array().cloned().unwrap_or_default());
    }
    json!({ "type": "object", "properties": properties, "required": required })
}

fn build_tool(def: ToolDef, operation: &Value, components: &Value) -> Tool {
    let mut properties = Map::new();
    let mut required = Vec::new();
    let mut path_names = Vec::new();
    let mut query_names = Vec::new();

    for parameter in operation["parameters"].as_array().into_iter().flatten() {
        let Some(name) = parameter["name"].as_str() else { continue };
        let mut schema = inline_refs(&parameter["schema"], components, 0);
        if let Some(description) = parameter["description"].as_str() {
            schema["description"] = json!(description);
        }
        properties.insert(name.to_owned(), schema);
        if parameter["required"].as_bool() == Some(true) {
            required.push(json!(name));
        }
        match parameter["in"].as_str() {
            Some("path") => path_names.push(name.to_owned()),
            Some("query") => query_names.push(name.to_owned()),
            _ => {}
        }
    }

    for name in TIME_ARGS {
        if let Some(schema) = properties.get_mut(name)
            && schema["type"] == "integer"
        {
            schema["type"] = json!(["integer", "string"]);
            let description = schema["description"].as_str().unwrap_or("A time");
            schema["description"] = json!(format!(
                "{description}. Takes a duration back from now like 15m, 1h or 7d, an ISO 8601 time, or Unix ms"
            ));
            schema.as_object_mut().map(|schema| schema.remove("format"));
        }
    }

    let body_schema = &operation["requestBody"]["content"]["application/json"]["schema"];
    let body = if body_schema.is_null() {
        BodyShape::None
    } else {
        let schema = merge_all_of(inline_refs(body_schema, components, 0));
        match schema["properties"].as_object() {
            Some(fields) => {
                properties.extend(fields.clone());
                required
                    .extend(schema["required"].as_array().cloned().unwrap_or_default());
                BodyShape::Fields
            }
            None => {
                properties.insert("body".to_owned(), schema);
                required.push(json!("body"));
                BodyShape::Whole
            }
        }
    };

    let is_command = def.path.ends_with("/commands");
    let annotations = json!({
        "readOnlyHint": def.method == Method::GET,
        "destructiveHint": def.method == Method::DELETE || is_command || def.method == Method::PUT,
        "idempotentHint": matches!(def.method, Method::GET | Method::PUT | Method::DELETE),
        "openWorldHint": false,
    });
    let listing = json!({
        "name": def.name,
        "description": def.description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
        "annotations": annotations,
    });

    Tool { def, path_names, query_names, body, listing }
}

fn parse_duration(text: &str) -> Option<Duration> {
    let (number, unit) = text.split_at(text.len().checked_sub(1)?);
    let number = number.parse::<i64>().ok()?;
    match unit {
        "s" => Duration::try_seconds(number),
        "m" => Duration::try_minutes(number),
        "h" => Duration::try_hours(number),
        "d" => Duration::try_days(number),
        "w" => Duration::try_weeks(number),
        _ => None,
    }
}

fn parse_time(text: &str, now: DateTime<Utc>) -> Option<i64> {
    let text = text.trim();
    if let Ok(ms) = text.parse::<i64>() {
        return Some(ms);
    }
    if let Some(duration) = parse_duration(text) {
        return Some((now - duration).timestamp_millis());
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn read_time_args(args: &mut Map<String, Value>) -> Result<(), String> {
    let now = Utc::now();
    for name in TIME_ARGS {
        let Some(Value::String(text)) = args.get(name) else { continue };
        let ms = parse_time(text, now).ok_or(format!(
            "{name} takes a duration like 1h or 7d, an ISO 8601 time, or Unix ms"
        ))?;
        args.insert(name.to_owned(), json!(ms));
    }
    Ok(())
}

fn to_iso(number: &Value) -> Option<Value> {
    let number = number.as_i64()?;
    let time = if number >= 100_000_000_000_000 {
        DateTime::from_timestamp_micros(number)
    } else if number >= 100_000_000_000 {
        DateTime::from_timestamp_millis(number)
    } else {
        None
    }?;
    Some(json!(time.to_rfc3339_opts(SecondsFormat::Millis, true)))
}

fn tidy(value: &mut Value, app_url: &str) {
    match value {
        Value::Object(object) => {
            if let Some(key) = object.get("ping_key").and_then(Value::as_str) {
                let url = format!("{app_url}/p/{key}");
                object.insert("ping_url".to_owned(), json!(url));
            }
            for (key, item) in object.iter_mut() {
                if key == "at" || key.ends_with("_at") {
                    if let Some(time) = to_iso(item) {
                        *item = time;
                    }
                } else if key == "next_runs"
                    && let Value::Array(items) = item
                {
                    for run in items {
                        if let Some(time) = to_iso(run) {
                            *run = time;
                        }
                    }
                } else {
                    tidy(item, app_url);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| tidy(item, app_url)),
        _ => {}
    }
}

fn to_param(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn build_uri(tool: &Tool, args: &Map<String, Value>) -> Result<String, String> {
    let mut path = tool.def.path.to_owned();
    for name in &tool.path_names {
        let value = args.get(name).map(to_param).ok_or(format!("{name} is required"))?;
        let encoded =
            form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>();
        path = path.replace(&format!("{{{name}}}"), &encoded);
    }
    let mut query = form_urlencoded::Serializer::new(String::new());
    for name in &tool.query_names {
        if let Some(value) = args.get(name).filter(|value| !value.is_null()) {
            query.append_pair(name, &to_param(value));
        }
    }
    let query = query.finish();
    Ok(if query.is_empty() { path } else { format!("{path}?{query}") })
}

fn build_body(tool: &Tool, args: Map<String, Value>) -> Option<Value> {
    match tool.body {
        BodyShape::None => None,
        BodyShape::Whole => args.get("body").cloned(),
        BodyShape::Fields => Some(Value::Object(
            args.into_iter()
                .filter(|(key, _)| {
                    !tool.path_names.contains(key) && !tool.query_names.contains(key)
                })
                .collect(),
        )),
    }
}

fn tool_result(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

struct Caller<'a> {
    mcp: &'a Mcp,
    authorization: &'a HeaderValue,
    client_ip: ClientIp,
}

impl Caller<'_> {
    async fn send(
        &self,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> Result<(StatusCode, String), (i64, String)> {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, self.authorization)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
            .map_err(|error| (-32603, error.to_string()))?;
        request.extensions_mut().insert(self.client_ip);
        request.extensions_mut().insert(McpCall);

        let response = self
            .mcp
            .api
            .clone()
            .oneshot(request)
            .await
            .map_err(|error| (-32603, error.to_string()))?;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
            .await
            .map_err(|error| (-32603, error.to_string()))?;
        Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
    }

    async fn get(&self, uri: &str) -> Result<Value, String> {
        match self.send(Method::GET, uri, None).await {
            Ok((status, text)) if status.is_success() => {
                serde_json::from_str(&text).map_err(|error| error.to_string())
            }
            Ok((status, text)) => Err(format!("GET {uri} failed with {status}: {text}")),
            Err((_, message)) => Err(message),
        }
    }

    async fn overview(&self) -> Result<Value, String> {
        let since = (Utc::now() - Duration::days(1)).timestamp_millis();
        let issues_uri = format!("/v1/issues?since={since}");
        let (alerts, mutes, incidents, servers, checks, crons, issues) = tokio::try_join!(
            self.get("/v1/alerts?status=firing"),
            self.get("/v1/alert-mutes"),
            self.get("/v1/incidents"),
            self.get("/v1/servers"),
            self.get("/v1/uptime-checks"),
            self.get("/v1/crons"),
            self.get(&issues_uri),
        )?;
        Ok(build_overview(
            &alerts, &mutes, &incidents, &servers, &checks, &crons, &issues,
        ))
    }
}

fn items(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn pick(item: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|key| {
                item.get(*key)
                    .filter(|value| !value.is_null())
                    .map(|value| ((*key).to_owned(), value.clone()))
            })
            .collect(),
    )
}

fn alert_target(alert: &Value) -> Option<&str> {
    ["server_id", "check_id", "cron_id"]
        .into_iter()
        .find_map(|key| alert[key].as_str())
}

fn is_muted(alert: &Value, mutes: &Value) -> bool {
    let target = alert_target(alert);
    items(mutes).iter().any(|mute| {
        let targets = ["server_id", "check_id", "cron_id"].map(|key| mute[key].as_str());
        mute["rule_id"] == alert["rule_id"]
            && (targets.iter().all(Option::is_none)
                || targets.iter().any(|id| id.is_some() && *id == target))
    })
}

fn build_overview(
    alerts: &Value,
    mutes: &Value,
    incidents: &Value,
    servers: &Value,
    checks: &Value,
    crons: &Value,
    issues: &Value,
) -> Value {
    let firing: Vec<&Value> =
        items(alerts).iter().filter(|alert| !is_muted(alert, mutes)).collect();
    let alerted: Vec<&str> =
        firing.iter().filter_map(|alert| alert_target(alert)).collect();
    let not_alerted =
        |item: &&Value| item["id"].as_str().is_none_or(|id| !alerted.contains(&id));

    let open_incidents: Vec<Value> = items(incidents)
        .iter()
        .filter(|incident| {
            incident["kind"] != "maintenance"
                && !matches!(incident["status"].as_str(), Some("resolved" | "completed"))
        })
        .map(|incident| {
            pick(incident, &["id", "title", "status", "severity", "started_at"])
        })
        .collect();
    let firing_alerts: Vec<Value> = firing
        .iter()
        .map(|alert| {
            pick(
                alert,
                &[
                    "id",
                    "rule_id",
                    "rule_name",
                    "target_name",
                    "severity",
                    "value",
                    "started_at",
                    "server_id",
                    "check_id",
                    "cron_id",
                ],
            )
        })
        .collect();
    let bad_servers: Vec<Value> = items(servers)
        .iter()
        .filter(|server| matches!(server["status"].as_str(), Some("down" | "warning")))
        .filter(not_alerted)
        .map(|server| {
            pick(
                server,
                &[
                    "id",
                    "name",
                    "status",
                    "cpu",
                    "mem_percent",
                    "disk_percent",
                    "last_seen_at",
                ],
            )
        })
        .collect();
    let active_checks: Vec<&Value> =
        items(checks).iter().filter(|check| check["paused"] != true).collect();
    let bad_checks: Vec<Value> = active_checks
        .iter()
        .copied()
        .filter(|check| matches!(check["status"].as_str(), Some("down" | "degraded")))
        .filter(not_alerted)
        .map(|check| {
            pick(
                check,
                &[
                    "id",
                    "name",
                    "target",
                    "status",
                    "last_response_ms",
                    "last_checked_at",
                ],
            )
        })
        .collect();
    let active_crons: Vec<&Value> =
        items(crons).iter().filter(|cron| cron["paused"] != true).collect();
    let bad_crons: Vec<Value> = active_crons
        .iter()
        .copied()
        .filter(|cron| {
            cron["status"]
                .as_str()
                .is_some_and(|status| BAD_CRON_STATUSES.contains(&status))
        })
        .filter(not_alerted)
        .map(|cron| {
            pick(cron, &["id", "name", "status", "server_name", "next_expected_at"])
        })
        .collect();
    let mut recent_issues: Vec<&Value> = items(issues).iter().collect();
    recent_issues.sort_by_key(|issue| std::cmp::Reverse(issue["last_seen_at"].as_i64()));
    let recent_issues: Vec<Value> = recent_issues
        .into_iter()
        .take(OVERVIEW_ISSUES)
        .map(|issue| {
            pick(
                issue,
                &[
                    "fingerprint",
                    "service",
                    "kind",
                    "message",
                    "count",
                    "last_seen_at",
                    "trace_id",
                ],
            )
        })
        .collect();

    let needs_attention = !(open_incidents.is_empty()
        && firing_alerts.is_empty()
        && bad_servers.is_empty()
        && bad_checks.is_empty()
        && bad_crons.is_empty());
    json!({
        "needs_attention": needs_attention,
        "watching": {
            "servers": items(servers).len(),
            "uptime_checks": active_checks.len(),
            "crons": active_crons.len(),
            "muted_alerts": items(alerts).len() - firing.len(),
        },
        "open_incidents": open_incidents,
        "firing_alerts": firing_alerts,
        "servers": bad_servers,
        "uptime_checks": bad_checks,
        "crons": bad_crons,
        "recent_issues": recent_issues,
    })
}

async fn call_tool(caller: &Caller<'_>, params: &Value) -> Result<Value, (i64, String)> {
    let mcp = caller.mcp;
    let name = params["name"].as_str().unwrap_or_default();
    if name == "get_overview" {
        return Ok(match caller.overview().await {
            Ok(mut overview) => {
                tidy(&mut overview, &mcp.app_url);
                tool_result(overview.to_string(), false)
            }
            Err(message) => tool_result(message, true),
        });
    }
    let tool = mcp
        .tools
        .iter()
        .find(|tool| tool.def.name == name)
        .ok_or_else(|| (-32602, format!("Unknown tool {name}")))?;
    let mut args = params["arguments"].as_object().cloned().unwrap_or_default();
    if let Err(message) = read_time_args(&mut args) {
        return Ok(tool_result(message, true));
    }

    let uri = match build_uri(tool, &args) {
        Ok(uri) => uri,
        Err(message) => return Ok(tool_result(message, true)),
    };
    let body = build_body(tool, args);
    let (status, mut text) = caller.send(tool.def.method.clone(), &uri, body).await?;

    if text.is_empty() {
        text = if status.is_success() {
            format!("Done ({status})")
        } else {
            status.to_string()
        };
    } else if let Ok(mut value) = serde_json::from_str::<Value>(&text) {
        if name == "create_install_command"
            && let Some(token) = value["token"].as_str()
        {
            let app_url = &mcp.app_url;
            value["command"] = json!(format!(
                "curl -fsSL {app_url}/install.sh | sudo DAROYAN_API_URL={app_url} DAROYAN_TOKEN={token} sh"
            ));
        }
        tidy(&mut value, &mcp.app_url);
        text = value.to_string();
    }
    if text.len() > MAX_RESULT_CHARS {
        let cut = text.floor_char_boundary(MAX_RESULT_CHARS);
        text.truncate(cut);
        text.push_str("\n… truncated. Narrow the query or lower the limit.");
    }
    Ok(tool_result(text, !status.is_success()))
}

fn rpc_result(id: Value, result: Value) -> Response {
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn rpc_error(id: Value, code: i64, message: &str) -> Response {
    Json(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
        .into_response()
}

fn negotiate_version(params: &Value) -> &'static str {
    let asked = params["protocolVersion"].as_str().unwrap_or_default();
    PROTOCOL_VERSIONS
        .into_iter()
        .find(|version| *version == asked)
        .unwrap_or(PROTOCOL_VERSIONS[0])
}

fn unauthorized() -> Response {
    let mut response = Error::new(
        ErrorName::Unauthorized,
        "Send an MCP token as Authorization: Bearer <token>",
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

pub async fn handle_mcp(
    State(state): State<AppState>,
    Extension(mcp): Extension<Arc<Mcp>>,
    request: Request,
) -> Response {
    let (mut parts, body) = request.into_parts();
    if !read_bearer(&parts.headers).is_ok_and(|token| token.starts_with(MCP_TOKEN_PREFIX))
    {
        return unauthorized();
    }
    parts.headers.remove(header::COOKIE);
    parts.extensions.insert(McpCall);
    if let Err(error) = CurrentUser::from_request_parts(&mut parts, &state).await {
        let mut response = error.into_response();
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    }
    let Some(authorization) = parts.headers.get(header::AUTHORIZATION).cloned() else {
        return unauthorized();
    };
    let Some(client_ip) = parts.extensions.get::<ClientIp>().copied() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let Ok(bytes) = to_bytes(body, MAX_REQUEST_BYTES).await else {
        return rpc_error(Value::Null, -32700, "The body is too large or unreadable");
    };
    let Ok(message) = serde_json::from_slice::<Value>(&bytes) else {
        return rpc_error(Value::Null, -32700, "Parse error: send one JSON-RPC message");
    };
    let Some(id) = message.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    if message["jsonrpc"] != "2.0" || !message["method"].is_string() {
        return rpc_error(id, -32600, "Invalid request: send one JSON-RPC 2.0 message");
    }

    let params = &message["params"];
    match message["method"].as_str().unwrap_or_default() {
        "initialize" => rpc_result(
            id,
            json!({
                "protocolVersion": negotiate_version(params),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "daroyan", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }),
        ),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(
            id,
            json!({
                "tools": std::iter::once(&mcp.overview)
                    .chain(mcp.tools.iter().map(|tool| &tool.listing))
                    .collect::<Vec<_>>()
            }),
        ),
        "tools/call" => match call_tool(
            &Caller {
                mcp: &mcp,
                authorization: &authorization,
                client_ip,
            },
            params,
        )
        .await
        {
            Ok(result) => rpc_result(id, result),
            Err((code, message)) => rpc_error(id, code, &message),
        },
        method => rpc_error(id, -32601, &format!("Method not found: {method}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_times_and_shows_them_as_iso() {
        let now = DateTime::parse_from_rfc3339("2026-10-19T12:00:00Z").unwrap().to_utc();
        let hour_ago = now.timestamp_millis() - 3_600_000;
        assert_eq!(parse_time("1h", now), Some(hour_ago));
        assert_eq!(parse_time(" 2026-10-19T11:00:00Z ", now), Some(hour_ago));
        assert_eq!(parse_time("1792000000000", now), Some(1_792_000_000_000));
        assert_eq!(parse_time("later", now), None);
        assert_eq!(parse_time("h", now), None);

        let mut value = json!({
            "at": 1_792_000_000_123_i64,
            "spans": [{ "started_at": 1_792_000_000_123_456_i64, "duration_us": 23_560 }],
            "next_runs": [1_792_000_000_000_i64],
            "uptime": 21_917,
            "ping_key": "abc",
        });
        tidy(&mut value, "https://d.example");
        assert_eq!(value["at"], "2026-10-14T17:46:40.123Z");
        assert_eq!(value["spans"][0]["started_at"], "2026-10-14T17:46:40.123Z");
        assert_eq!(value["spans"][0]["duration_us"], 23_560);
        assert_eq!(value["next_runs"][0], "2026-10-14T17:46:40.000Z");
        assert_eq!(value["uptime"], 21_917);
        assert_eq!(value["ping_url"], "https://d.example/p/abc");
    }
}
