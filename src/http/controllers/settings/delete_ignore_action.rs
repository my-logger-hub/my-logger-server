use std::sync::Arc;

use super::contracts::*;
use my_http_server::{macros::http_route, HttpContext, HttpFailResult, HttpOkResult, HttpOutput};

use crate::{app::AppContext, repo::dto::IgnoreWhereModel};

#[http_route(
    method: "DELETE",
    route: "/api/settings/ignore",
    summary: "Delete ignore events marker",
    description: "Delete ignore events marker",
    input_data: DeleteIgnoreMaskHttpInput,
    controller: "Settings",
    result:[
        {status_code: 204, description: "Ok response"},
    ]
)]
pub struct DeleteIgnoreAction {
    app: Arc<AppContext>,
}

impl DeleteIgnoreAction {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}
async fn handle_request(
    action: &DeleteIgnoreAction,
    input_data: DeleteIgnoreMaskHttpInput,
    _ctx: &HttpContext,
) -> Result<HttpOkResult, HttpFailResult> {
    crate::flows::remove_ignore_event(
        &action.app,
        IgnoreWhereModel {
            level: input_data.level.into(),
            application: input_data.application,
            marker: input_data.marker,
        },
    )
    .await;

    return HttpOutput::Empty.into_ok_result(true).into();
}
