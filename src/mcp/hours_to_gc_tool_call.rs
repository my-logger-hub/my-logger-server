use std::sync::Arc;

use mcp_server_middleware::{ApplyJsonSchema, ToolDefinition};
use serde::{Deserialize, Serialize};

use mcp_server_middleware::McpToolCall;

use crate::{app::AppContext, logs_storage::LOCAL_HOURS};

/// How long the hours are kept, in words: without S3 it is not `hours_to_gc`
fn hours_kept_message(has_s3: bool, hours_to_gc: u64) -> String {
    if has_s3 {
        format!(
            "The Warning, Error and FatalError hours are kept in S3 for {} hours. The Debug and Info hours are deleted {} hours after they end.",
            hours_to_gc, LOCAL_HOURS
        )
    } else {
        format!(
            "S3 is not set up, so the hours are kept for {} hours: every hour is deleted {} hours after it ends. hours_to_gc is {} and is not used until S3 is set up.",
            LOCAL_HOURS, LOCAL_HOURS, hours_to_gc
        )
    }
}

// ====================== get_hours_to_gc ======================

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetHoursToGcInputData {}

#[derive(ApplyJsonSchema, Debug, Serialize, Deserialize)]
pub struct GetHoursToGcResponse {
    #[property(description: "How long the hours are kept, in words: 2 hours when S3 is not set up, otherwise the hours_to_gc which is set.")]
    pub message: String,
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
    const DESCRIPTION: &'static str = "Get how long the hours are kept, in words. When S3 is not set up every hour is deleted 2 hours after it ends, whatever hours_to_gc is set. With S3 the Warning, Error and FatalError hours are kept in it for hours_to_gc; the Debug and Info hours are deleted 2 hours after they end.";
}

#[async_trait::async_trait]
impl McpToolCall<GetHoursToGcInputData, GetHoursToGcResponse> for GetHoursToGcHandler {
    async fn execute_tool_call(
        &self,
        _model: GetHoursToGcInputData,
    ) -> Result<GetHoursToGcResponse, String> {
        Ok(GetHoursToGcResponse {
            message: hours_kept_message(
                self.app.logs_storage.has_s3(),
                self.app.settings_repo.get_hours_to_gc().await,
            ),
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
    #[property(description: "How long the hours are kept from now on, in words - as get_hours_to_gc tells it.")]
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
            message: hours_kept_message(
                self.app.logs_storage.has_s3(),
                self.app.settings_repo.get_hours_to_gc().await,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::hours_kept_message;

    #[test]
    fn without_s3_the_hours_are_kept_for_two_hours_whatever_is_set() {
        let message = hours_kept_message(false, 168);
        assert!(message.starts_with("S3 is not set up, so the hours are kept for 2 hours"), "{}", message);
        assert!(message.contains("hours_to_gc is 168 and is not used"), "{}", message);
    }

    #[test]
    fn with_s3_the_hours_are_kept_for_what_is_set() {
        let message = hours_kept_message(true, 168);
        assert!(message.contains("kept in S3 for 168 hours"), "{}", message);
        assert!(!message.contains("not set up"), "{}", message);
    }
}
