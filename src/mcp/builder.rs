use std::sync::Arc;

use mcp_server_middleware::McpMiddleware;

use crate::app::AppContext;

const MCP_PATH: &str = "/mcp";
const MCP_NAME: &str = "MyLogger";
const MCP_VERSION: &str = env!("CARGO_PKG_VERSION");
const MCP_INSTRUCTIONS: &str = "MyLogger MCP server. Workflow for incident investigation: (1) call `get_dashboard_statistics` first to see hourly per-application Error / FatalError counts and identify which application and hour have problems; (2) call `search_logs` with a tight date range, the offending `application` (and optionally `version`), and an optional `phrase` for full-text search to retrieve the actual log records. All times are ISO-8601 / RFC-3339 in UTC. To suppress known-noise events, manage ignore rules with `get_ignore_rules`, `add_ignore_rule` and `delete_ignore_rule` (level + application + context marker; \"*\" is a wildcard).";

pub async fn build_mcp_middleware(app: &Arc<AppContext>) -> McpMiddleware {
    let mut middleware = McpMiddleware::new(MCP_PATH, MCP_NAME, MCP_VERSION, MCP_INSTRUCTIONS);

    middleware.register_tool_call(Arc::new(super::SearchLogsHandler::new(app.clone())));

    middleware.register_tool_call(Arc::new(super::DashboardStatisticsHandler::new(
        app.clone(),
    )));

    middleware.register_tool_call(Arc::new(super::GetIgnoreRulesHandler::new(app.clone())));
    middleware.register_tool_call(Arc::new(super::AddIgnoreRuleHandler::new(app.clone())));
    middleware.register_tool_call(Arc::new(super::DeleteIgnoreRuleHandler::new(app.clone())));

    middleware
}
