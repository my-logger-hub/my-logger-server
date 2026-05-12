use std::sync::Arc;
use std::time::Duration;

use my_ai_agent::{macros::ApplyJsonSchema, ToolDefinition};
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::{
    app::AppContext,
    repo::dto::{IgnoreItemDto, IgnoreWhereModel, LogLevelDto},
};

fn parse_level(level: &str) -> Result<LogLevelDto, String> {
    match level.trim().to_lowercase().as_str() {
        "info" => Ok(LogLevelDto::Info),
        "warning" | "warn" => Ok(LogLevelDto::Warning),
        "error" => Ok(LogLevelDto::Error),
        "fatalerror" | "fatal" => Ok(LogLevelDto::FatalError),
        "debug" => Ok(LogLevelDto::Debug),
        _ => Err(format!(
            "Unknown log level '{}'. Expected one of: Info, Warning, Error, FatalError, Debug.",
            level
        )),
    }
}

// ====================== get_ignore_rules ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetIgnoreRulesInputData {}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetIgnoreRulesResponse {
    #[property(description: "Number of ignore rules currently configured.")]
    pub count: i64,
    #[property(description: "Ignore rules as JSON array string. Each item: {level, application, marker, expires_at}. `expires_at` is an ISO-8601 / RFC-3339 UTC moment, or null for a rule that never expires.")]
    pub rules_json: String,
}

pub struct GetIgnoreRulesHandler {
    app: Arc<AppContext>,
}

impl GetIgnoreRulesHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for GetIgnoreRulesHandler {
    const FUNC_NAME: &'static str = "get_ignore_rules";
    const DESCRIPTION: &'static str = "List the ignore (suppression) rules. A matching log record (by level + application + context marker) is dropped before it is stored or alerted on. Use this to see what is currently being suppressed before adding or removing a rule.";
}

#[async_trait::async_trait]
impl McpToolCall<GetIgnoreRulesInputData, GetIgnoreRulesResponse> for GetIgnoreRulesHandler {
    async fn execute_tool_call(
        &self,
        _model: GetIgnoreRulesInputData,
    ) -> Result<GetIgnoreRulesResponse, String> {
        let items = self.app.settings_repo.get_ignore_events().await;

        let rules: Vec<_> = items
            .iter()
            .map(|itm| {
                serde_json::json!({
                    "level": format!("{:?}", itm.level),
                    "application": itm.application,
                    "marker": itm.marker,
                    "expires_at": itm
                        .expires_at
                        .map(|micros| DateTimeAsMicroseconds::new(micros).to_rfc3339()),
                })
            })
            .collect();

        Ok(GetIgnoreRulesResponse {
            count: rules.len() as i64,
            rules_json: serde_json::to_string(&rules).unwrap_or_else(|_| "[]".to_string()),
        })
    }
}

// ====================== add_ignore_rule ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct AddIgnoreRuleInputData {
    #[property(description: "Log level the rule applies to. One of: Info, Warning, Error, FatalError, Debug.")]
    pub level: String,
    #[property(description: "Application name the rule applies to. Use \"*\" to match any application.")]
    pub application: String,
    #[property(description: "Context marker (a context key present on the log record) the rule matches. Use \"*\" to match any record of the given level/application.")]
    pub marker: String,
    #[property(description: "Optional. Auto-remove this rule after the given number of minutes. Omit for a permanent rule. If a rule with the same level/application/marker already exists, its expiration is updated.")]
    pub expires_in_minutes: Option<i64>,
}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct ManageIgnoreRuleResponse {
    #[property(description: "Human-readable result message.")]
    pub message: String,
}

pub struct AddIgnoreRuleHandler {
    app: Arc<AppContext>,
}

impl AddIgnoreRuleHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for AddIgnoreRuleHandler {
    const FUNC_NAME: &'static str = "add_ignore_rule";
    const DESCRIPTION: &'static str = "Add an ignore (suppression) rule. Log records matching the given level + application + context marker are dropped before they are stored or alerted on. Use marker \"*\" to suppress every record of that level/application, and application \"*\" to match any application.";
}

#[async_trait::async_trait]
impl McpToolCall<AddIgnoreRuleInputData, ManageIgnoreRuleResponse> for AddIgnoreRuleHandler {
    async fn execute_tool_call(
        &self,
        model: AddIgnoreRuleInputData,
    ) -> Result<ManageIgnoreRuleResponse, String> {
        let level = parse_level(&model.level)?;

        let expires_at = match model.expires_in_minutes {
            Some(minutes) => {
                if minutes <= 0 {
                    return Err("`expires_in_minutes` must be a positive number.".to_string());
                }
                let dt = DateTimeAsMicroseconds::now()
                    .add(Duration::from_secs(minutes as u64 * 60));
                Some(dt.unix_microseconds)
            }
            None => None,
        };

        crate::flows::add_ignore_event(
            &self.app,
            IgnoreItemDto {
                level: level.clone(),
                application: model.application.clone(),
                marker: model.marker.clone(),
                expires_at,
            },
        )
        .await;

        let expiry_text = match expires_at {
            Some(micros) => format!(
                ", expires at {}",
                DateTimeAsMicroseconds::new(micros).to_rfc3339()
            ),
            None => " (no expiration)".to_string(),
        };

        Ok(ManageIgnoreRuleResponse {
            message: format!(
                "Ignore rule added: level={:?}, application='{}', marker='{}'{}.",
                level, model.application, model.marker, expiry_text
            ),
        })
    }
}

// ====================== delete_ignore_rule ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct DeleteIgnoreRuleInputData {
    #[property(description: "Log level of the rule to delete. One of: Info, Warning, Error, FatalError, Debug.")]
    pub level: String,
    #[property(description: "Application name of the rule to delete (must match exactly, including \"*\").")]
    pub application: String,
    #[property(description: "Context marker of the rule to delete (must match exactly, including \"*\").")]
    pub marker: String,
}

pub struct DeleteIgnoreRuleHandler {
    app: Arc<AppContext>,
}

impl DeleteIgnoreRuleHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for DeleteIgnoreRuleHandler {
    const FUNC_NAME: &'static str = "delete_ignore_rule";
    const DESCRIPTION: &'static str = "Delete an existing ignore (suppression) rule. The level + application + marker must match an existing rule exactly (see `get_ignore_rules`). After deletion matching log records are stored and alerted on again.";
}

#[async_trait::async_trait]
impl McpToolCall<DeleteIgnoreRuleInputData, ManageIgnoreRuleResponse> for DeleteIgnoreRuleHandler {
    async fn execute_tool_call(
        &self,
        model: DeleteIgnoreRuleInputData,
    ) -> Result<ManageIgnoreRuleResponse, String> {
        let level = parse_level(&model.level)?;

        crate::flows::remove_ignore_event(
            &self.app,
            IgnoreWhereModel {
                level: level.clone(),
                application: model.application.clone(),
                marker: model.marker.clone(),
            },
        )
        .await;

        Ok(ManageIgnoreRuleResponse {
            message: format!(
                "Ignore rule removed (if it existed): level={:?}, application='{}', marker='{}'.",
                level, model.application, model.marker
            ),
        })
    }
}
