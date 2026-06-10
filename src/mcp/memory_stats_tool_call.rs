use std::sync::Arc;

use my_ai_agent::{macros::ApplyJsonSchema, ToolDefinition};
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::app::AppContext;

// ====================== get_memory_stats ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct MemoryStatsInputData {}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct MemoryStatsResponse {
    #[property(description: "Bytes allocated by the application (jemalloc stats.allocated), in MB.")]
    pub jemalloc_allocated_mb: i64,
    #[property(description: "Bytes in active pages (jemalloc stats.active), in MB.")]
    pub jemalloc_active_mb: i64,
    #[property(description: "Physically resident memory as seen by jemalloc (stats.resident), in MB. Closest to the RSS the OS reports.")]
    pub jemalloc_resident_mb: i64,
    #[property(description: "Mapped address space (jemalloc stats.mapped), in MB.")]
    pub jemalloc_mapped_mb: i64,
    #[property(description: "Memory returned to the OS but still mapped (jemalloc stats.retained), in MB. Does not consume physical RAM.")]
    pub jemalloc_retained_mb: i64,
    #[property(description: "jemalloc metadata overhead, in MB.")]
    pub jemalloc_metadata_mb: i64,

    #[property(description: "Items waiting in the in-memory queue of the tantivy (Warning/Error/FatalError) flusher. Capped at 100000; oldest items are dropped on overflow.")]
    pub tantivy_queue_len: i64,
    #[property(description: "Items waiting in the in-memory queue of the sqlite (Debug/Info) flusher. Same 100000 cap.")]
    pub sqlite_queue_len: i64,

    #[property(description: "Tantivy hour indexes currently held open in the pool. Each holds mmap'ed segment files and an index reader.")]
    pub tantivy_open_indexes: i64,
    #[property(description: "Tantivy hour indexes with a live IndexWriter (~15MB heap each). Normally 1-2: the current and the previous hour; idle writers are closed by GC.")]
    pub tantivy_open_writers: i64,
    #[property(description: "Keys of the open tantivy hour indexes (YYYYMMDDHH UTC), as JSON array string.")]
    pub tantivy_open_index_keys_json: String,

    #[property(description: "Open sqlite hour databases, as JSON array string of 'Level:YYYYMMDDHH'. Each holds a page cache.")]
    pub sqlite_open_dbs_json: String,

    #[property(description: "Insights cache: number of cached values per context key, as JSON object string. Capped at 10000 values per key.")]
    pub insights_counts_json: String,

    #[property(description: "Hours currently held in the in-memory hourly statistics (max 48).")]
    pub hourly_statistics_hours: i64,
}

pub struct MemoryStatsHandler {
    app: Arc<AppContext>,
}

impl MemoryStatsHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for MemoryStatsHandler {
    const FUNC_NAME: &'static str = "get_memory_stats";
    const DESCRIPTION: &'static str = "Get memory-management diagnostics of the logger server itself: jemalloc allocator statistics (allocated / active / resident / mapped / retained / metadata) plus the internal state that drives memory usage - flush queue lengths, open tantivy hour-indexes and writers, open sqlite hour-databases, insights cache sizes, hourly-statistics retention. Use it to diagnose memory consumption or to verify that memory is released after load. For a history of allocator readings (sampled every 30s) call `get_debug_console`.";
}

