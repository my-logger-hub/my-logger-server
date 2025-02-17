use std::sync::Arc;

use super::contracts::*;
use my_http_server::{macros::http_route, HttpContext, HttpFailResult, HttpOkResult, HttpOutput};

use crate::app::AppContext;

#[http_route(
    method: "POST",
    route: "/api/events/raw",
    summary: "Writes Logs in Seq Format",
    description: "Writes Logs in Seq Format",
    input_data: "SeqInputHttpData",
    controller: "LogWriter",
    result:[
        {status_code: 204, description: "Ok response"},
    ]
)]
pub struct PostLogsAction {
    app: Arc<AppContext>,
}

impl PostLogsAction {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}
async fn handle_request(
    action: &PostLogsAction,
    input_data: SeqInputHttpData,
    _ctx: &HttpContext,
) -> Result<HttpOkResult, HttpFailResult> {
    /*
    println!(
        "We have log event: {}",
        std::str::from_utf8(input_data.body.as_slice()).unwrap()
    );
     */
    let log_events = input_data.parse_log_events();

    if log_events.len() > 0 {
        crate::flows::post_items(&action.app, log_events).await;
    }

    return HttpOutput::Empty.into_ok_result(true).into();
}
