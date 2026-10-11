use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TelegramSettings {
    pub api_key: String,
    pub chat_id: i64,
    pub message_thread_id: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ElasticSettings {
    pub esecure: String,
    pub url: String,
    pub env_source: String,
}

/// Where the Warning, Error and FatalError hours go once they are older than the hours
/// kept on the local disk
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct S3Settings {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    /// The folder of the hours in the bucket; `logs` when it is not set
    pub prefix: Option<String>,
}

#[derive(my_settings_reader::SettingsModel, Serialize, Deserialize, Debug, Clone)]
pub struct SettingsModel {
    #[serde(rename = "ApiKey")]
    pub api_key: Option<String>,

    #[serde(rename = "LogsDbPath")]
    pub logs_db_path: String,

    #[serde(rename = "LogsDbArchivePath")]
    pub logs_db_archive_path: String,

    pub hours_to_gc: u64,

    #[serde(rename = "TelegramSettings")]
    pub telegram_settings: Option<TelegramSettings>,

    #[serde(rename = "ElasticSettings")]
    pub elastic: Option<ElasticSettings>,

    #[serde(rename = "S3Settings")]
    pub s3: Option<S3Settings>,

    #[serde(rename = "EnvName")]
    pub env_name: String,

    #[serde(rename = "UiUrl")]
    pub ui_url: Option<String>,

    #[serde(rename = "InsightsKeys")]
    pub insights_keys: Option<Vec<String>>,
}

impl SettingsReader {
    pub async fn get_telegram_settings(&self) -> Option<TelegramSettings> {
        let read_access = self.settings.read().await;
        read_access.telegram_settings.clone()
    }

    pub async fn get_env_name(&self) -> String {
        let read_access = self.settings.read().await;
        read_access.env_name.clone()
    }

    pub async fn get_insights_keys(&self) -> Vec<String> {
        let read_access = self.settings.read().await;
        read_access.insights_keys.clone().unwrap_or_default()
    }

    pub async fn get_elastic_settings(&self) -> Option<ElasticSettings> {
        let read_access = self.settings.read().await;
        read_access.elastic.clone()
    }

    pub async fn get_s3_settings(&self) -> Option<S3Settings> {
        let read_access = self.settings.read().await;
        read_access.s3.clone()
    }

    pub async fn get_hours_to_gc(&self) -> u64 {
        let read_access = self.settings.read().await;
        read_access.hours_to_gc
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
}
