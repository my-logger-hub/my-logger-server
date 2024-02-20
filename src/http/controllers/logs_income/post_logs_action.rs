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
        {status_code: 202, description: "Ok response"},
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
    let get_default_tenant = action.app.settings_reader.get_default_tenant().await;

    println!(
        "We have log event: {}",
        std::str::from_utf8(input_data.body.as_slice()).unwrap()
    );

    let log_events = input_data.parse_log_events(get_default_tenant.as_str());

    if let Some(log_events) = log_events {
        let log_events = action
            .app
            .settings_reader
            .filter_events(log_events, |event, filter_events| {
                for filter in filter_events {
                    if filter.matches_ignore_filter(event) {
                        return false;
                    }
                }

                true
            })
            .await;

        if log_events.len() > 0 {
            action.app.logs_queue.add(log_events).await;
        }
    }

    return HttpOutput::Empty.into_ok_result(true).into();
}
