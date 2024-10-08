use my_http_server::{controllers::swagger::SwaggerMiddleware, MyHttpServer};
use std::{net::SocketAddr, sync::Arc};

use crate::app::AppContext;

const DEFAULT_PORT: u16 = 8000;

pub async fn setup_server(app: Arc<AppContext>) {
    let http_port = if let Ok(result) = std::env::var("HTTP_PORT") {
        match result.parse() {
            Ok(port) => port,
            Err(_) => DEFAULT_PORT,
        }
    } else {
        DEFAULT_PORT
    };
    let mut http_server = MyHttpServer::new(SocketAddr::from(([0, 0, 0, 0], http_port)));

    let controllers = Arc::new(super::builder::build_controllers(&app));

    let swagger_middleware = SwaggerMiddleware::new(
        controllers.clone(),
        crate::app::APP_NAME.to_string(),
        crate::app::APP_VERSION.to_string(),
    );

    http_server.add_middleware(Arc::new(swagger_middleware));

    http_server.add_middleware(controllers);

    http_server.start(app.app_states.clone(), my_logger::LOGGER.clone());
}
