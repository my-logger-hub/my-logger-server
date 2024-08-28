use std::sync::Arc;

use elastic_client::{ElasticClient, ElasticClientAuth};
use rust_extensions::AppStates;

use crate::{
    cache::FilterEventsCache,
    repo::{LogsRepo, SettingsRepo},
};

use super::LogsQueue;

pub const PROCESS_CONTEXT_KEY: &'static str = "Process";

pub struct AppContext {
    pub settings_reader: Arc<crate::settings::SettingsReader>,
    pub app_states: Arc<AppStates>,
    pub logs_repo: LogsRepo,
    pub logs_queue: LogsQueue,
    pub settings_repo: SettingsRepo,
    pub filter_events_cache: FilterEventsCache,
    pub elastic_client: Option<ElasticClient>,
}

pub const APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

impl AppContext {
    pub async fn new(settings_reader: Arc<crate::settings::SettingsReader>) -> Self {
        let logs_db_path = settings_reader.get_logs_db_path(None).await;
        let settings_db_path = settings_reader.get_logs_db_path("settings.db".into()).await;
        Self {
            app_states: Arc::new(AppStates::create_initialized()),
            logs_repo: LogsRepo::new(logs_db_path).await,
            logs_queue: LogsQueue::new(),
            settings_repo: SettingsRepo::new(settings_db_path).await,
            filter_events_cache: FilterEventsCache::new(),
            elastic_client: settings_reader.get_elastic_settings().await.map(|x| {
                ElasticClient::new(ElasticClientAuth::SingleNode {
                    url: x.url,
                    esecure: Some(x.esecure),
                })
                .unwrap()
            }),
            settings_reader,
        }
    }
}
