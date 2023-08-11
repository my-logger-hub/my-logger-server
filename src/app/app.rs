use std::sync::Arc;

use rust_extensions::AppStates;

use crate::postgres::LogsRepo;

use super::LogsQueue;

pub struct AppContext {
    pub settings_reader: Arc<crate::settings::SettingsReader>,
    pub app_states: Arc<AppStates>,
    pub logs_repo: LogsRepo,
    pub logs_queue: LogsQueue,
}

pub const APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

impl AppContext {
    pub async fn new(settings_reader: Arc<crate::settings::SettingsReader>) -> Self {
        Self {
            app_states: Arc::new(AppStates::create_initialized()),
            logs_repo: LogsRepo::new(settings_reader.clone()).await,
            logs_queue: LogsQueue::new(),
            settings_reader,
        }
    }
}
