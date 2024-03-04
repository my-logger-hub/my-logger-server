use std::sync::Arc;

use super::contracts::*;
use my_http_server::{macros::http_route, HttpContext, HttpFailResult, HttpOkResult, HttpOutput};

use crate::{app::AppContext, repo::dto::IgnoreItemDto};

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
    crate::flows::add_ignore_event(
        &action.app,
        IgnoreItemDto {
            level: input_data.level.into(),
            application: input_data.application,
            marker: input_data.marker,
        },
    )
    .await;

    action.app.filter_events_cache.reset().await;
    return HttpOutput::Empty.into_ok_result(true).into();
}
