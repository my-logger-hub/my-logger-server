use std::sync::Arc;

use elastic_client::{ElasticClient, ElasticClientAuth};
use rust_extensions::AppStates;
use tokio::sync::Mutex;

use crate::{
    ignore_single_events_cache::IgnoreSingleEventCache,
    log_item::*,
    repo::{ignore_events::*, ignore_single_events::*, logs::*, statistics::*},
    telegram::TelegramNotificationData,
};

pub const APPLICATION_KEY: &'static str = "Application";
//pub const PROCESS_CONTEXT_KEY: &'static str = "Process";

pub struct ElasticInner {
    pub client: ElasticClient,
    pub logs_queue: LogsQueue,
}

pub struct AppContext {
    pub settings_reader: Arc<crate::settings::SettingsReader>,
    pub app_states: Arc<AppStates>,
    pub logs_repo: LogsRepo,
    pub logs_queue: LogsQueue,
    pub elastic: Option<ElasticInner>,
    pub is_debug: bool,

    pub telegram_notification_data: TelegramNotificationData,

    pub hour_statistics_repo: HourStatisticsRepo,

    pub ignore_events_repo: IgnoreEventsRepo,

    pub ignore_single_events_repo: IgnoreSingleEventsRepo,

    pub ignore_single_event_cache: Mutex<IgnoreSingleEventCache>,

    pub env_name: String,

    pub ui_url: Mutex<String>,
}

pub const APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

impl AppContext {
    pub async fn new(settings_reader: Arc<crate::settings::SettingsReader>) -> Self {
        let logs_db_path = settings_reader.get_logs_db_path().await;

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

        Self {
            env_name,
            ignore_events_repo: IgnoreEventsRepo::new(logs_db_path.clone()).await,
            app_states: Arc::new(AppStates::create_initialized()),
            logs_repo: LogsRepo::new(logs_db_path.clone()),
            logs_queue: LogsQueue::new(),
            ignore_single_events_repo: IgnoreSingleEventsRepo::new(logs_db_path.clone()).await,
            hour_statistics_repo: HourStatisticsRepo::new(logs_db_path).await,
            ignore_single_event_cache: Mutex::default(),
            elastic: settings_reader
                .get_elastic_settings()
                .await
                .map(|x| ElasticInner {
                    logs_queue: LogsQueue::new(),
                    client: ElasticClient::new(ElasticClientAuth::SingleNode {
                        url: x.url,
                        esecure: Some(x.esecure),
                    })
                    .unwrap(),
                }),
            settings_reader,
            is_debug,
            telegram_notification_data: TelegramNotificationData::new(),
            ui_url: Mutex::new(String::new()),
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
