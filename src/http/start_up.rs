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

    let unix_socket = std::env::var("UNIX_SOCKET");

    let mut unix_socket = if let Ok(unix_socket) = unix_socket {
        Some(MyHttpServer::new_as_unix_socket(unix_socket))
    } else {
        None
    };

    let controllers = Arc::new(super::builder::build_controllers(&app));

    let swagger_middleware = SwaggerMiddleware::new(
        controllers.clone(),
        crate::app::APP_NAME.to_string(),
        crate::app::APP_VERSION.to_string(),
    );

    let swagger_middleware = Arc::new(swagger_middleware);

    let mcp = Arc::new(crate::mcp::build_mcp_middleware(&app).await);

    if let Some(unix_socket) = unix_socket.as_mut() {
        unix_socket.add_middleware(swagger_middleware.clone());
        unix_socket.add_middleware(mcp.clone());
        unix_socket.add_middleware(controllers.clone());
        unix_socket.start(app.app_states.clone(), my_logger::LOGGER.clone());
    }

    http_server.add_middleware(swagger_middleware);
    http_server.add_middleware(mcp);
    http_server.add_middleware(controllers);

    http_server.start(app.app_states.clone(), my_logger::LOGGER.clone());
}
