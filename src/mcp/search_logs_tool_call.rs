use std::{collections::BTreeMap, sync::Arc, time::Duration};

use my_ai_agent::{macros::ApplyJsonSchema, ToolDefinition};
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::{app::AppContext, repo::dto::LogLevelDto};

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SearchLogsInputData {
    #[property(description: "Free text phrase. Searched across message, process and context values. Empty or omitted means no phrase filter.")]
    pub phrase: Option<String>,

    #[property(description: "Search in last N minutes from now. When greater than zero overrides from_time and to_time. Use this for queries like find errors in the last hour where N is 60.")]
    pub last_minutes: Option<i64>,

    #[property(description: "Range start as unix microseconds. Used only when last_minutes is omitted. Must be paired with to_time.")]
    pub from_time: Option<i64>,

    #[property(description: "Range end as unix microseconds. Used only when last_minutes is omitted. Must be paired with from_time.")]
    pub to_time: Option<i64>,

    #[property(description: "Filter by log levels. Allowed values are info warning error fatal_error debug. Empty or omitted means all levels.")]
    pub levels: Option<Vec<String>>,

    #[property(description: "Exact equality filters over context. Each entry is a string in the form key value separated by an equal sign. Case insensitive matching. Example Application billing.")]
    pub context_filters: Option<Vec<String>>,

    #[property(description: "Maximum number of records to return. Default 100. Range 1 to 1000.")]
    pub take: Option<i64>,
}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SearchLogsResponse {
    #[property(description: "Matching log records as JSON array string sorted by timestamp descending")]
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
    const DESCRIPTION: &'static str = "Search log records. Combines optional phrase, level filter, key value context filters and time range. Time range can be given as last_minutes or as explicit from_time and to_time microseconds. Returns log records sorted by timestamp descending capped by take.";
}

#[async_trait::async_trait]
impl McpToolCall<SearchLogsInputData, SearchLogsResponse> for SearchLogsHandler {
    async fn execute_tool_call(
        &self,
        model: SearchLogsInputData,
    ) -> Result<SearchLogsResponse, String> {
        let (from_us, to_us) = resolve_range(
            model.last_minutes,
            model.from_time,
            model.to_time,
        )?;

        let take = model.take.unwrap_or(100).clamp(1, 1000) as usize;

        let levels = parse_levels(model.levels.as_deref())?;
        let context = parse_context(model.context_filters.as_deref())?;
        let phrase_owned = model
            .phrase
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());

        let from_dt = DateTimeAsMicroseconds::new(from_us);
        let to_dt = DateTimeAsMicroseconds::new(to_us);

        let items = self
            .app
            .logs_repo
            .search(
                from_dt,
                to_dt,
                levels,
                context,
                phrase_owned.as_deref(),
                take,
            )
            .await;

        let mapped: Vec<serde_json::Value> = items
            .into_iter()
            .map(|i| {
                serde_json::json!({
                    "timestamp": i.moment.unix_microseconds,
                    "iso_time": i.moment.to_rfc3339(),
                    "level": format!("{:?}", i.level),
                    "message": i.message,
                    "context": i.context,
                })
            })
            .collect();

        let items_json = serde_json::to_string(&mapped).unwrap_or_else(|_| "[]".to_string());
        Ok(SearchLogsResponse { items_json })
    }
}

fn resolve_range(
    last_minutes: Option<i64>,
    from_time: Option<i64>,
    to_time: Option<i64>,
) -> Result<(i64, i64), String> {
    if let Some(minutes) = last_minutes {
        if minutes > 0 {
            let now = DateTimeAsMicroseconds::now();
            let from = now.sub(Duration::from_secs((minutes as u64) * 60));
            return Ok((from.unix_microseconds, now.unix_microseconds));
        }
    }

    match (from_time, to_time) {
        (Some(from), Some(to)) if from > 0 && to > 0 => {
            if from >= to {
                Err("from_time must be less than to_time".to_string())
            } else {
                Ok((from, to))
            }
        }
        _ => Err(
            "Specify either last_minutes or both from_time and to_time as unix microseconds"
                .to_string(),
        ),
    }
}

fn parse_levels(levels: Option<&[String]>) -> Result<Option<Vec<LogLevelDto>>, String> {
    let arr = match levels {
        Some(a) if !a.is_empty() => a,
        _ => return Ok(None),
    };
    let mut out = Vec::with_capacity(arr.len());
    for raw in arr {
        let s = raw.trim().to_ascii_lowercase();
        let lvl = match s.as_str() {
            "info" => LogLevelDto::Info,
            "warning" => LogLevelDto::Warning,
            "error" => LogLevelDto::Error,
            "fatal_error" | "fatalerror" | "fatal" => LogLevelDto::FatalError,
            "debug" => LogLevelDto::Debug,
            other => {
                return Err(format!(
                    "Unknown level {}. Allowed info warning error fatal_error debug",
                    other
                ));
            }
        };
        out.push(lvl);
    }
    Ok(Some(out))
}

fn parse_context(
    filters: Option<&[String]>,
) -> Result<Option<BTreeMap<String, String>>, String> {
    let arr = match filters {
        Some(a) if !a.is_empty() => a,
        _ => return Ok(None),
    };
    let mut map = BTreeMap::new();
    for raw in arr {
        let mut split = raw.splitn(2, '=');
        let key = split.next().unwrap_or("").trim();
        let value = match split.next() {
            Some(v) => v.trim(),
            None => {
                return Err(format!(
                    "context_filters entry must be in key=value form, got: {}",
                    raw
                ));
            }
        };
        if key.is_empty() {
            continue;
        }
        map.insert(key.to_string(), value.to_string());
    }
    if map.is_empty() {
        Ok(None)
    } else {
        Ok(Some(map))
    }
}
