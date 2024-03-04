use std::sync::Arc;

use rust_extensions::AppStates;

use crate::{
    cache::FilterEventsCache,
    repo::{LogsRepo, SettingsRepo},
};

use super::LogsQueue;

pub struct AppContext {
    pub settings_reader: Arc<crate::settings::SettingsReader>,
    pub app_states: Arc<AppStates>,
    pub logs_repo: LogsRepo,
    pub logs_queue: LogsQueue,
    pub settings_repo: SettingsRepo,
    pub filter_events_cache: FilterEventsCache,
}

pub const APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

impl AppContext {
    pub async fn new(settings_reader: Arc<crate::settings::SettingsReader>) -> Self {
        let logs_db_path = settings_reader.get_logs_db_path("logs.db").await;
        let settings_db_path = settings_reader.get_logs_db_path("settings.db").await;
        Self {
            app_states: Arc::new(AppStates::create_initialized()),
            logs_repo: LogsRepo::new(logs_db_path).await,
            logs_queue: LogsQueue::new(),
            settings_reader,
            settings_repo: SettingsRepo::new(settings_db_path).await,
            filter_events_cache: FilterEventsCache::new(),
        }
    }
}
