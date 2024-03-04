use std::sync::Arc;

use my_http_server::controllers::ControllersMiddleware;

use crate::app::AppContext;

pub fn build_controllers(app: &Arc<AppContext>) -> ControllersMiddleware {
    let mut result = ControllersMiddleware::new(None, None);

    result.register_post_action(Arc::new(
        super::controllers::logs_income::PostLogsAction::new(app.clone()),
    ));

    result.register_post_action(Arc::new(
        super::controllers::logs_income::PostLogsV2Action::new(app.clone()),
    ));

    // Settings controller

    result.register_post_action(Arc::new(
        super::controllers::settings::PostIgnoreAction::new(app.clone()),
    ));
    result.register_get_action(Arc::new(
        super::controllers::settings::GetIgnoreAction::new(app.clone()),
    ));
    result
}
