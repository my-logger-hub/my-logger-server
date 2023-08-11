use std::sync::Arc;

use my_http_server_controllers::controllers::{
    ControllersAuthorization, ControllersMiddleware, RequiredClaims,
};

use crate::app::AppContext;

pub fn build_controllers(app: &Arc<AppContext>) -> ControllersMiddleware {
    let mut result = ControllersMiddleware::new(
        ControllersAuthorization::BearerAuthentication {
            global: true,
            global_claims: RequiredClaims::no_claims(),
        }
        .into(),
        None,
    );

    result.register_post_action(Arc::new(
        super::controllers::logs_income::PostLogsAction::new(app.clone()),
    ));

    result
}
