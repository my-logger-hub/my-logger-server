use std::sync::Arc;

use elastic_client::{ElasticClient, ElasticClientAuth};
use rust_extensions::AppStates;
use tokio::sync::Mutex;

use crate::{
    cache::FilterEventsCache,
    hourly_statistics::HourlyStatistics,
    ignore_single_events::IgnoreSingleEventCache,
    insights_repo::InsightsRepo,
    logs_storage::{LogsStorage, S3Archive},
    repo::SettingsRepo,
    telegram::TelegramNotificationData,
};

use super::{DebugConsole, LogsQueue};

pub const PROCESS_CONTEXT_KEY: &'static str = "Process";

pub struct ElasticInner {
    pub client: ElasticClient,
    pub logs_queue: LogsQueue,
}

pub struct AppContext {
    pub settings_reader: Arc<crate::settings::SettingsReader>,
    pub app_states: Arc<AppStates>,
    pub logs_storage: Arc<LogsStorage>,
    pub settings_repo: SettingsRepo,
    pub filter_events_cache: FilterEventsCache,
    pub elastic: Option<ElasticInner>,
    pub is_debug: bool,
    pub ignore_single_event_cache: Mutex<IgnoreSingleEventCache>,

    pub telegram_notification_data: Mutex<TelegramNotificationData>,

    pub insights_repo: InsightsRepo,

    pub hourly_statistics: Mutex<HourlyStatistics>,

    pub statistics_path: String,

    pub env_name: String,

    pub ui_url: Mutex<String>,

    pub debug_console: Arc<DebugConsole>,
}

pub const APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

impl AppContext {
    pub async fn new(settings_reader: Arc<crate::settings::SettingsReader>) -> Self {
        let logs_db_path = settings_reader.get_logs_db_path(None).await;
        let settings_db_path = settings_reader
            .get_logs_db_path("settings.json".into())
            .await;
        let statistics_path = settings_reader
            .get_logs_db_path("statistics.json".into())
            .await;

        let mut is_debug = false;

        if let Ok(value) = std::env::var("DEBUG") {
            if value == "true" {
                is_debug = true;
            }

            if value == "1" {
                is_debug = true;
            }
        }

        let env_name = settings_reader.get_env_name().await;

        let insight_keys = settings_reader.get_insights_keys().await;

        let insights_repo = InsightsRepo::new(insight_keys.clone(), 1024);

        let debug_console = Arc::new(DebugConsole::new());

        let s3 = settings_reader
            .get_s3_settings()
            .await
            .map(|settings| S3Archive::new(&settings));

        Self {
            app_states: Arc::new(AppStates::create_initialized()),
            logs_storage: Arc::new(LogsStorage::new(logs_db_path, s3, insight_keys.clone())),
            env_name,
            settings_repo: SettingsRepo::new(settings_db_path).await,
            filter_events_cache: FilterEventsCache::new(),
            ignore_single_event_cache: Mutex::new(IgnoreSingleEventCache::new()),
            hourly_statistics: Mutex::new(HourlyStatistics::new()),
            statistics_path,
            elastic: settings_reader
                .get_elastic_settings()
                .await
                .map(|x| ElasticInner {
                    logs_queue: LogsQueue::new("elastic", debug_console.clone()),
                    client: ElasticClient::new(ElasticClientAuth::SingleNode {
                        url: x.url,
                        esecure: Some(x.esecure),
                    })
                    .unwrap(),
                }),
            settings_reader,
            is_debug,
            insights_repo,
            telegram_notification_data: Mutex::new(TelegramNotificationData::new()),
            ui_url: Mutex::new(String::new()),
            debug_console,
        }
    }

    pub async fn update_ui_url(&self, ui_url: &str) {
        let mut write_access = self.ui_url.lock().await;
        if &*write_access != ui_url {
            *write_access = ui_url.to_string()
        }
    }

    pub async fn get_ui_url(&self) -> String {
        let read_access = self.ui_url.lock().await;
        read_access.clone()
    }
}
