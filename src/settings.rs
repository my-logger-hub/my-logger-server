use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TelegramSettings {
    pub api_key: String,
    pub chat_id: i64,
    pub message_thread_id: i32,
    pub env_info: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ElasticSettings {
    pub esecure: String,
    pub url: String,
}

#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    #[serde(rename = "DefaultTenant")]
    pub default_tenant: String,

    #[serde(rename = "ApiKey")]
    pub api_key: Option<String>,

    #[serde(rename = "LogsDbPath")]
    pub logs_db_path: String,

    pub hours_to_gc: u64,

    #[serde(rename = "TelegramSettings")]
    pub telegram_settings: Option<TelegramSettings>,
    
    #[serde(rename = "TelegramSettings")]
    pub elastic: Option<ElasticSettings>,
}

impl SettingsReader {
    pub async fn get_telegram_settings(&self) -> Option<TelegramSettings> {
        let read_access = self.settings.read().await;
        read_access.telegram_settings.clone()
    }

    pub async fn get_elastic_settings(&self) -> Option<ElasticSettings> {
        let read_access = self.settings.read().await;
        read_access.elastic.clone()
    }

    pub async fn get_duration_to_gc(&self) -> Duration {
        let read_access = self.settings.read().await;
        Duration::from_secs(60 * 60 * read_access.hours_to_gc as u64)
    }

    pub async fn get_logs_db_path(&self, file_name: Option<&str>) -> String {
        let read_access = self.settings.read().await;

        let mut result = if read_access.logs_db_path.starts_with("~") {
            read_access
                .logs_db_path
                .replace("~", &std::env::var("HOME").unwrap())
        } else {
            read_access.logs_db_path.clone()
        };

        if !result.ends_with(std::path::MAIN_SEPARATOR) {
            result.push('/')
        }

        if let Some(file_name) = file_name {
            result.push_str(file_name);
        }

        result
    }

    pub async fn get_default_tenant(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.default_tenant.clone()
    }
}
