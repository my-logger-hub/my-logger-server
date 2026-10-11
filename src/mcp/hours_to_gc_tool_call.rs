use std::sync::Arc;

use mcp_server_middleware::{ApplyJsonSchema, ToolDefinition};
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::app::AppContext;

// ====================== get_hours_to_gc ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetHoursToGcInputData {}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetHoursToGcResponse {
    #[property(description: "How long the Warning, Error and FatalError hours are kept in S3, in hours.")]
    pub hours_to_gc: i64,
    #[property(description: "Whether S3 is set up. Without it every hour is deleted 2 hours after it ends and hours_to_gc is not used.")]
    pub has_s3: bool,
}

pub struct GetHoursToGcHandler {
    app: Arc<AppContext>,
}

impl GetHoursToGcHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for GetHoursToGcHandler {
    const FUNC_NAME: &'static str = "get_hours_to_gc";
    const DESCRIPTION: &'static str = "Get how long the Warning, Error and FatalError hours are kept in S3. The Debug and Info hours are deleted 2 hours after they end, and so is every hour when S3 is not set up.";
}

#[async_trait::async_trait]
impl McpToolCall<GetHoursToGcInputData, GetHoursToGcResponse> for GetHoursToGcHandler {
    async fn execute_tool_call(
        &self,
        _model: GetHoursToGcInputData,
    ) -> Result<GetHoursToGcResponse, String> {
        Ok(GetHoursToGcResponse {
            hours_to_gc: self.app.settings_repo.get_hours_to_gc().await as i64,
            has_s3: self.app.logs_storage.has_s3(),
        })
    }
}

// ====================== set_hours_to_gc ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SetHoursToGcInputData {
    #[property(description: "How long the Warning, Error and FatalError hours are kept in S3, in hours. An hour is uploaded 3 hours after it starts: with 3 or less nothing goes to S3 and the hours are deleted 2 hours after they end. The hours older are deleted from S3 within 10 minutes.")]
    pub hours_to_gc: i64,
}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct SetHoursToGcResponse {
    #[property(description: "Human-readable result message.")]
    pub message: String,
}

pub struct SetHoursToGcHandler {
    app: Arc<AppContext>,
}

impl SetHoursToGcHandler {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}

impl ToolDefinition for SetHoursToGcHandler {
    const FUNC_NAME: &'static str = "set_hours_to_gc";
    const DESCRIPTION: &'static str = "Set how long the Warning, Error and FatalError hours are kept in S3. Lowering it deletes the hours older from S3 for good.";
}

#[async_trait::async_trait]
impl McpToolCall<SetHoursToGcInputData, SetHoursToGcResponse> for SetHoursToGcHandler {
    async fn execute_tool_call(
        &self,
        model: SetHoursToGcInputData,
    ) -> Result<SetHoursToGcResponse, String> {
        crate::flows::set_hours_to_gc(&self.app, model.hours_to_gc).await?;

        Ok(SetHoursToGcResponse {
            message: format!("The hours are kept in S3 for {} hours", model.hours_to_gc),
        })
    }
}
