use std::sync::Arc;

use super::contracts::*;
use my_http_server::{macros::http_route, HttpContext, HttpFailResult, HttpOkResult, HttpOutput};

use crate::app::AppContext;

#[http_route(
    method: "POST",
    route: "/api/v2",
    summary: "Writes Logs in Compliant Json Format",
    description: "Writes Logs in Compliant Json Format",
    input_data: PostJsonLogsV2InputData,
    controller: "LogWriter",
    result:[
        {status_code: 204, description: "Ok response"},
    ]
)]
pub struct PostLogsV2Action {
    app: Arc<AppContext>,
}

impl PostLogsV2Action {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}
async fn handle_request(
    action: &PostLogsV2Action,
    input_data: PostJsonLogsV2InputData,
    _ctx: &HttpContext,
) -> Result<HttpOkResult, HttpFailResult> {
    let log_events = input_data.parse_log_events()?;

    if log_events.len() > 0 {
        crate::flows::post_items(&action.app, log_events).await;
    }

    return HttpOutput::Empty.into_ok_result(true).into();
}
