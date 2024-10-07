use std::{sync::Arc, time::Duration};

use app::AppContext;
use background::*;
use rust_extensions::MyTimer;

mod app;
mod background;
mod cache;
mod flows;
mod grpc_server;
mod hourly_statistics;
mod http;
mod ignore_single_events;
mod insights_repo;
mod repo;
mod settings;
mod telegram;
mod utils;
#[allow(non_snake_case)]
pub mod my_logger_grpc {
    tonic::include_proto!("my_logger");
}

#[tokio::main]
async fn main() {
    let settings_reader = crate::settings::SettingsReader::new(".my-logger-server").await;
    let settings_reader = Arc::new(settings_reader);
    let elastic_settings = settings_reader.get_elastic_settings().await;
    let app = Arc::new(AppContext::new(settings_reader).await);

    crate::http::start_up::setup_server(app.clone()).await;

    if let Some(elastic_settings) = elastic_settings {
        let mut elastic_timer = MyTimer::new(Duration::from_millis(500));
        elastic_timer.register_timer(
            "ToElasticFlusher",
            Arc::new(FlushToElastic::new(
                app.clone(),
                &elastic_settings.env_source,
            )),
        );
        elastic_timer.start(app.app_states.clone(), my_logger::LOGGER.clone());
    }
    let mut my_timer = MyTimer::new(Duration::from_millis(500));
    my_timer.register_timer("ToDbFlusher", Arc::new(FlushToDbTimer::new(app.clone())));
    my_timer.start(app.app_states.clone(), my_logger::LOGGER.clone());

    let mut gc_timer = MyTimer::new(Duration::from_secs(30));
    gc_timer.register_timer("GcTimer", Arc::new(GcTimer::new(app.clone()).await));
    gc_timer.register_timer(
        "TelegramPusher",
        Arc::new(NotifyTelegramTimer::new(app.clone())),
    );
    gc_timer.start(app.app_states.clone(), my_logger::LOGGER.clone());

    crate::flows::init(&app).await;

    crate::grpc_server::start(app.clone());

    app.app_states.wait_until_shutdown().await;
}
