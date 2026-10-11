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
mod logs_storage;
mod mcp;
mod product_scopes;
mod repo;
mod settings;
mod telegram;
#[allow(non_snake_case)]
pub mod my_logger_grpc {
    tonic::include_proto!("my_logger");
}

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Read by jemalloc at init on Linux (unprefixed build); the MALLOC_CONF env var still overrides.
// background_thread + short decay return freed pages to the OS within ~5s.
#[allow(non_upper_case_globals)]
#[export_name = "malloc_conf"]
pub static malloc_conf: &[u8] = b"background_thread:true,dirty_decay_ms:5000,muzzy_decay_ms:5000\0";

#[tokio::main]
async fn main() {
    let settings_reader = crate::settings::SettingsReader::new("~/.my-logger-server").await;
    let settings_reader = Arc::new(settings_reader);
    let elastic_settings = settings_reader.get_elastic_settings().await;
    let app = Arc::new(AppContext::new(settings_reader).await);

    crate::flows::init(&app).await;

    crate::http::start_up::setup_server(app.clone()).await;

    if let Some(elastic_settings) = elastic_settings {
        let mut elastic_timer = MyTimer::new(Duration::from_millis(500), my_logger::LOGGER.clone());
        elastic_timer.register_timer(
            "ToElasticFlusher",
            Arc::new(FlushToElastic::new(
                app.clone(),
                &elastic_settings.env_source,
            )),
        );
        elastic_timer.start();
    }
    let mut my_timer = MyTimer::new(Duration::from_millis(500), my_logger::LOGGER.clone());
    my_timer.register_timer(
        "ToFilesFlusher",
        Arc::new(FlushToFilesTimer::new(app.clone())),
    );
    my_timer.start();

    if app.logs_storage.has_s3() {
        // An upload of an hour may take minutes
        let mut s3_timer = MyTimer::new_with_execute_timeout(
            Duration::from_secs(60),
            Duration::from_secs(60 * 60),
            my_logger::LOGGER.clone(),
        );
        s3_timer.register_timer("ToS3Uploader", Arc::new(UploadToS3Timer::new(app.clone())));
        s3_timer.start();
    }

    let mut gc_timer = MyTimer::new(Duration::from_secs(30), my_logger::LOGGER.clone());
    gc_timer.register_timer("GcTimer", Arc::new(GcTimer::new(app.clone()).await));
    gc_timer.register_timer(
        "TelegramPusher",
        Arc::new(NotifyTelegramTimer::new(app.clone())),
    );
    gc_timer.start();

    let mut persist_timer = MyTimer::new(Duration::from_secs(60), my_logger::LOGGER.clone());
    persist_timer.register_timer(
        "PersistStatistics",
        Arc::new(PersistStatisticsTimer::new(app.clone())),
    );
    persist_timer.start();

    crate::grpc_server::start(app.clone());

    app.app_states.wait_until_shutdown().await;

    // What waits in memory is written before the process ends
    crate::flows::flush_logs_to_files(&app, true).await;
}
