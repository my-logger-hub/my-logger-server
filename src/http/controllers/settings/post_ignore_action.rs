use std::sync::Arc;

use super::contracts::*;
use my_http_server::{macros::http_route, HttpContext, HttpFailResult, HttpOkResult, HttpOutput};

use crate::app::AppContext;

#[http_route(
    method: "POST",
    route: "/api/settings/ignore",
    summary: "Set ignore events marker",
    description: "Set ignore events marker",
    input_data: PostIgnoreMaskHttpInput,
    controller: "Settings",
    result:[
        {status_code: 204, description: "Ok response"},
    ]
)]
pub struct PostIgnoreAction {
    app: Arc<AppContext>,
}

impl PostIgnoreAction {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}
async fn handle_request(
    action: &PostIgnoreAction,
    input_data: PostIgnoreMaskHttpInput,
    _ctx: &HttpContext,
) -> Result<HttpOkResult, HttpFailResult> {
    crate::flows::ignore_event::add(&action.app, input_data.into()).await;

    return HttpOutput::Empty.into_ok_result(true).into();
}
