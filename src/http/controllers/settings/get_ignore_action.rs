use std::sync::Arc;

use my_http_server::{macros::http_route, HttpContext, HttpFailResult, HttpOkResult, HttpOutput};

use super::contracts::*;
use crate::app::AppContext;

#[http_route(
    method: "GET",
    route: "/api/settings/ignore",
    summary: "Get ignore events",
    description: "Get ignore events",
    controller: "Settings",
    result:[
        {status_code: 200, description: "Ok response", model:"Vec<IgnoreEventHttpModel>"},
    ]
)]
pub struct GetIgnoreAction {
    app: Arc<AppContext>,
}

impl GetIgnoreAction {
    pub fn new(app: Arc<AppContext>) -> Self {
        Self { app }
    }
}
async fn handle_request(
    action: &GetIgnoreAction,
    _ctx: &HttpContext,
) -> Result<HttpOkResult, HttpFailResult> {
    let result = action.app.settings_repo.get_ignore_events().await;

    let mut model = Vec::with_capacity(result.len());

    for itm in result {
        model.push(IgnoreEventHttpModel {
            level: format!("{:?}", itm.level),
            application: itm.application,
            marker: itm.marker,
        });
    }

    return HttpOutput::as_json(model).into_ok_result(true).into();
}
