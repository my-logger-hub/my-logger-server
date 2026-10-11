use std::{collections::BTreeMap, sync::Arc};

use mcp_server_middleware::{ApplyJsonSchema, ToolDefinition};
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::{
    app::{AppContext, PROCESS_CONTEXT_KEY},
    logs_storage::SearchDirection,
    repo::dto::{LogItemDto, LogLevelDto},
};

const APPLICATION_KEY: &str = "Application";
const VERSION_KEY: &str = "Version";
const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 1000;

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SearchLogsInputData {
    #[property(description: "Range start, ISO-8601 / RFC-3339 in UTC. Example: 2026-04-28T10:00:00Z. Inclusive.")]
    pub from_date: String,

    #[property(description: "Range end, ISO-8601 / RFC-3339 in UTC. Example: 2026-04-28T11:00:00Z. Inclusive.")]
    pub to_date: String,

    #[property(description: "Optional. The product scope to search - see get_product_scopes. If omitted, the default scope.")]
    pub product_scope: Option<String>,

    #[property(description: "Optional. Filter by application name. Matches the 'Application' context key. Case-insensitive.")]
    pub application: Option<String>,

    #[property(description: "Optional. Filter by application version. Matches the 'Version' context key. Case-insensitive.")]
    pub version: Option<String>,

    #[property(description: "Optional. Case-insensitive substring searched in the message, the process and the context values. Empty or omitted means no phrase filter.")]
    pub phrase: Option<String>,

    #[property(
        enum: ["Info", "Warning", "Error", "FatalError", "Debug"],
        description: "Optional. Filter by log levels. If omitted or empty, all levels match. Pass any subset, e.g. [\"Error\", \"FatalError\"]."
    )]
    pub levels: Option<Vec<String>>,

    #[property(description: "Maximum number of records to return. Default 100. Range 1 to 1000.")]
    pub take: Option<i64>,
}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SearchLogsResponse {
    #[property(description: "Number of records returned.")]
    pub count: i64,

    #[property(description: "True if `take` records were found before `from_date` was reached: there may be earlier ones. To read on, ask again with `to_date` just before the first returned record.")]
    pub truncated: bool,

    #[property(description: "Matching log records as JSON array string sorted by timestamp ascending.")]
    pub items_json: String,
}

pub struct SearchLogsHandler {
    app: Arc<AppContext>,
}

impl SearchLogsHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for SearchLogsHandler {
    const FUNC_NAME: &'static str = "search_logs";
    const DESCRIPTION: &'static str = "Search log records of a product scope by time range with optional application, version, level and substring phrase filters. Returns the latest matching records, at most `take` of them, sorted by timestamp ascending.";
}

#[async_trait::async_trait]
impl McpToolCall<SearchLogsInputData, SearchLogsResponse> for SearchLogsHandler {
    async fn execute_tool_call(
        &self,
        model: SearchLogsInputData,
    ) -> Result<SearchLogsResponse, String> {
        let from_dt = parse_iso_date(&model.from_date, "from_date")?;
        let to_dt = parse_iso_date(&model.to_date, "to_date")?;

        if to_dt.unix_microseconds <= from_dt.unix_microseconds {
            return Err("`to_date` must be strictly greater than `from_date`".to_string());
        }

        let product_scope = self
            .app
            .logs_storage
            .product_scopes()
            .requested(model.product_scope.as_deref())?;

        let take = model.take.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;

        let mut context = BTreeMap::new();
        if let Some(app) = trimmed(model.application.as_deref()) {
            context.insert(APPLICATION_KEY.to_string(), app.to_string());
        }
        if let Some(ver) = trimmed(model.version.as_deref()) {
            context.insert(VERSION_KEY.to_string(), ver.to_string());
        }
        let context = if context.is_empty() {
            None
        } else {
            Some(context)
        };

        let phrase_owned = model
            .phrase
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());

        let levels = match model.levels {
            Some(v) if !v.is_empty() => {
                let parsed: Result<Vec<_>, _> =
                    v.iter().map(|s| parse_level(s.as_str())).collect();
                Some(parsed?)
            }
            _ => None,
        };

        let mut items = crate::flows::search_logs(
            &self.app,
            &product_scope,
            from_dt,
            to_dt,
            levels,
            context,
            phrase_owned.as_deref(),
            take,
            SearchDirection::Down,
        )
        .await;

        items.sort_by_key(|i| i.moment.unix_microseconds);

        let truncated = items.len() >= take;
        let count = items.len() as i64;

        let mapped: Vec<serde_json::Value> = items.into_iter().map(record_to_json).collect();
        let items_json = serde_json::to_string(&mapped).unwrap_or_else(|_| "[]".to_string());

        Ok(SearchLogsResponse {
            count,
            truncated,
            items_json,
        })
    }
}

fn parse_iso_date(value: &str, field: &str) -> Result<DateTimeAsMicroseconds, String> {
    DateTimeAsMicroseconds::parse_iso_string(value).ok_or_else(|| {
        format!(
            "`{}` is not a valid ISO-8601 / RFC-3339 datetime: '{}'",
            field, value
        )
    })
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn parse_level(value: &str) -> Result<LogLevelDto, String> {
    match value {
        "Info" => Ok(LogLevelDto::Info),
        "Warning" => Ok(LogLevelDto::Warning),
        "Error" => Ok(LogLevelDto::Error),
        "FatalError" => Ok(LogLevelDto::FatalError),
        "Debug" => Ok(LogLevelDto::Debug),
        other => Err(format!(
            "invalid log level '{}'. Allowed: Info, Warning, Error, FatalError, Debug",
            other
        )),
    }
}

fn level_to_str(level: &LogLevelDto) -> &'static str {
    match level {
        LogLevelDto::Info => "Info",
        LogLevelDto::Warning => "Warning",
        LogLevelDto::Error => "Error",
        LogLevelDto::FatalError => "FatalError",
        LogLevelDto::Debug => "Debug",
    }
}

fn record_to_json(mut item: LogItemDto) -> serde_json::Value {
    let application = item.context.remove(APPLICATION_KEY);
    let version = item.context.remove(VERSION_KEY);
    let process = item.context.remove(PROCESS_CONTEXT_KEY);

    serde_json::json!({
        "iso_time": item.moment.to_rfc3339(),
        "timestamp": item.moment.unix_microseconds,
        "level": level_to_str(&item.level),
        "application": application,
        "version": version,
        "process": process,
        "message": item.message,
        "context": item.context,
    })
}