#[async_trait::async_trait]
impl McpToolCall<MemoryStatsInputData, MemoryStatsResponse> for MemoryStatsHandler {
    async fn execute_tool_call(
        &self,
        _model: MemoryStatsInputData,
    ) -> Result<MemoryStatsResponse, String> {
        let jemalloc = crate::app::read_jemalloc_stats();
        let mb = |value: usize| (value / 1024 / 1024) as i64;

        let tantivy_queue_len = self.app.logs_queue.len().await as i64;
        let sqlite_queue_len = self.app.sqlite_logs_queue.len().await as i64;

        let (index_keys, open_writers) = self.app.logs_repo.get_open_indexes().await;
        let sqlite_dbs = self.app.sqlite_logs_repo.get_open_db_keys().await;
        let insights = self.app.insights_repo.get_value_counts().await;

        let hourly_statistics_hours = {
            let read_access = self.app.hourly_statistics.lock().await;
            read_access.hours_count() as i64
        };

        Ok(MemoryStatsResponse {
            jemalloc_allocated_mb: jemalloc.as_ref().map(|s| mb(s.allocated)).unwrap_or(-1),
            jemalloc_active_mb: jemalloc.as_ref().map(|s| mb(s.active)).unwrap_or(-1),
            jemalloc_resident_mb: jemalloc.as_ref().map(|s| mb(s.resident)).unwrap_or(-1),
            jemalloc_mapped_mb: jemalloc.as_ref().map(|s| mb(s.mapped)).unwrap_or(-1),
            jemalloc_retained_mb: jemalloc.as_ref().map(|s| mb(s.retained)).unwrap_or(-1),
            jemalloc_metadata_mb: jemalloc.as_ref().map(|s| mb(s.metadata)).unwrap_or(-1),
            tantivy_queue_len,
            sqlite_queue_len,
            tantivy_open_indexes: index_keys.len() as i64,
            tantivy_open_writers: open_writers as i64,
            tantivy_open_index_keys_json: serde_json::to_string(&index_keys)
                .unwrap_or_else(|_| "[]".to_string()),
            sqlite_open_dbs_json: serde_json::to_string(&sqlite_dbs)
                .unwrap_or_else(|_| "[]".to_string()),
            insights_counts_json: serde_json::to_string(&insights)
                .unwrap_or_else(|_| "{}".to_string()),
            hourly_statistics_hours,
        })
    }
}

// ====================== get_debug_console ======================

const DEBUG_CONSOLE_DEFAULT_LIMIT: i64 = 100;
const DEBUG_CONSOLE_MAX_LIMIT: i64 = 1000;

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct DebugConsoleInputData {
    #[property(description: "How many of the most recent messages to return. Default 100, hard cap 1000 (the console keeps the last 1000 messages in memory).")]
    pub limit: Option<i64>,
}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct DebugConsoleResponse {
    #[property(description: "Number of messages returned.")]
    pub count: i64,
    #[property(description: "Messages as JSON array string, oldest first. Each item: {moment: ISO-8601 UTC, message}. Contains jemalloc readings (sampled every 30s), GC activity and queue overflow warnings.")]
    pub messages_json: String,
}

pub struct DebugConsoleHandler {
    app: Arc<AppContext>,
}

impl DebugConsoleHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for DebugConsoleHandler {
    const FUNC_NAME: &'static str = "get_debug_console";
    const DESCRIPTION: &'static str = "Read the in-memory debug console of the logger server (ring buffer, last 1000 messages). It accumulates internal diagnostics: jemalloc memory readings sampled every 30 seconds, GC activity (which hour shards were deleted) and queue overflow warnings. Use it to see the memory consumption trend over the last hours or to investigate what server maintenance has been doing.";
}

#[async_trait::async_trait]
impl McpToolCall<DebugConsoleInputData, DebugConsoleResponse> for DebugConsoleHandler {
    async fn execute_tool_call(
        &self,
        model: DebugConsoleInputData,
    ) -> Result<DebugConsoleResponse, String> {
        let limit = model
            .limit
            .unwrap_or(DEBUG_CONSOLE_DEFAULT_LIMIT)
            .clamp(1, DEBUG_CONSOLE_MAX_LIMIT) as usize;

        let messages = self.app.debug_console.get_last(limit).await;

        let items: Vec<serde_json::Value> = messages
            .iter()
            .map(|item| {
                serde_json::json!({
                    "moment": item.moment.to_rfc3339(),
                    "message": item.message,
                })
            })
            .collect();

        Ok(DebugConsoleResponse {
            count: items.len() as i64,
            messages_json: serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string()),
        })
    }
}
