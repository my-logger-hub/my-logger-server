use std::{sync::Arc, time::Duration};

use app::AppContext;
use background::FlushToDbTimer;
use rust_extensions::MyTimer;

mod app;
mod background;
mod grpc_server;
mod http;
mod repo;
mod settings;
mod utils;

#[allow(non_snake_case)]
pub mod my_logger_grpc {
    tonic::include_proto!("my_logger");
}

#[tokio::main]
async fn main() {
    let settings_reader = crate::settings::SettingsReader::new(".my-logger-server").await;
    let settings_reader = Arc::new(settings_reader);

    let app = Arc::new(AppContext::new(settings_reader).await);

    crate::http::start_up::setup_server(app.clone()).await;

    let mut my_timer = MyTimer::new(Duration::from_millis(500));

    my_timer.register_timer("ToDbFlusher", Arc::new(FlushToDbTimer::new(app.clone())));

    my_timer.start(app.app_states.clone(), my_logger::LOGGER.clone());

    crate::grpc_server::start(app.clone());

    app.app_states.wait_until_shutdown().await;
}
