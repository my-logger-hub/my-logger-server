use std::sync::Arc;

use my_ai_agent::{macros::ApplyJsonSchema, ToolDefinition};
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::app::AppContext;

const DEFAULT_HOURS: i64 = 24;
const MAX_HOURS: i64 = 48;

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct DashboardStatisticsInputData {
    #[property(description: "How many recent hours to include. Default 24, hard cap 48 (server keeps 48 hours of hourly stats).")]
    pub hours: Option<i64>,
}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct DashboardStatisticsResponse {
    #[property(description: "Hourly buckets, newest hour first, as JSON array string. Each item: {hour: u64 in YYYYMMDDHH UTC form, applications: [{application, info, warning, error, fatal, debug}]}.")]
    pub buckets_json: String,

    #[property(description: "Total Info-level events across all returned hours.")]
    pub total_info: i64,
    #[property(description: "Total Warning-level events.")]
    pub total_warning: i64,
    #[property(description: "Total Error-level events. Investigate when > 0.")]
    pub total_error: i64,
    #[property(description: "Total FatalError-level events. Always investigate when > 0.")]
    pub total_fatal: i64,
    #[property(description: "Total Debug-level events.")]
    pub total_debug: i64,
}

pub struct DashboardStatisticsHandler {
    app: Arc<AppContext>,
}

impl DashboardStatisticsHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for DashboardStatisticsHandler {
    const FUNC_NAME: &'static str = "get_dashboard_statistics";
    const DESCRIPTION: &'static str = "Get dashboard-style hourly statistics: per-hour, per-application counts of Info / Warning / Error / FatalError / Debug events. Use this first to spot which application/hour has errors, then call `search_logs` with a tighter time range and the offending Application to investigate.";
}

#[async_trait::async_trait]
impl McpToolCall<DashboardStatisticsInputData, DashboardStatisticsResponse>
    for DashboardStatisticsHandler
{
    async fn execute_tool_call(
        &self,
        model: DashboardStatisticsInputData,
    ) -> Result<DashboardStatisticsResponse, String> {
        let hours = model
            .hours
            .unwrap_or(DEFAULT_HOURS)
            .clamp(1, MAX_HOURS) as usize;

        let snapshot = {
            let read_access = self.app.hourly_statistics.lock().await;
            read_access.get_max_hours(hours)
        };

        let mut total_info: i64 = 0;
        let mut total_warning: i64 = 0;
        let mut total_error: i64 = 0;
        let mut total_fatal: i64 = 0;
        let mut total_debug: i64 = 0;

        let mut buckets = Vec::with_capacity(snapshot.len());

        for (hour, by_app) in snapshot {
            let mut applications = Vec::with_capacity(by_app.len());
            for (app_name, item) in by_app {
                total_info += item.info as i64;
                total_warning += item.warning as i64;
                total_error += item.error as i64;
                total_fatal += item.fatal_error as i64;
                total_debug += item.debug as i64;

                applications.push(serde_json::json!({
                    "application": app_name,
                    "info": item.info,
                    "warning": item.warning,
                    "error": item.error,
                    "fatal": item.fatal_error,
                    "debug": item.debug,
                }));
            }

            buckets.push(serde_json::json!({
                "hour": hour.get_value(),
                "applications": applications,
            }));
        }

        let buckets_json = serde_json::to_string(&buckets).unwrap_or_else(|_| "[]".to_string());

        Ok(DashboardStatisticsResponse {
            buckets_json,
            total_info,
            total_warning,
            total_error,
            total_fatal,
            total_debug,
        })
    }
}
